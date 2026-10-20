// SPDX-License-Identifier: AGPL-3.0-or-later
//! Job queue and gate routing for JSON-RPC handler.

use std::sync::Arc;
use uuid::Uuid;

use crate::cross_gate::{GateOwnership, JobRouter};
use crate::gpu_job_queue::{GpuJobQueue, JobQueueConfig, JobQueueError};

use crate::pure_jsonrpc::types::{JsonRpcError, JsonRpcErrorExt};

/// Handles GPU job queue operations and gate routing.
pub(super) struct JobHandler {
    pub(super) job_queue: GpuJobQueue,
    pub(super) router: Arc<std::sync::RwLock<JobRouter>>,
    pub(super) gate_ownership: Arc<GateOwnership>,
}

impl JobHandler {
    pub(super) fn new(gate_ownership: Arc<GateOwnership>) -> Self {
        Self {
            job_queue: GpuJobQueue::new(JobQueueConfig::default()),
            router: Arc::new(std::sync::RwLock::new(JobRouter::new(
                gate_ownership.local_gate_id.as_ref(),
            ))),
            gate_ownership,
        }
    }

    pub(super) fn job_queue_error(err: &JobQueueError) -> JsonRpcError {
        let code = match err {
            JobQueueError::JobNotFound { .. } => {
                toadstool_common::constants::jsonrpc::error_codes::WORKLOAD_NOT_FOUND
            }
            _ => JsonRpcError::INTERNAL_ERROR,
        };
        JsonRpcError {
            code,
            message: err.to_string(),
            data: None,
        }
    }

