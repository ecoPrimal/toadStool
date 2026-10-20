// SPDX-License-Identifier: AGPL-3.0-or-later
//! Workload executor for JSON-RPC handler.

use std::sync::Arc;

use tracing::info;

use crate::pure_jsonrpc::types::{JsonRpcError, JsonRpcErrorExt, JsonWorkloadSubmission};
use crate::tarpc_server::WorkloadExecutor;

/// Handles high-level workload execution (toadstool.* namespace).
pub(super) struct WorkloadHandler {
    pub(super) executor: Arc<crate::tarpc_server::WorkloadExecutorDispatch>,
}

impl WorkloadHandler {
    pub(super) fn new(executor: Arc<crate::tarpc_server::WorkloadExecutorDispatch>) -> Self {
        Self { executor }
    }

    /// Submit a compute workload for execution.
    ///
    /// **IPC contract**: All string fields (paths, identifiers, metadata values)
    /// must be fully resolved before submission. The server does **not** perform
    /// `${VAR}`/`$VAR` environment variable expansion — that is a CLI-only
    /// convenience in `load_workload_file`. IPC callers must send pre-resolved
    /// values. This prevents ambiguity about whose environment applies in
    /// cross-primal composition.
    pub(super) async fn submit_workload(
        &self,
        params: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let params = params.ok_or_else(|| JsonRpcError::invalid_params("Missing params"))?;

        let submission: JsonWorkloadSubmission = serde::Deserialize::deserialize(params)
            .map_err(|e| JsonRpcError::invalid_params(format!("Invalid params: {e}")))?;

        info!("Submitting workload: {}", submission.workload_id.as_ref());

        let tarpc_submission = submission
            .into_tarpc()
            .map_err(JsonRpcError::invalid_params)?;

        let result = self
            .executor
            .execute(tarpc_submission)
            .await
            .map_err(|e| JsonRpcError::internal_error(e.to_string()))?;

        serde_json::to_value(result)
            .map_err(|e| JsonRpcError::internal_error(format!("Serialization error: {e}")))
    }

    pub(super) async fn cancel_workload(
        &self,
        params: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let params = params.ok_or_else(|| JsonRpcError::invalid_params("Missing params"))?;

        let workload_id = params
            .as_str()
            .ok_or_else(|| JsonRpcError::invalid_params("workload_id must be a string"))?;

        info!("Canceling workload: {}", workload_id);

        self.executor
            .cancel(workload_id)
            .await
            .map_err(|e| JsonRpcError::internal_error(e.to_string()))?;

        Ok(serde_json::json!({"success": true}))
    }

    pub(super) async fn query_capabilities(&self) -> Result<serde_json::Value, JsonRpcError> {
        info!("Querying capabilities (self-knowledge)");

        let caps = self
            .executor
            .query_capabilities()
            .await
            .map_err(|e| JsonRpcError::internal_error(e.to_string()))?;

        serde_json::to_value(caps)
            .map_err(|e| JsonRpcError::internal_error(format!("Serialization error: {e}")))
    }

    /// Pre-flight validate a workload before dispatch (Tier 2 Science API).
    ///
    /// Parses the workload spec, checks GPU availability, determines precision
    /// tier, and estimates dispatch time — all without actually submitting.
    pub(super) async fn validate(
        &self,
        params: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let params = params.ok_or_else(|| {
            JsonRpcError::invalid_params("Expected { workload_path: string, dry_run?: bool }")
        })?;

        let workload_path = params
            .get("workload_path")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| JsonRpcError::invalid_params("workload_path must be a string"))?;

