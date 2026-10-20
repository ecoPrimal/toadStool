// SPDX-License-Identifier: AGPL-3.0-or-later
//! Auto-init handlers — auto-detect GPU, find best recipe, apply (single or all).

use super::HwLearnHandler;
use super::helpers::check_thermal_for_bdf;
use crate::pure_jsonrpc::types::{JsonRpcError, JsonRpcErrorExt};

/// Per-GPU init work (runs in spawn_blocking when parallel).
fn init_one_gpu(
    gpu: &nvpmu::pci::NvidiaGpu,
    card_path: &str,
    recipe: &hw_learn::distiller::InitRecipe,
    dry_run: bool,
    store_dir: &std::path::Path,
) -> serde_json::Value {
    let bdf = gpu.bdf.clone();
    let chip = gpu.chip.as_deref().unwrap_or("unknown");

    if dry_run {
        let mut applicator = hw_learn::RecipeApplicator::new(true);
        let result = applicator.apply(recipe, card_path);
        return serde_json::json!({
            "bdf": bdf,
            "chip": chip,
            "driver": gpu.driver,
            "status": "dry_run",
            "verdict": format!("{:?}", result.verdict),
            "steps_total": result.steps_total,
        });
    }

    let thermal = check_thermal_for_bdf(&bdf);
    if let Some(ref status) = thermal
        && !status.compute_safe()
    {
        return serde_json::json!({
            "bdf": bdf,
            "chip": chip,
            "driver": gpu.driver,
            "status": "skipped",
            "reason": format!("thermal {:?} — refusing init", status),
        });
    }

    let Ok(mut bar0) = nvpmu::Bar0Access::open(&bdf) else {
        return serde_json::json!({
            "bdf": bdf,
            "chip": chip,
            "driver": gpu.driver,
            "status": "failed",
            "reason": "BAR0 open failed — run setup-gpu-sovereign.sh",
        });
    };

    let snapshot = nvpmu::RegisterSnapshot::capture(recipe, &bar0);
    let mut applicator = hw_learn::RecipeApplicator::new(false).with_register_access(&mut bar0);
    let result = applicator.apply(recipe, card_path);

    let confidence = match result.verdict {
        hw_learn::applicator::ApplyVerdict::Success => 1.0,
        hw_learn::applicator::ApplyVerdict::PartialSuccess => 0.5,
        _ => 0.0,
    };

    if result.verdict != hw_learn::applicator::ApplyVerdict::Success && !snapshot.is_empty() {
        let _ = snapshot.rollback(&mut bar0);
    }

    if let Ok(mut s) = hw_learn::knowledge::KnowledgeStore::open(store_dir)
        && let Some(id) = s.best_recipe(&hw_learn::distiller::GpuArch {
            vendor: hw_learn::distiller::Vendor::Nvidia,
            generation: String::new(),
            chip: chip.to_string(),
            compute_class: String::new(),
        })
    {
        let _ = s.update_confidence(&id, confidence);
    }

    serde_json::json!({
        "bdf": bdf,
        "chip": chip,
        "driver": gpu.driver,
        "status": if result.verdict == hw_learn::applicator::ApplyVerdict::Success {
            "succeeded"
        } else {
            "failed"
        },
        "verdict": format!("{:?}", result.verdict),
        "steps_executed": result.steps_executed,
        "steps_total": result.steps_total,
        "thermal_checked": thermal.is_some(),
    })
}