    pub(super) fn extract_job_id(params: Option<&serde_json::Value>) -> Result<Uuid, JsonRpcError> {
        let params = params.ok_or_else(|| JsonRpcError::invalid_params("Missing params"))?;
        let id_str = params
            .get("job_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| JsonRpcError::invalid_params("Missing 'job_id'"))?;
        Uuid::parse_str(id_str).map_err(|_| JsonRpcError::invalid_params("Invalid job_id UUID"))
    }

    pub(super) async fn compute_submit(
        &self,
        params: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let params = params.ok_or_else(|| JsonRpcError::invalid_params("Missing params"))?;

        let priority = params
            .get("priority")
            .and_then(serde_json::Value::as_u64)
            .and_then(|n| u32::try_from(n).ok())
            .unwrap_or(0);
        let vram_hint = params
            .get("vram_required_mb")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(4096);

        let job_type: crate::gpu_job_queue::JobType = serde::Deserialize::deserialize(params)
            .map_err(|e| JsonRpcError::invalid_params(format!("Invalid job type: {e}")))?;

        let routing = {
            let router = self
                .router
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let model = match &job_type {
                crate::gpu_job_queue::JobType::Inference { model, .. } => model.as_str(),
                _ => "",
            };
            router.route(model, vram_hint)
        };

        // If routing to a remote gate, forward the job instead of local submit
        let remote_forward = {
            let router = self
                .router
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if router.is_remote_gate(routing.gate_id.as_ref()) {
                router
                    .gate_endpoint(routing.gate_id.as_ref())
                    .map(|endpoint| (endpoint, routing.clone()))
            } else {
                None
            }
        };
        if let Some((endpoint, routing)) = remote_forward {
            match crate::cross_gate::RemoteDispatcher::forward(
                &endpoint,
                "compute.submit",
                params.clone(),
            )
            .await
            {
                Ok(remote_result) => {
                    return Ok(serde_json::json!({
                        "routing": {
                            "gate_id": routing.gate_id.as_ref(),
                            "reason": routing.reason,
                            "estimated_wait_ms": routing.estimated_wait_ms,
                        },
                        "forwarded": true,
                        "remote_gate": routing.gate_id.as_ref(),
                        "remote_result": remote_result,
                    }));
                }
                Err(e) => {
                    tracing::warn!(
                        gate = routing.gate_id.as_ref(),
                        error = %e,
                        "remote dispatch failed, falling back to local"
                    );
                    // Fall through to local submission
                }
            }
        }

        match self.job_queue.submit(job_type, priority).await {
            Ok(job_id) => Ok(serde_json::json!({
                "job_id": job_id,
                "routing": {
                    "gate_id": routing.gate_id.as_ref(),
                    "reason": routing.reason,
                    "estimated_wait_ms": routing.estimated_wait_ms,
                }
            })),
            Err(e) => Err(Self::job_queue_error(&e)),
        }
    }

    pub(super) async fn compute_status(
        &self,
        params: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let job_id = Self::extract_job_id(params)?;
        match self.job_queue.status(job_id).await {
            Ok(job) => serde_json::to_value(job)
                .map_err(|e| JsonRpcError::internal_error(format!("Serialization: {e}"))),
            Err(e) => Err(Self::job_queue_error(&e)),
        }
    }

    pub(super) async fn compute_result(
        &self,
        params: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let job_id = Self::extract_job_id(params)?;
        self.job_queue
            .result(job_id)
            .await
            .map_err(|e| Self::job_queue_error(&e))
    }

    pub(super) async fn compute_cancel(
        &self,
        params: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let job_id = Self::extract_job_id(params)?;
        self.job_queue
            .cancel(job_id)
            .await
            .map(|()| serde_json::json!({"cancelled": true}))
            .map_err(|e| Self::job_queue_error(&e))
    }

    pub(super) async fn compute_list(
        &self,
        params: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let state_filter = params
            .and_then(|p| p.get("state"))
            .and_then(|v| serde::Deserialize::deserialize(v).ok());
        let jobs = self.job_queue.list(state_filter).await;
        let counts = self.job_queue.counts().await;
        Ok(serde_json::json!({"jobs": jobs, "counts": counts}))
    }

    pub(super) async fn query_status(
        &self,
        params: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let params = params.ok_or_else(|| JsonRpcError::invalid_params("Missing params"))?;

        let workload_id = params
            .as_str()
            .ok_or_else(|| JsonRpcError::invalid_params("workload_id must be a string"))?;

        tracing::info!("Querying status: {}", workload_id);

        let job_id = Uuid::parse_str(workload_id)
            .map_err(|_| JsonRpcError::invalid_params("Invalid job ID format"))?;

        match self.job_queue.status(job_id).await {
            Ok(job) => serde_json::to_value(job)
                .map_err(|e| JsonRpcError::internal_error(format!("Serialization error: {e}"))),
            Err(e) => Err(JsonRpcError::internal_error(e.to_string())),
        }
    }

    pub(super) async fn list_workloads(
        &self,
        _params: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, JsonRpcError> {
        tracing::info!("Listing workloads");

        let jobs = self.job_queue.list(None).await;
        let counts = self.job_queue.counts().await;

        Ok(serde_json::json!({
            "jobs": jobs,
            "counts": counts,
        }))
    }

    pub(super) async fn gate_update(
        &self,
        params: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let params = params.ok_or_else(|| JsonRpcError::invalid_params("Missing params"))?;
        let gate_info: crate::cross_gate::GateGpuInfo = serde::Deserialize::deserialize(params)
            .map_err(|e| JsonRpcError::invalid_params(format!("Invalid gate info: {e}")))?;
        let gate_id = std::sync::Arc::clone(&gate_info.gate_id);
        if gate_info.is_owner {
            self.gate_ownership.note_gate_update(&gate_id, true).await;
        } else if self.gate_ownership.hardware_owner_gate_id().await.as_ref() == gate_id.as_ref() {
            self.gate_ownership.revert_to_local_owner().await;
        }
        self.router
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .update_gate(gate_info);
        Ok(serde_json::json!({"updated": true, "gate_id": gate_id.as_ref()}))
    }

    pub(super) async fn gate_remove(
        &self,
        params: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let gate_id = params
            .and_then(|p| p.get("gate_id"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| JsonRpcError::invalid_params("Missing 'gate_id' param"))?;
        if self.gate_ownership.hardware_owner_gate_id().await.as_ref() == gate_id {
            self.gate_ownership.revert_to_local_owner().await;
        }
        self.router
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove_gate(gate_id);
        Ok(serde_json::json!({"removed": true, "gate_id": gate_id}))
    }

    pub(super) async fn gate_list(&self) -> Result<serde_json::Value, JsonRpcError> {
        let router = self
            .router
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let gates: Vec<&crate::cross_gate::GateGpuInfo> = router.gates().values().collect();
        Ok(serde_json::json!({"gates": gates}))
    }

    pub(super) async fn gate_route(
        &self,
        params: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let params = params.ok_or_else(|| JsonRpcError::invalid_params("Missing params"))?;
        let model = params.get("model").and_then(|v| v.as_str()).unwrap_or("");
        let vram = params
            .get("vram_required_mb")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(4096);
        let router = self
            .router
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let decision = router.route(model, vram);
        Ok(serde_json::json!({
            "gate_id": decision.gate_id.as_ref(),
            "reason": decision.reason,
            "estimated_wait_ms": decision.estimated_wait_ms,
        }))
    }
}

#[cfg(test)]
#[path = "job_tests.rs"]
mod tests;
