// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::pure_jsonrpc::types::{JsonRpcError, JsonRpcErrorExt};
use serde_json::Value;
use tracing::info;

use super::DEFAULT_BAR0_SIZE;
use super::snapshot::sovereign_snapshot;

/// `sovereign.kernel_health` — kernel build environment health check.
///
/// Runs a 3-layer detection (autoconf freshness, struct layout probe,
/// reference cross-check) and returns a full health report. Optionally
/// attempts repair if `repair` is set to `true`.
///
/// Params:
/// - `repair` (optional, bool): attempt to repair via .deb cache if unhealthy
/// - `repair_strategy` (optional, string): `"PackageRestore"` (default) or `"PackageReinstall"`
pub fn sovereign_kernel_health(params: Option<&Value>) -> Result<Value, JsonRpcError> {
    use toadstool_cylinder::vfio::kernel_health;

    let report = kernel_health::full_kernel_health_check()
        .map_err(|e| JsonRpcError::internal_error(format!("kernel health check failed: {e}")))?;

    let should_repair = params
        .and_then(|p| p.get("repair"))
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let mut repair_result = None;

    if should_repair && !report.layout_matches {
        let strategy = params
            .and_then(|p| p.get("repair_strategy"))
            .and_then(Value::as_str)
            .unwrap_or("PackageRestore");

        let strat = match strategy {
            "PackageReinstall" => kernel_health::RepairStrategy::PackageReinstall,
            _ => kernel_health::RepairStrategy::PackageRestore,
        };

        match kernel_health::repair_autoconf(strat) {
            Ok(path) => {
                info!(path = %path.display(), "autoconf.h repaired");
                repair_result = Some(serde_json::json!({
                    "success": true,
                    "restored_path": path.display().to_string(),
                    "strategy": strategy,
                }));
            }
            Err(e) => {
                repair_result = Some(serde_json::json!({
                    "success": false,
                    "error": e.to_string(),
                    "strategy": strategy,
                }));
            }
        }
    }

    let report_json = serde_json::to_value(&report)
        .map_err(|e| JsonRpcError::internal_error(format!("serialization: {e}")))?;

    Ok(serde_json::json!({
        "report": report_json,
        "repair": repair_result,
    }))
}

