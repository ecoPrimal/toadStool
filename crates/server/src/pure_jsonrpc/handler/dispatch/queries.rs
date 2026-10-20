// SPDX-License-Identifier: AGPL-3.0-or-later

use super::DispatchHandler;
use super::types::DispatchStatus;
use crate::pure_jsonrpc::types::{JsonRpcError, JsonRpcErrorExt};

impl DispatchHandler {
    pub async fn dispatch_status(
        &self,
        params: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let job_id = params
            .and_then(|p| p.get("job_id"))
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| JsonRpcError::invalid_params("Missing 'job_id'"))?;

        let jobs = self
            .jobs
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let job = jobs.get(job_id).ok_or_else(|| {
            JsonRpcError::internal_error(format!("Dispatch job {job_id} not found"))
        })?;

        let (status_str, error_str) = match &job.status {
            DispatchStatus::Failed(msg) => ("failed", Some(msg.clone())),
            other => (other.as_str(), None),
        };

        Ok(serde_json::json!({
            "domain": "compute.dispatch",
            "operation": "status",
            "job_id": job_id,
            "status": status_str,
            "output": null,
            "error": error_str,
            "metadata": {
                "bdf": job.bdf,
                "binary_size": job.binary_size,
                "elapsed_ms": job.submitted_at.elapsed().as_millis() as u64,
            },
        }))
    }

    pub async fn dispatch_result(
        &self,
        params: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let job_id = params
            .and_then(|p| p.get("job_id"))
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| JsonRpcError::invalid_params("Missing 'job_id'"))?;

        let jobs = self
            .jobs
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let job = jobs.get(job_id).ok_or_else(|| {
            JsonRpcError::internal_error(format!("Dispatch job {job_id} not found"))
        })?;

        let (status_str, error_str) = match &job.status {
            DispatchStatus::Failed(msg) => ("failed", Some(msg.clone())),
            other => (other.as_str(), None),
        };

        Ok(serde_json::json!({
            "domain": "compute.dispatch",
            "operation": "result",
            "job_id": job_id,
            "status": status_str,
            "output": job.result,
            "error": error_str,
            "metadata": {},
        }))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::DispatchHandler;
    use crate::pure_jsonrpc::types::JsonRpcError;
    use crate::visualization_client::VisualizationClient;

    fn test_handler() -> DispatchHandler {
        DispatchHandler::new(Arc::new(VisualizationClient::unavailable()), None)
    }

    fn submit_params() -> serde_json::Value {
        serde_json::json!({
            "binary": [1u8, 2, 3],
            "bdf": "0000:03:00.0",
            "dispatch_mode": "passthrough",
        })
    }

    #[tokio::test]
    async fn dispatch_status_missing_params_returns_invalid_params() {
        let handler = test_handler();
        let err = handler
            .dispatch_status(None)
            .await
            .expect_err("missing job_id");
        assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
        assert!(err.message.contains("job_id"));
    }

    #[tokio::test]
    async fn dispatch_status_unknown_job_returns_not_found() {
        let handler = test_handler();
        let params = serde_json::json!({ "job_id": "missing-job" });
        let err = handler
            .dispatch_status(Some(&params))
            .await
            .expect_err("unknown job");
        assert_eq!(err.code, JsonRpcError::INTERNAL_ERROR);
        assert!(err.message.contains("not found"));
    }

    #[tokio::test]
    async fn dispatch_status_existing_job_returns_domain_and_metadata() {
        let handler = test_handler();
        let submit = handler
            .dispatch_submit(Some(&submit_params()))
            .await
            .expect("submit");
        let job_id = submit["job_id"].as_str().expect("job_id");

        let result = handler
            .dispatch_status(Some(&serde_json::json!({ "job_id": job_id })))
            .await
            .expect("status");
        assert_eq!(result["domain"], "compute.dispatch");
        assert_eq!(result["operation"], "status");
        assert_eq!(result["metadata"]["bdf"], "0000:03:00.0");
    }

    #[tokio::test]
    async fn dispatch_result_missing_job_id_returns_invalid_params() {
        let handler = test_handler();
        let err = handler
            .dispatch_result(Some(&serde_json::json!({})))
            .await
            .expect_err("missing job_id");
        assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
    }

    #[tokio::test]
    async fn dispatch_result_existing_job_returns_operation_result() {
        let handler = test_handler();
        let submit = handler
            .dispatch_submit(Some(&submit_params()))
            .await
            .expect("submit");
        let job_id = submit["job_id"].as_str().expect("job_id");

        let result = handler
            .dispatch_result(Some(&serde_json::json!({ "job_id": job_id })))
            .await
            .expect("result");
        assert_eq!(result["domain"], "compute.dispatch");
        assert_eq!(result["operation"], "result");
        assert_eq!(result["job_id"], job_id);
    }

    #[tokio::test]
    async fn dispatch_result_invalid_job_id_type_returns_invalid_params() {
        let handler = test_handler();
        let params = serde_json::json!({ "job_id": ["not", "a", "string"] });
        let err = handler
            .dispatch_result(Some(&params))
            .await
            .expect_err("bad job_id type");
        assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
    }
}
