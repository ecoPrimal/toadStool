// SPDX-License-Identifier: AGPL-3.0-or-later
//! Observe and distill handlers — parse mmiotraces, diff traces, build init recipes.

use super::HwLearnHandler;
use super::helpers::observe_from_text;
use crate::pure_jsonrpc::types::{JsonRpcError, JsonRpcErrorExt};

impl HwLearnHandler {
    /// `compute.hardware.observe` — Parse an mmiotrace into structured events.
    ///
    /// Params: `{ "trace_data": "<mmiotrace text>", "mode": "mmiotrace" }`
    /// Returns: `{ "events_count": N, "gpu_id": ..., "driver": ... }`
    ///
    /// # Errors
    ///
    /// Returns an error if `trace_data` is missing or mmiotrace parsing fails.
    #[expect(
        clippy::unused_async,
        reason = "async for JSON-RPC handler trait consistency"
    )]
    pub async fn hw_learn_observe(
        &self,
        params: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let trace_data = params
            .and_then(|p| p.get("trace_data"))
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                JsonRpcError::invalid_params("Missing required 'trace_data' string parameter")
            })?;

        let result = observe_from_text(trace_data, "trace")?;

        Ok(serde_json::json!({
            "domain": "compute.hardware",
            "operation": "observe",
            "events_count": result.events.len(),
            "gpu_id": result.gpu_id,
            "driver": result.driver,
            "compute_triggered": result.compute_triggered,
            "duration_us": result.duration_us,
        }))
    }

    /// `compute.hardware.distill` — Diff baseline vs compute traces, build init recipe.
    ///
    /// Params: `{ "baseline": "<mmiotrace>", "compute": "<mmiotrace>", "chip": "gv100" }`
    /// Returns: `{ "recipe": {...}, "diff_count": N }`
    ///
    /// # Errors
    ///
    /// Returns an error if `baseline`, `compute`, or `chip` params are missing,
    /// or if mmiotrace parsing fails for either trace.
    #[expect(
        clippy::unused_async,
        reason = "async for JSON-RPC handler trait consistency"
    )]
    pub async fn hw_learn_distill(
        &self,
        params: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let p = params.ok_or_else(|| {
            JsonRpcError::invalid_params("Expected { baseline, compute, chip } parameters")
        })?;

        let baseline_text = p
            .get("baseline")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| JsonRpcError::invalid_params("Missing 'baseline' trace data"))?;

        let compute_text = p
            .get("compute")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| JsonRpcError::invalid_params("Missing 'compute' trace data"))?;

        let chip = p
            .get("chip")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| JsonRpcError::invalid_params("Missing 'chip' codename"))?;

        let baseline = observe_from_text(baseline_text, "baseline")?;
        let compute = observe_from_text(compute_text, "compute")?;

        let diff = hw_learn::distiller::diff_traces(&baseline.events, &compute.events);
        let target_arch = hw_learn::distiller::GpuArch {
            vendor: hw_learn::distiller::Vendor::Nvidia,
            generation: String::new(),
            chip: chip.to_string(),
            compute_class: String::new(),
        };
        let recipe =
            hw_learn::distiller::RecipeDistiller::distill(&compute, Some(&baseline), target_arch);

        Ok(serde_json::json!({
            "domain": "compute.hardware",
            "operation": "distill",
            "chip": chip,
            "diff_count": diff.len(),
            "recipe_steps": recipe.steps.len(),
            "recipe": serde_json::to_value(&recipe).unwrap_or_default(),
        }))
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

    /// Minimal mmiotrace line (see `hw_learn::observer::mmio_trace` tests).
    fn minimal_mmiotrace_text() -> &'static str {
        "W 4 1.000000 1 0xfee00000 0x00000001 0x0\n"
    }

    #[tokio::test]
    async fn observe_missing_params_returns_error() {
        let (handler, _dir) = handler_with_temp_store();
        let err = handler.hw_learn_observe(None).await.unwrap_err();
        assert_eq!(
            err.code,
            crate::pure_jsonrpc::types::JsonRpcError::INVALID_PARAMS
        );
        assert!(err.message.contains("trace_data"));
    }

    #[tokio::test]
    async fn observe_missing_trace_data_returns_error() {
        let (handler, _dir) = handler_with_temp_store();
        let params = json!({});
        let err = handler.hw_learn_observe(Some(&params)).await.unwrap_err();
        assert_eq!(
            err.code,
            crate::pure_jsonrpc::types::JsonRpcError::INVALID_PARAMS
        );
        assert!(err.message.contains("trace_data"));
    }

    #[tokio::test]
    async fn observe_valid_mmiotrace_parses() {
        let (handler, _dir) = handler_with_temp_store();
        let params = json!({
            "trace_data": minimal_mmiotrace_text(),
            "mode": "mmiotrace",
        });
        let value = handler.hw_learn_observe(Some(&params)).await.unwrap();
        assert_eq!(value.get("domain"), Some(&json!("compute.hardware")));
        assert_eq!(value.get("operation"), Some(&json!("observe")));
        assert_eq!(value.get("events_count"), Some(&json!(1)));
        assert_eq!(value.get("driver"), Some(&json!("mmiotrace")));
    }

    #[tokio::test]
    async fn distill_missing_params_returns_error() {
        let (handler, _dir) = handler_with_temp_store();
        let err = handler.hw_learn_distill(None).await.unwrap_err();
        assert_eq!(
            err.code,
            crate::pure_jsonrpc::types::JsonRpcError::INVALID_PARAMS
        );
    }

    #[tokio::test]
    async fn distill_missing_baseline_returns_error() {
        let (handler, _dir) = handler_with_temp_store();
        let params = json!({
            "compute": minimal_mmiotrace_text(),
            "chip": "gv100",
        });
        let err = handler.hw_learn_distill(Some(&params)).await.unwrap_err();
        assert_eq!(
            err.code,
            crate::pure_jsonrpc::types::JsonRpcError::INVALID_PARAMS
        );
        assert!(err.message.contains("baseline"));
    }

    #[tokio::test]
    async fn distill_missing_compute_returns_error() {
        let (handler, _dir) = handler_with_temp_store();
        let params = json!({
            "baseline": minimal_mmiotrace_text(),
            "chip": "gv100",
        });
        let err = handler.hw_learn_distill(Some(&params)).await.unwrap_err();
        assert_eq!(
            err.code,
            crate::pure_jsonrpc::types::JsonRpcError::INVALID_PARAMS
        );
        assert!(err.message.contains("compute"));
    }

    #[tokio::test]
    async fn distill_missing_chip_returns_error() {
        let (handler, _dir) = handler_with_temp_store();
        let params = json!({
            "baseline": minimal_mmiotrace_text(),
            "compute": minimal_mmiotrace_text(),
        });
        let err = handler.hw_learn_distill(Some(&params)).await.unwrap_err();
        assert_eq!(
            err.code,
            crate::pure_jsonrpc::types::JsonRpcError::INVALID_PARAMS
        );
        assert!(err.message.contains("chip"));
    }

    #[tokio::test]
    async fn distill_valid_traces_returns_recipe() {
        let (handler, _dir) = handler_with_temp_store();
        let t = minimal_mmiotrace_text();
        let params = json!({
            "baseline": t,
            "compute": t,
            "chip": "gv100",
        });
        let value = handler.hw_learn_distill(Some(&params)).await.unwrap();
        assert_eq!(value.get("domain"), Some(&json!("compute.hardware")));
        assert_eq!(value.get("operation"), Some(&json!("distill")));
        assert_eq!(value.get("chip"), Some(&json!("gv100")));
        assert!(value.get("diff_count").is_some());
        assert!(value.get("recipe_steps").is_some());
        assert!(value.get("recipe").is_some());
    }
}
