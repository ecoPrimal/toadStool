// SPDX-License-Identifier: AGPL-3.0-or-later
//! Apply handler — apply recipes (dry-run or live BAR0).

use super::HwLearnHandler;
use super::helpers::{check_thermal_for_bdf, resolve_bdf};
use crate::pure_jsonrpc::types::{JsonRpcError, JsonRpcErrorExt};

impl HwLearnHandler {
    /// `compute.hardware.apply` — Apply a recipe (dry-run or live BAR0).
    ///
    /// Params: `{ "recipe_json": "..." }` or `{ "recipe_id": "..." }`
    ///         Optional: `{ "live": true, "bdf": "0000:65:00.0" }`
    ///
    /// When `live` is true and BAR0 is accessible, the recipe is applied
    /// directly to the GPU via MMIO register writes. Without `live`, the
    /// applicator performs a dry-run simulation.
    ///
    /// Returns: `{ "result": {...}, "mode": "live"|"dry_run" }`
    ///
    /// # Errors
    ///
    /// Returns an error if params are missing/invalid, recipe JSON is invalid,
    /// recipe store fails to open/load, BAR0 access fails, or thermal check
    /// refuses live apply.
    #[expect(
        clippy::unused_async,
        reason = "async for JSON-RPC handler trait consistency"
    )]
    pub async fn hw_learn_apply(
        &self,
        params: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let p = params.ok_or_else(|| {
            JsonRpcError::invalid_params("Expected { recipe_json } or { recipe_id } parameter")
        })?;

        let recipe = if let Some(json_str) =
            p.get("recipe_json").and_then(serde_json::Value::as_str)
        {
            hw_learn::knowledge::import_recipe(json_str)
                .map_err(|e| JsonRpcError::invalid_params(format!("Invalid recipe JSON: {e}")))?
        } else if let Some(id) = p.get("recipe_id").and_then(serde_json::Value::as_str) {
            let store = self.open_store()?;
            store
                .load(id)
                .map_err(|e| {
                    JsonRpcError::invalid_params(format!("Failed to load recipe {id}: {e}"))
                })?
                .ok_or_else(|| {
                    JsonRpcError::invalid_params(format!("No recipe found for id '{id}'"))
                })?
        } else {
            return Err(JsonRpcError::invalid_params(
                "Provide 'recipe_json' string or 'recipe_id' to load from store",
            ));
        };

        let live = p
            .get("live")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);

        let card_path = p
            .get("card_path")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(toadstool_common::constants::compute::DEFAULT_DRI_CARD);

        if live {
            let bdf = resolve_bdf(p)?;
            let mut bar0 = nvpmu::Bar0Access::open(&bdf).map_err(|e| {
                JsonRpcError::internal_error(format!(
                    "Failed to open BAR0 for {bdf}: {e}. \
                     Run setup-gpu-sovereign.sh or use sudo."
                ))
            })?;

            let thermal = check_thermal_for_bdf(&bdf);
            if let Some(ref status) = thermal
                && !status.compute_safe()
            {
                return Err(JsonRpcError::internal_error(format!(
                    "GPU {bdf} thermal status {status:?} — refusing live apply"
                )));
            }

            let mut applicator =
                hw_learn::RecipeApplicator::new(false).with_register_access(&mut bar0);
            let result = applicator.apply(&recipe, card_path);

            if result.verdict == hw_learn::applicator::ApplyVerdict::Success
                && let Ok(mut store) = self.open_store()
            {
                let _ = store.store(&recipe);
            }

            Ok(serde_json::json!({
                "domain": "compute.hardware",
                "operation": "apply",
                "mode": "live",
                "bdf": bdf,
                "thermal_checked": thermal.is_some(),
                "verdict": format!("{:?}", result.verdict),
                "steps_executed": result.steps_executed,
                "steps_total": result.steps_total,
                "result": serde_json::to_value(&result).unwrap_or_default(),
            }))
        } else {
            let mut applicator = hw_learn::RecipeApplicator::new(true);
            let result = applicator.apply(&recipe, card_path);

            Ok(serde_json::json!({
                "domain": "compute.hardware",
                "operation": "apply",
                "mode": "dry_run",
                "verdict": format!("{:?}", result.verdict),
                "steps_executed": result.steps_executed,
                "steps_total": result.steps_total,
                "result": serde_json::to_value(&result).unwrap_or_default(),
                "note": "Pass 'live: true' with BAR0 access for real register writes",
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::pure_jsonrpc::handler::hw_learn::HwLearnHandler;
    use serde_json::json;

    fn handler_with_temp_store() -> (HwLearnHandler, tempfile::TempDir) {
        let dir = tempfile::TempDir::new().unwrap();
        let handler = HwLearnHandler {
            store_dir: dir.path().to_path_buf(),
        };
        (handler, dir)
    }

    fn minimal_recipe_json() -> String {
        use hw_learn::distiller::{DriverKind, GpuArch, InitRecipe, InitStep, RegFunction, Vendor};
        let arch = GpuArch {
            vendor: Vendor::Nvidia,
            generation: "Volta".into(),
            chip: "GV100".into(),
            compute_class: "sm70".into(),
        };
        let recipe = InitRecipe {
            source_arch: arch.clone(),
            source_driver: DriverKind::Nouveau,
            target_arch: arch,
            steps: vec![InitStep::RegisterWrite {
                offset: 0x20000,
                value: 1,
                function: RegFunction::PowerGate,
            }],
            confidence: 0.0,
            description: "unit test".into(),
        };
        serde_json::to_string(&recipe).unwrap()
    }

    #[tokio::test]
    async fn missing_params_returns_error() {
        let (handler, _dir) = handler_with_temp_store();
        let err = handler.hw_learn_apply(None).await.unwrap_err();
        assert_eq!(
            err.code,
            crate::pure_jsonrpc::types::JsonRpcError::INVALID_PARAMS
        );
        assert!(err.message.contains("recipe_json") || err.message.contains("recipe_id"));
    }

    #[tokio::test]
    async fn missing_recipe_json_and_recipe_id_returns_error() {
        let (handler, _dir) = handler_with_temp_store();
        let params = json!({});
        let err = handler.hw_learn_apply(Some(&params)).await.unwrap_err();
        assert_eq!(
            err.code,
            crate::pure_jsonrpc::types::JsonRpcError::INVALID_PARAMS
        );
        assert!(err.message.contains("recipe_json") || err.message.contains("recipe_id"));
    }

    #[tokio::test]
    async fn dry_run_with_recipe_json_succeeds() {
        let (handler, _dir) = handler_with_temp_store();
        let params = json!({
            "recipe_json": minimal_recipe_json(),
        });
        let value = handler.hw_learn_apply(Some(&params)).await.unwrap();
        assert_eq!(value.get("mode"), Some(&json!("dry_run")));
        assert_eq!(value.get("domain"), Some(&json!("compute.hardware")));
        assert_eq!(value.get("operation"), Some(&json!("apply")));
        assert!(value.get("verdict").is_some());
    }

    #[tokio::test]
    async fn invalid_recipe_json_returns_error() {
        let (handler, _dir) = handler_with_temp_store();
        let params = json!({
            "recipe_json": "not valid json {{{",
        });
        let err = handler.hw_learn_apply(Some(&params)).await.unwrap_err();
        assert_eq!(
            err.code,
            crate::pure_jsonrpc::types::JsonRpcError::INVALID_PARAMS
        );
        assert!(err.message.contains("Invalid recipe JSON"));
    }

    #[tokio::test]
    async fn apply_with_recipe_id_loads_from_store() {
        let (handler, _dir) = handler_with_temp_store();
        let recipe_str = minimal_recipe_json();
        let save = json!({
            "action": "save",
            "recipe_json": recipe_str.clone(),
        });
        let saved = handler.hw_learn_share_recipe(Some(&save)).await.unwrap();
        let id = saved.get("recipe_id").and_then(|v| v.as_str()).unwrap();

        let apply_params = json!({ "recipe_id": id });
        let value = handler.hw_learn_apply(Some(&apply_params)).await.unwrap();
        assert_eq!(value.get("mode"), Some(&json!("dry_run")));
    }

    #[tokio::test]
    async fn live_apply_without_working_bar0_returns_error() {
        let (handler, _dir) = handler_with_temp_store();
        let params = json!({
            "recipe_json": minimal_recipe_json(),
            "live": true,
            "bdf": "0000:ff:00.0",
        });
        let err = handler.hw_learn_apply(Some(&params)).await.unwrap_err();
        assert_eq!(
            err.code,
            crate::pure_jsonrpc::types::JsonRpcError::INTERNAL_ERROR
        );
        assert!(
            err.message.contains("BAR0")
                || err.message.contains("GPU")
                || err.message.contains("thermal")
                || err.message.contains("refusing live apply"),
            "unexpected message: {}",
            err.message
        );
    }
}