        let dry_run = params
            .get("dry_run")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true);

        info!(workload_path, dry_run, "validating workload (pre-flight)");

        let mut warnings: Vec<String> = Vec::new();
        let mut required_capabilities: Vec<String> = Vec::new();

        let spec_valid = if std::path::Path::new(workload_path).exists() {
            match std::fs::read_to_string(workload_path) {
                Ok(content) => {
                    if content.contains("[workload]") || content.contains("binary") {
                        required_capabilities.push("compute.dispatch".into());
                        if content.contains("precision") {
                            required_capabilities.push("precision.routing".into());
                        }
                        true
                    } else {
                        warnings.push("workload spec missing [workload] section".into());
                        false
                    }
                }
                Err(e) => {
                    warnings.push(format!("cannot read workload file: {e}"));
                    false
                }
            }
        } else {
            warnings.push(format!("workload file not found: {workload_path}"));
            false
        };

        let caps = self
            .executor
            .query_capabilities()
            .await
            .map_err(|e| JsonRpcError::internal_error(e.to_string()))?;

        let gpu_available = caps
            .compute_units
            .iter()
            .any(|u| u.unit_type.to_lowercase().contains("gpu"));

        if !gpu_available {
            warnings.push("no GPU device available for dispatch".into());
        }

        let precision_tier = if caps
            .metadata
            .get("precision_tiers")
            .is_some_and(|v| v.contains("DF64"))
        {
            "DF64"
        } else if gpu_available {
            "FP32"
        } else {
            "none"
        };

        let estimated_dispatch_time_ms: u64 = if gpu_available && spec_valid { 100 } else { 0 };

        Ok(serde_json::json!({
            "valid": spec_valid,
            "gpu_available": gpu_available,
            "precision_tier": precision_tier,
            "estimated_dispatch_time_ms": estimated_dispatch_time_ms,
            "warnings": warnings,
            "required_capabilities": required_capabilities,
            "dry_run": dry_run,
        }))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::tarpc_server::{TestWorkloadDouble, WorkloadExecutorDispatch};
    use base64::{Engine as _, engine::general_purpose::STANDARD};

    use super::WorkloadHandler;
    use crate::pure_jsonrpc::types::JsonRpcError;

    fn handler(double: TestWorkloadDouble) -> WorkloadHandler {
        WorkloadHandler::new(Arc::new(WorkloadExecutorDispatch::TestDouble(double)))
    }

    fn valid_submit_params() -> serde_json::Value {
        serde_json::json!({
            "workload_id": "work-1",
            "workload_type": "cpu_compute",
            "data": STANDARD.encode([1u8, 2, 3]),
            "metadata": {},
            "priority": "Normal",
            "requirements": {
                "cpu_cores": 1,
                "memory_bytes": 1024
            }
        })
    }

    #[tokio::test]
    async fn submit_workload_missing_params_returns_invalid_params() {
        let err = handler(TestWorkloadDouble::Mock)
            .submit_workload(None)
            .await
            .expect_err("missing params");
        assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
        assert!(err.message.contains("Missing params"));
    }

    #[tokio::test]
    async fn submit_workload_invalid_params_returns_invalid_params() {
        let params = serde_json::json!({"workload_id": 42});
        let err = handler(TestWorkloadDouble::Mock)
            .submit_workload(Some(&params))
            .await
            .expect_err("invalid params");
        assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
        assert!(err.message.contains("Invalid params"));
    }

    #[tokio::test]
    async fn submit_workload_valid_params_forwards_to_executor() {
        let result = handler(TestWorkloadDouble::Mock)
            .submit_workload(Some(&valid_submit_params()))
            .await
            .expect("submit");
        assert_eq!(result["workload_id"], "work-1");
        assert_eq!(result["status"], "Completed");
    }

    #[tokio::test]
    async fn cancel_workload_missing_params_returns_invalid_params() {
        let err = handler(TestWorkloadDouble::Mock)
            .cancel_workload(None)
            .await
            .expect_err("missing params");
        assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
    }

    #[tokio::test]
    async fn cancel_workload_invalid_params_returns_invalid_params() {
        let params = serde_json::json!(42);
        let err = handler(TestWorkloadDouble::Mock)
            .cancel_workload(Some(&params))
            .await
            .expect_err("invalid params");
        assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
        assert!(err.message.contains("workload_id must be a string"));
    }

    #[tokio::test]
    async fn cancel_workload_valid_params_returns_success() {
        let result = handler(TestWorkloadDouble::Mock)
            .cancel_workload(Some(&serde_json::json!("work-1")))
            .await
            .expect("cancel");
        assert_eq!(result["success"], true);
    }

    #[tokio::test]
    async fn query_capabilities_forwards_to_executor() {
        let result = handler(TestWorkloadDouble::Mock)
            .query_capabilities()
            .await
            .expect("capabilities");
        assert_eq!(result["service_id"], "mock");
    }

    #[tokio::test]
    async fn validate_missing_params_returns_invalid_params() {
        let err = handler(TestWorkloadDouble::Mock)
            .validate(None)
            .await
            .expect_err("missing params");
        assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
    }

    #[tokio::test]
    async fn validate_missing_workload_path_returns_invalid_params() {
        let params = serde_json::json!({"dry_run": true});
        let err = handler(TestWorkloadDouble::Mock)
            .validate(Some(&params))
            .await
            .expect_err("missing workload_path");
        assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
        assert!(err.message.contains("workload_path"));
    }

    #[tokio::test]
    async fn validate_nonexistent_path_reports_warning() {
        let params = serde_json::json!({
            "workload_path": "/nonexistent/workload-validate-test.toml",
            "dry_run": true
        });
        let result = handler(TestWorkloadDouble::Mock)
            .validate(Some(&params))
            .await
            .expect("validate");
        assert_eq!(result["valid"], false);
        assert!(
            result["warnings"]
                .as_array()
                .unwrap()
                .iter()
                .any(|w| w.as_str().unwrap().contains("not found"))
        );
    }
}