/// `sovereign.reagent_capture` — Capture firmware reagents while nvidia is loaded.
///
/// Orchestrates the full reagent capture pipeline:
/// 1. Probes nvidia state (driver bound, FECS/TPC liveness)
/// 2. Catalogs linux-firmware blobs for the chip
/// 3. Optionally distills mmiotrace to ACR recipe
/// 4. Copies existing catalyst artifacts
/// 5. Persists ReagentManifest to storage
///
/// Params:
/// - `bdf` (required): PCI BDF address (e.g. `"0000:41:00.0"`)
/// - `chip` (optional): GPU chip identifier — discovered from BOOT0 when omitted
/// - `driver_version` (optional): nvidia driver version — discovered from procfs when omitted
/// - `mmiotrace_path` (optional): Path to mmiotrace log for ACR recipe distillation
pub fn sovereign_reagent_capture(params: Option<&Value>) -> Result<Value, JsonRpcError> {
    use toadstool_cylinder::nv::generation::VOLTA;
    use toadstool_cylinder::vfio::reagent;

    let bdf = params
        .and_then(|p| p.get("bdf"))
        .and_then(Value::as_str)
        .ok_or_else(|| JsonRpcError::invalid_params("Missing 'bdf' string parameter"))?;

    let chip = params
        .and_then(|p| p.get("chip"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| reagent::discover_chip_from_bdf(bdf).map(str::to_owned))
        .unwrap_or_else(|| {
            // DISCOVERY: Volta GV100 default when BOOT0 read is unavailable
            VOLTA.firmware_chip.to_owned()
        });

    let driver_version = params
        .and_then(|p| p.get("driver_version"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(reagent::discover_nvidia_driver_version)
        .unwrap_or_else(|| {
            // DISCOVERY: nvidia-470 default when /proc/driver/nvidia/version is unavailable
            reagent::DEFAULT_REAGENT_DRIVER_VERSION.to_owned()
        });

    let mmiotrace_path = params
        .and_then(|p| p.get("mmiotrace_path"))
        .and_then(Value::as_str);

    info!(
        bdf = bdf,
        chip = chip.as_str(),
        driver_version = driver_version.as_str(),
        mmiotrace_path = ?mmiotrace_path,
        "sovereign.reagent_capture: starting capture pipeline"
    );

    let mut result = reagent::execute_reagent_capture(bdf, chip.as_str(), driver_version.as_str());

    // Optionally distill mmiotrace
    if let Some(trace_path) = mmiotrace_path {
        let trace = std::path::Path::new(trace_path);
        if trace.exists() {
            let output = result.manifest.store_path().join("mmiotrace").join(format!(
                "{}_recipe.json",
                trace
                    .file_stem()
                    .map_or_else(|| "trace".to_owned(), |s| s.to_string_lossy().to_string())
            ));
            match reagent::distill_mmiotrace_to_reagent(trace, &output) {
                Ok(summary) => {
                    result.manifest.mmiotrace_recipe = Some(output);
                    result.manifest.completeness.mmiotrace_recipe = true;
                    result.strategy_results.insert(
                        "mmiotrace_distill".to_owned(),
                        reagent::StrategyResult {
                            success: true,
                            detail: format!(
                                "{} writes → {} recipe steps ({} ACR-relevant)",
                                summary.total_writes, summary.recipe_steps, summary.acr_steps
                            ),
                            artifacts: std::collections::HashMap::from([
                                ("recipe".to_owned(), summary.recipe_steps as u64),
                                ("acr_subset".to_owned(), summary.acr_steps as u64),
                            ]),
                        },
                    );
                }
                Err(e) => {
                    result.strategy_results.insert(
                        "mmiotrace_distill".to_owned(),
                        reagent::StrategyResult {
                            success: false,
                            detail: format!("Distillation failed: {e}"),
                            artifacts: std::collections::HashMap::new(),
                        },
                    );
                }
            }
        }
    }

    // Re-persist with updated mmiotrace data
    if let Ok(path) = result.manifest.persist() {
        result.manifest_path = Some(path);
    }

    let strategy_summary: Vec<_> = result
        .strategy_results
        .iter()
        .map(|(name, r)| {
            serde_json::json!({
                "strategy": name,
                "success": r.success,
                "detail": r.detail,
            })
        })
        .collect();

    Ok(serde_json::json!({
        "chip": result.manifest.chip,
        "bdf": bdf,
        "driver_version": driver_version.as_str(),
        "captured_at": result.manifest.captured_at,
        "manifest_path": result.manifest_path,
        "completeness": {
            "fraction": result.manifest.completeness.fraction(),
            "bar0_snapshot": result.manifest.completeness.bar0_snapshot,
            "linux_firmware": result.manifest.completeness.linux_firmware,
            "mmiotrace_recipe": result.manifest.completeness.mmiotrace_recipe,
            "falcon_firmware": result.manifest.completeness.falcon_firmware,
            "patch_set": result.manifest.completeness.patch_set,
            "vram_firmware": result.manifest.completeness.vram_firmware,
        },
        "linux_firmware_blobs": result.manifest.firmware.linux_firmware_blobs.len(),
        "strategies": strategy_summary,
    }))
}

/// `sovereign.recipe_replay` — Replay a captured RecipeStep JSON on a VFIO GPU.
///
/// Loads a recipe (distilled mmiotrace or BAR0 diff) and writes every step
/// to BAR0 in order via VFIO.  Post-replay validates PTIMER and PMC_BOOT_0,
/// then takes a sovereign snapshot to show tier advancement.
///
/// Params:
/// - `bdf`         (required): PCI BDF address (e.g. `"0000:02:00.0"`)
/// - `recipe_path` (required): Absolute path to a `RecipeStep[]` JSON file
/// - `snapshot`    (optional, default true): Take a sovereign snapshot after replay
pub fn sovereign_recipe_replay(params: Option<&Value>) -> Result<Value, JsonRpcError> {
    use toadstool_cylinder::vfio::channel::diagnostic::replay;

    let bdf = params
        .and_then(|p| p.get("bdf"))
        .and_then(Value::as_str)
        .ok_or_else(|| JsonRpcError::invalid_params("Missing 'bdf' string parameter"))?;

    let recipe_path = params
        .and_then(|p| p.get("recipe_path"))
        .and_then(Value::as_str)
        .ok_or_else(|| JsonRpcError::invalid_params("Missing 'recipe_path' string parameter"))?;

    let take_snapshot = params
        .and_then(|p| p.get("snapshot"))
        .and_then(Value::as_bool)
        .unwrap_or(true);

    info!(bdf, recipe_path, "sovereign.recipe_replay: loading recipe");

    let recipe = replay::load_recipe(std::path::Path::new(recipe_path))
        .map_err(|e| JsonRpcError::internal_error(format!("Failed to load recipe: {e}")))?;

    info!(
        bdf,
        steps = recipe.len(),
        "sovereign.recipe_replay: opening sysfs BAR0"
    );

    let bar0 = toadstool_cylinder::vfio::device::MappedBar::from_sysfs_rw(bdf, DEFAULT_BAR0_SIZE)
        .map_err(|e| {
        JsonRpcError::internal_error(format!("BAR0 open failed for {bdf}: {e}"))
    })?;

    info!(
        bdf,
        steps = recipe.len(),
        "sovereign.recipe_replay: applying recipe"
    );

    let result = replay::apply_recipe_to_bar0(&bar0, &recipe)
        .map_err(|e| JsonRpcError::internal_error(format!("Replay failed: {e}")))?;

    let domain_summary: serde_json::Map<String, Value> = result
        .domain_counts
        .iter()
        .map(|(k, (ok, fail))| {
            (
                k.clone(),
                serde_json::json!({"applied": ok, "failed": fail}),
            )
        })
        .collect();

    let mut resp = serde_json::json!({
        "bdf": bdf,
        "recipe_path": recipe_path,
        "total_steps": recipe.len(),
        "applied": result.applied,
        "failed": result.failed,
        "pmc_boot_0": format!("{:#010x}", result.pmc_boot_0),
        "ptimer_ticking": result.ptimer_ticking,
        "is_alive": result.is_alive(),
        "domains": domain_summary,
    });

    if take_snapshot && let Some(obj) = resp.as_object_mut() {
        match sovereign_snapshot(Some(&serde_json::json!({"bdf": bdf}))) {
            Ok(snap) => {
                obj.insert("post_snapshot".to_owned(), snap);
            }
            Err(e) => {
                obj.insert(
                    "snapshot_error".to_owned(),
                    Value::String(e.message.to_string()),
                );
            }
        }
    }

    Ok(resp)
}

/// `sovereign.runtime_services_probe` — Probe nvidia's live state for runtime services.
///
/// When nvidia stays bound as a runtime compute service, this RPC probes what
/// nvidia has established: driver binding, FECS context, TPC stations, channels.
///
/// Params:
/// - `bdf` (required): PCI BDF address (e.g. `"0000:41:00.0"`)
pub fn sovereign_runtime_services_probe(params: Option<&Value>) -> Result<Value, JsonRpcError> {
    use toadstool_cylinder::vfio::sovereign_handoff;

    let bdf = params
        .and_then(|p| p.get("bdf"))
        .and_then(Value::as_str)
        .ok_or_else(|| JsonRpcError::invalid_params("Missing 'bdf' string parameter"))?;

    info!(
        bdf = bdf,
        "sovereign.runtime_services_probe: probing nvidia state"
    );

    let probe = sovereign_handoff::probe_runtime_services(bdf);

    Ok(serde_json::json!({
        "bdf": probe.bdf,
        "driver": probe.driver,
        "nvidia_loaded": probe.nvidia_loaded,
        "fecs_state": probe.fecs_state,
        "tpc_alive": probe.tpc_alive,
        "nvidia_channels": probe.nvidia_channels,
        "runtime_services_ready": probe.nvidia_loaded && probe.tpc_alive,
    }))
}
