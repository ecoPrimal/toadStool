// SPDX-License-Identifier: AGPL-3.0-or-later
//! Share recipe handler — save, load, or list recipes.

use super::HwLearnHandler;
use crate::pure_jsonrpc::types::{JsonRpcError, JsonRpcErrorExt};

impl HwLearnHandler {
    /// `compute.hardware.share_recipe` — Save, load, or list recipes.
    ///
    /// Save: `{ "action": "save", "recipe_json": "..." }`
    /// Load: `{ "action": "load", "recipe_id": "..." }`
    /// List: `{ "action": "list" }`
    ///
    /// # Errors
    ///
    /// Returns an error if params are missing/invalid, recipe JSON is invalid
    /// (save), store fails to open/save/load, recipe not found (load), or
    /// action is unknown.
    #[expect(
        clippy::unused_async,
        reason = "async for JSON-RPC handler trait consistency"
    )]
    pub async fn hw_learn_share_recipe(
        &self,
        params: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let p = params
            .ok_or_else(|| JsonRpcError::invalid_params("Expected { action, ... } parameter"))?;

        let action = p
            .get("action")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("list");

        match action {
            "save" => {
                let json_str = p
                    .get("recipe_json")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| {
                        JsonRpcError::invalid_params("Missing 'recipe_json' for save action")
                    })?;
                let recipe = hw_learn::knowledge::import_recipe(json_str).map_err(|e| {
                    JsonRpcError::invalid_params(format!("Invalid recipe JSON: {e}"))
                })?;
                let mut store = self.open_store()?;
                let id = store.store(&recipe).map_err(|e| {
                    JsonRpcError::internal_error(format!("Failed to save recipe: {e}"))
                })?;
                Ok(serde_json::json!({
                    "domain": "compute.hardware",
                    "operation": "share_recipe",
                    "action": "save",
                    "recipe_id": id,
                }))
            }
            "load" => {
                let id = p
                    .get("recipe_id")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| {
                        JsonRpcError::invalid_params("Missing 'recipe_id' for load action")
                    })?;
                let store = self.open_store()?;
                let recipe = store
                    .load(id)
                    .map_err(|e| {
                        JsonRpcError::invalid_params(format!("Failed to load recipe {id}: {e}"))
                    })?
                    .ok_or_else(|| {
                        JsonRpcError::invalid_params(format!("No recipe found for id '{id}'"))
                    })?;
                Ok(serde_json::json!({
                    "domain": "compute.hardware",
                    "operation": "share_recipe",
                    "action": "load",
                    "recipe_id": id,
                    "recipe": serde_json::to_value(&recipe).unwrap_or_default(),
                }))
            }
            "list" => {
                let store = self.open_store()?;
                let archs = store.architectures();
                let entries: Vec<serde_json::Value> = archs
                    .iter()
                    .map(|arch| {
                        serde_json::json!({
                            "arch": format!("{}", arch),
                        })
                    })
                    .collect();
                Ok(serde_json::json!({
                    "domain": "compute.hardware",
                    "operation": "share_recipe",
                    "action": "list",
                    "architectures": entries,
                    "count": entries.len(),
                }))
            }
            _ => Err(JsonRpcError::invalid_params(format!(
                "Unknown action '{action}'. Expected 'save', 'load', or 'list'"
            ))),
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
            description: "share test".into(),
        };
        serde_json::to_string(&recipe).unwrap()
    }

    #[tokio::test]
    async fn missing_params_returns_error() {
        let (handler, _dir) = handler_with_temp_store();
        let err = handler.hw_learn_share_recipe(None).await.unwrap_err();
        assert_eq!(
            err.code,
            crate::pure_jsonrpc::types::JsonRpcError::INVALID_PARAMS
        );
        assert!(err.message.contains("action"));
    }

    #[tokio::test]
    async fn unknown_action_returns_error() {
        let (handler, _dir) = handler_with_temp_store();
        let params = json!({ "action": "nope" });
        let err = handler
            .hw_learn_share_recipe(Some(&params))
            .await
            .unwrap_err();
        assert_eq!(
            err.code,
            crate::pure_jsonrpc::types::JsonRpcError::INVALID_PARAMS
        );
        assert!(err.message.contains("Unknown action"));
    }

    #[tokio::test]
    async fn list_empty_store_works() {
        let (handler, _dir) = handler_with_temp_store();
        let params = json!({ "action": "list" });
        let value = handler.hw_learn_share_recipe(Some(&params)).await.unwrap();
        assert_eq!(value.get("action"), Some(&json!("list")));
        assert_eq!(value.get("domain"), Some(&json!("compute.hardware")));
        let count = value
            .get("count")
            .and_then(serde_json::Value::as_u64)
            .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn save_and_load_roundtrip() {
        let (handler, _dir) = handler_with_temp_store();
        let recipe_json = minimal_recipe_json();
        let save = json!({
            "action": "save",
            "recipe_json": recipe_json,
        });
        let saved = handler.hw_learn_share_recipe(Some(&save)).await.unwrap();
        let id = saved.get("recipe_id").and_then(|v| v.as_str()).unwrap();

        let load = json!({
            "action": "load",
            "recipe_id": id,
        });
        let loaded = handler.hw_learn_share_recipe(Some(&load)).await.unwrap();
        assert_eq!(loaded.get("action"), Some(&json!("load")));
        assert_eq!(loaded.get("recipe_id"), Some(&json!(id)));
        let recipe = loaded.get("recipe").expect("recipe");
        assert_eq!(recipe.get("description"), Some(&json!("share test")));
    }
}
