// SPDX-License-Identifier: AGPL-3.0-or-later
//! Status handler — report hw-learn pipeline state and firmware inventory.

use super::HwLearnHandler;
use crate::pure_jsonrpc::types::{JsonRpcError};
use tracing::warn;

impl HwLearnHandler {
    /// `compute.hardware.status` — Report hw-learn pipeline state and firmware inventory.
    ///
    /// Params: `{ "chip": "gv100" }` (optional)
    /// Returns: `{ "pipeline": ..., "firmware": ..., "recipes": ... }`
    ///
    /// # Errors
    ///
    /// This function does not return errors.
    #[expect(
        clippy::unused_async,
        reason = "async for JSON-RPC handler trait consistency"
    )]
    pub async fn hw_learn_status(
        &self,
        params: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let store = match self.open_store() {
            Ok(s) => Some(s),
            Err(e) => {
                warn!(
                    error = ?e,
                    "hw_learn_status: could not open recipe store; recipe architecture count will be zero"
                );
                None
            }
        };
        let arch_count = store.as_ref().map_or(0, |s| s.architectures().len());

        let chip = params
            .and_then(|p| p.get("chip"))
            .and_then(serde_json::Value::as_str);

        let firmware = chip.map(|c| {
            let inv = nvpmu::FirmwareInventory::probe(c);
            serde_json::json!({
                "chip": c,
                "compute_viable": inv.compute_viable(),
                "compute_blockers": inv.compute_blockers(),
                "needs_software_pmu": inv.needs_software_pmu(),
                "inventory": serde_json::to_value(&inv).unwrap_or_default(),
            })
        });

        let sysmon_gpus = toadstool_sysmon::discover_gpus();
        let gpu_count = sysmon_gpus.len();

        Ok(serde_json::json!({
            "domain": "compute.hardware",
            "operation": "status",
            "pipeline": {
                "phase": "3 — BAR0 live apply available",
                "stages": ["observe", "distill", "apply", "share_recipe"],
                "bar0_live_apply": true,
                "bar0_requires": "gpu-mmio group or root (run setup-gpu-sovereign.sh)",
            },
            "recipes": {
                "stored_architectures": arch_count,
            },
            "firmware": firmware,
            "gpus_detected": gpu_count,
        }))
    }
}

#[cfg(test)]
mod tests {
    use crate::pure_jsonrpc::handler::hw_learn::HwLearnHandler;

    fn handler_with_temp_store() -> (HwLearnHandler, tempfile::TempDir) {
        let dir = tempfile::TempDir::new().unwrap();
        let handler = HwLearnHandler {
            store_dir: dir.path().to_path_buf(),
        };
        (handler, dir)
    }

    #[tokio::test]
    async fn no_params_returns_valid_json() {
        let (handler, _dir) = handler_with_temp_store();
        let value = handler.hw_learn_status(None).await.unwrap();
        assert_eq!(
            value.get("domain"),
            Some(&serde_json::json!("compute.hardware"))
        );
        assert_eq!(value.get("operation"), Some(&serde_json::json!("status")));
    }

    #[tokio::test]
    async fn result_has_expected_fields() {
        let (handler, _dir) = handler_with_temp_store();
        let value = handler.hw_learn_status(None).await.unwrap();
        assert!(value.get("pipeline").is_some());
        let pipeline = value.get("pipeline").unwrap();
        assert!(pipeline.get("phase").is_some());
        assert!(value.get("recipes").is_some());
        let recipes = value.get("recipes").unwrap();
        assert!(recipes.get("stored_architectures").is_some());
        assert!(value.get("gpus_detected").is_some());
    }

    #[tokio::test]
    async fn optional_chip_param_still_succeeds() {
        let (handler, _dir) = handler_with_temp_store();
        let params = serde_json::json!({ "chip": "gv100" });
        let value = handler.hw_learn_status(Some(&params)).await.unwrap();
        assert_eq!(
            value.get("domain"),
            Some(&serde_json::json!("compute.hardware"))
        );
        assert!(value.get("firmware").is_some());
    }
}