impl HwLearnHandler {
    /// `compute.hardware.auto_init` — Auto-detect GPU, find best recipe, apply.
    ///
    /// This wires Gap 5 end-to-end: discover GPU → knowledge store → BAR0 apply.
    ///
    /// Params: `{ "bdf": "..." }` (optional, auto-detects if omitted)
    ///         `{ "dry_run": true }` (optional, default false)
    /// Returns: `{ "gpu": ..., "recipe_id": ..., "result": ... }`
    ///
    /// # Errors
    ///
    /// Returns an error if GPU discovery fails, no GPUs found, specified BDF
    /// not found, no recipe for chip, recipe load fails, thermal check refuses
    /// live apply, or BAR0 access fails.
    #[expect(
        clippy::unused_async,
        reason = "async for JSON-RPC handler trait consistency"
    )]
    pub async fn hw_learn_auto_init(
        &self,
        params: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let gpus = nvpmu::pci::discover_gpus()
            .map_err(|e| JsonRpcError::internal_error(format!("GPU discovery failed: {e}")))?;

        if gpus.is_empty() {
            return Err(JsonRpcError::internal_error("No NVIDIA GPUs found"));
        }

        let bdf = params
            .and_then(|p| p.get("bdf"))
            .and_then(serde_json::Value::as_str)
            .map_or_else(|| gpus[0].bdf.clone(), String::from);

        let gpu = gpus.iter().find(|g| g.bdf == bdf).ok_or_else(|| {
            JsonRpcError::invalid_params(format!("GPU {bdf} not found in PCI scan"))
        })?;

        let chip = gpu.chip.as_deref().unwrap_or("unknown");
        let target_arch = hw_learn::distiller::GpuArch {
            vendor: hw_learn::distiller::Vendor::Nvidia,
            generation: String::new(),
            chip: chip.to_string(),
            compute_class: String::new(),
        };

        let store = self.open_store()?;
        let recipe_id = store.best_recipe(&target_arch).ok_or_else(|| {
            JsonRpcError::internal_error(format!(
                "No recipe found for {chip}. Run compute.hardware.distill first \
                 to create a recipe from mmiotraces."
            ))
        })?;

        let recipe = store
            .load(&recipe_id)
            .map_err(|e| {
                JsonRpcError::internal_error(format!("Failed to load recipe {recipe_id}: {e}"))
            })?
            .ok_or_else(|| {
                JsonRpcError::internal_error(format!("Recipe {recipe_id} disappeared from store"))
            })?;

        let dry_run = params
            .and_then(|p| p.get("dry_run"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);

        let default_card = toadstool_sysmon::gpu::discover_gpus()
            .first()
            .map(|g| g.card_path().to_string_lossy().into_owned());
        let card_path_owned;
        let card_path = if let Some(p) = params
            .and_then(|p| p.get("card_path"))
            .and_then(serde_json::Value::as_str)
        {
            p
        } else {
            card_path_owned = default_card.unwrap_or_else(|| {
                toadstool_common::constants::compute::DEFAULT_DRI_CARD.to_string()
            });
            &card_path_owned
        };

        if dry_run {
            let mut applicator = hw_learn::RecipeApplicator::new(true);
            let result = applicator.apply(&recipe, card_path);

            return Ok(serde_json::json!({
                "domain": "compute.hardware",
                "operation": "auto_init",
                "mode": "dry_run",
                "gpu": { "bdf": bdf, "chip": chip },
                "recipe_id": recipe_id,
                "verdict": format!("{:?}", result.verdict),
                "steps_total": result.steps_total,
            }));
        }

        let thermal = check_thermal_for_bdf(&bdf);
        if let Some(ref status) = thermal
            && !status.compute_safe()
        {
            return Err(JsonRpcError::internal_error(format!(
                "GPU {bdf} thermal status {status:?} — refusing auto_init"
            )));
        }

        let mut bar0 = nvpmu::Bar0Access::open(&bdf).map_err(|e| {
            JsonRpcError::internal_error(format!(
                "BAR0 open failed for {bdf}: {e}. Run setup-gpu-sovereign.sh."
            ))
        })?;

        let snapshot = nvpmu::RegisterSnapshot::capture(&recipe, &bar0);

        let mut applicator = hw_learn::RecipeApplicator::new(false).with_register_access(&mut bar0);
        let result = applicator.apply(&recipe, card_path);

        let confidence = match result.verdict {
            hw_learn::applicator::ApplyVerdict::Success => 1.0,
            hw_learn::applicator::ApplyVerdict::PartialSuccess => 0.5,
            _ => 0.0,
        };

        let rollback_info = if result.verdict != hw_learn::applicator::ApplyVerdict::Success
            && !snapshot.is_empty()
        {
            tracing::warn!(bdf = %bdf, "auto_init failed — attempting rollback");
            let rollback_ok = snapshot.rollback(&mut bar0);
            serde_json::json!({
                "attempted": true,
                "succeeded": rollback_ok,
                "registers": snapshot.len(),
            })
        } else {
            serde_json::json!(null)
        };

        if let Ok(mut store) = self.open_store() {
            let _ = store.update_confidence(&recipe_id, confidence);
        }

        Ok(serde_json::json!({
            "domain": "compute.hardware",
            "operation": "auto_init",
            "mode": "live",
            "gpu": { "bdf": bdf, "chip": chip, "driver": gpu.driver },
            "recipe_id": recipe_id,
            "thermal_checked": thermal.is_some(),
            "verdict": format!("{:?}", result.verdict),
            "steps_executed": result.steps_executed,
            "steps_total": result.steps_total,
            "confidence_updated": confidence,
            "rollback": rollback_info,
            "result": serde_json::to_value(&result).unwrap_or_default(),
        }))
    }

    /// `compute.hardware.auto_init_all` — Initialize all detected NVIDIA GPUs.
    ///
    /// Discovers all GPUs, finds best recipes, applies in parallel with
    /// topology awareness. Reports per-GPU results.
    ///
    /// Params: `{ "dry_run": true }` (optional, default false)
    ///         `{ "parallel": true }` (optional, default true)
    /// Returns: `{ "gpus": [...], "total": N, "succeeded": N }`
    ///
    /// # Errors
    ///
    /// Returns an error if GPU discovery fails or the recipe store fails to open.
    pub async fn hw_learn_auto_init_all(
        &self,
        params: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let gpus = nvpmu::pci::discover_gpus()
            .map_err(|e| JsonRpcError::internal_error(format!("GPU discovery failed: {e}")))?;

        if gpus.is_empty() {
            return Ok(serde_json::json!({
                "domain": "compute.hardware",
                "operation": "auto_init_all",
                "gpus": [],
                "total": 0,
                "succeeded": 0,
                "failed": 0,
            }));
        }

        let dry_run = params
            .and_then(|p| p.get("dry_run"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);

        let parallel = params
            .and_then(|p| p.get("parallel"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true);

        let sysmon_gpus = toadstool_sysmon::discover_gpus();

        let store = self.open_store()?;

        let target_arch_for_gpu = |gpu: &nvpmu::pci::NvidiaGpu| {
            let chip = gpu.chip.as_deref().unwrap_or("unknown");
            hw_learn::distiller::GpuArch {
                vendor: hw_learn::distiller::Vendor::Nvidia,
                generation: String::new(),
                chip: chip.to_string(),
                compute_class: String::new(),
            }
        };

        let card_path_for_bdf = |bdf: &str| -> String {
            sysmon_gpus
                .iter()
                .find(|g| g.pci_slot == bdf)
                .or_else(|| sysmon_gpus.first())
                .map_or_else(
                    || toadstool_common::constants::compute::DEFAULT_DRI_CARD.to_string(),
                    |g| g.card_path().to_string_lossy().into_owned(),
                )
        };

        let store_dir = self.store_dir();

        let gpu_results: Vec<serde_json::Value> = if parallel && !dry_run {
            let handles: Vec<_> = gpus
                .iter()
                .filter_map(|gpu| {
                    let recipe_id = store.best_recipe(&target_arch_for_gpu(gpu))?;
                    let recipe = store.load(&recipe_id).ok().flatten()?;
                    let card_path = card_path_for_bdf(&gpu.bdf);
                    let gpu = gpu.clone();
                    let store_dir = store_dir.clone();
                    Some(tokio::task::spawn_blocking(move || {
                        init_one_gpu(&gpu, &card_path, &recipe, false, &store_dir)
                    }))
                })
                .collect();

            let mut results = Vec::with_capacity(handles.len());
            for h in handles {
                match h.await {
                    Ok(r) => results.push(r),
                    Err(e) => results.push(serde_json::json!({
                        "status": "failed",
                        "reason": format!("task join error: {e}"),
                    })),
                }
            }
            results
        } else if parallel && dry_run {
            let handles: Vec<_> = gpus
                .iter()
                .filter_map(|gpu| {
                    let recipe_id = store.best_recipe(&target_arch_for_gpu(gpu))?;
                    let recipe = store.load(&recipe_id).ok().flatten()?;
                    let card_path = card_path_for_bdf(&gpu.bdf);
                    let gpu = gpu.clone();
                    let store_dir = store_dir.clone();
                    Some(tokio::task::spawn_blocking(move || {
                        init_one_gpu(&gpu, &card_path, &recipe, true, &store_dir)
                    }))
                })
                .collect();

            let mut results = Vec::with_capacity(handles.len());
            for h in handles {
                match h.await {
                    Ok(r) => results.push(r),
                    Err(e) => results.push(serde_json::json!({
                        "status": "failed",
                        "reason": format!("task join error: {e}"),
                    })),
                }
            }
            results
        } else {
            gpus.iter()
                .filter_map(|gpu| {
                    let recipe_id = store.best_recipe(&target_arch_for_gpu(gpu))?;
                    let recipe = store.load(&recipe_id).ok().flatten()?;
                    let card_path = card_path_for_bdf(&gpu.bdf);
                    Some(init_one_gpu(gpu, &card_path, &recipe, dry_run, &store_dir))
                })
                .collect()
        };

        // Add skipped entries for GPUs without recipes
        let mut all_results = gpu_results;
        for gpu in &gpus {
            let chip = gpu.chip.as_deref().unwrap_or("unknown");
            if store.best_recipe(&target_arch_for_gpu(gpu)).is_none() {
                all_results.push(serde_json::json!({
                    "bdf": gpu.bdf,
                    "chip": chip,
                    "driver": gpu.driver,
                    "status": "skipped",
                    "reason": "no recipe — run compute.hardware.distill first",
                }));
            }
        }

        let succeeded = all_results
            .iter()
            .filter(|r| r.get("status").and_then(serde_json::Value::as_str) == Some("succeeded"))
            .count();
        let failed = all_results
            .iter()
            .filter(|r| r.get("status").and_then(serde_json::Value::as_str) == Some("failed"))
            .count();

        Ok(serde_json::json!({
            "domain": "compute.hardware",
            "operation": "auto_init_all",
            "gpus": all_results,
            "total": all_results.len(),
            "succeeded": succeeded,
            "failed": failed,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::super::HwLearnHandler;
    use hw_learn::distiller::{DriverKind, GpuArch, InitRecipe, Vendor};
    use nvpmu::pci::NvidiaGpu;
    use std::path::PathBuf;
    use tempfile::tempdir;

    fn minimal_recipe() -> InitRecipe {
        InitRecipe {
            source_arch: GpuArch {
                vendor: Vendor::Nvidia,
                generation: String::new(),
                chip: "test-chip".to_string(),
                compute_class: String::new(),
            },
            source_driver: DriverKind::Nouveau,
            target_arch: GpuArch {
                vendor: Vendor::Nvidia,
                generation: String::new(),
                chip: "test-chip".to_string(),
                compute_class: String::new(),
            },
            steps: vec![],
            confidence: 0.0,
            description: "unit test recipe".to_string(),
        }
    }

    fn dummy_gpu() -> NvidiaGpu {
        NvidiaGpu {
            bdf: "0000:01:00.0".to_string(),
            vendor_id: 0x10de,
            device_id: 0x1234,
            class_code: 0x0003_0200,
            sysfs_path: PathBuf::from("/sys/bus/pci/devices/0000:01:00.0"),
            driver: Some("nouveau".to_string()),
            chip: Some("test-chip".to_string()),
        }
    }

    #[test]
    fn test_init_one_gpu_dry_run_returns_dry_run_status() {
        let gpu = dummy_gpu();
        let recipe = minimal_recipe();
        let dir = tempdir().unwrap();
        let v = super::init_one_gpu(&gpu, "/dev/dri/card0", &recipe, true, dir.path());
        assert_eq!(v["status"], "dry_run");
        assert_eq!(v["bdf"], "0000:01:00.0");
        assert!(v.get("verdict").is_some());
        assert!(v.get("steps_total").is_some());
    }

    #[tokio::test]
    async fn test_hw_learn_auto_init_all_empty_result_shape() {
        let handler = HwLearnHandler::new();
        let v = handler.hw_learn_auto_init_all(None).await.unwrap();
        assert_eq!(v["domain"], "compute.hardware");
        assert_eq!(v["operation"], "auto_init_all");
        assert!(v["gpus"].is_array());
    }

    #[tokio::test]
    async fn test_hw_learn_auto_init_errors_without_hardware_or_recipe() {
        let handler = HwLearnHandler::new();
        let r = handler.hw_learn_auto_init(None).await;
        assert!(r.is_err());
    }
}
