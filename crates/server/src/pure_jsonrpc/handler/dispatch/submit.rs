// SPDX-License-Identifier: AGPL-3.0-or-later

use super::DispatchHandler;
use super::routing::resolve_dispatch_target;
use super::submit_params::{
    enforce_envelope, resolve_binary_param, resolve_buffers, resolve_workgroup_size,
};
use super::types::{DispatchJob, DispatchStatus};
use crate::pure_jsonrpc::handler::method_gate::CallerContext;
use crate::pure_jsonrpc::types::{JsonRpcError, JsonRpcErrorExt};
use base64::Engine;
use std::sync::atomic::Ordering;

use toadstool_distributed::crypto_integration::{
    CryptoOperation, CryptoRequest, SecurityLevel, encryption_algorithm_from_wire,
};

impl DispatchHandler {
    /// Lazily fetch and cache the `compute` purpose key from the crypto provider secrets store.
    ///
    /// Returns `Arc<EncryptionKey>` — cache hits cost a pointer bump, not a
    /// full key-material clone.
    async fn get_purpose_key(
        &self,
    ) -> Result<std::sync::Arc<toadstool::encryption::EncryptionKey>, JsonRpcError> {
        {
            let guard = self
                .cached_purpose_key
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(ref key) = *guard {
                return Ok(std::sync::Arc::clone(key));
            }
        }

        let client = self.crypto_client.as_ref().ok_or_else(|| {
            JsonRpcError::internal_error("crypto client unavailable for purpose key retrieval")
        })?;

        let key = client
            .retrieve_purpose_key("compute", None)
            .await
            .map_err(|e| {
                JsonRpcError::internal_error(format!("purpose key retrieval failed: {e}"))
            })?;

        let key = std::sync::Arc::new(key);
        let mut guard = self
            .cached_purpose_key
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *guard = Some(std::sync::Arc::clone(&key));
        Ok(key)
    }

    /// Encrypt binary payload via Tower `crypto.encrypt`.
    /// Returns the original bytes unchanged if no crypto client is present.
    async fn encrypt_payload(&self, data: &[u8]) -> Result<Vec<u8>, JsonRpcError> {
        let Some(ref client) = self.crypto_client else {
            return Ok(data.to_vec());
        };

        let key = self.get_purpose_key().await?;

        let request = CryptoRequest {
            request_id: uuid::Uuid::new_v4(),
            operation: CryptoOperation::Encrypt,
            data: data.to_vec(),
            key_id: Some(key.id.clone()),
            algorithm: Some(encryption_algorithm_from_wire(&key.algorithm)),
            security_level: SecurityLevel::High,
            metadata: serde_json::Value::Null,
        };

        let response = client
            .encrypt(request)
            .await
            .map_err(|e| JsonRpcError::internal_error(format!("crypto.encrypt failed: {e}")))?;

        let envelope = serde_json::json!({
            "v": 1,
            "ct": base64::engine::general_purpose::STANDARD.encode(&response.data),
            "n": response.metadata.get("nonce").and_then(|v| v.as_str()).unwrap_or(""),
            "alg": response.algorithm,
        });

        serde_json::to_vec(&envelope).map_err(|e| {
            JsonRpcError::internal_error(format!("envelope serialization failed: {e}"))
        })
    }

    /// Decrypt result payload from Tower `crypto.decrypt`.
    /// Returns the value unchanged if no crypto client is present.
    async fn decrypt_result(
        &self,
        result: serde_json::Value,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let Some(ct_b64) = result.get("ct").and_then(|v| v.as_str()) else {
            return Ok(result);
        };

        let Some(ref client) = self.crypto_client else {
            return Ok(result);
        };

        let ciphertext = base64::engine::general_purpose::STANDARD
            .decode(ct_b64)
            .map_err(|e| JsonRpcError::internal_error(format!("ciphertext base64 decode: {e}")))?;

        let key = self.get_purpose_key().await?;

        let request = CryptoRequest {
            request_id: uuid::Uuid::new_v4(),
            operation: CryptoOperation::Decrypt,
            data: ciphertext,
            key_id: Some(key.id.clone()),
            algorithm: Some(encryption_algorithm_from_wire(&key.algorithm)),
            security_level: SecurityLevel::High,
            metadata: serde_json::Value::Null,
        };

        let response = client
            .decrypt(request)
            .await
            .map_err(|e| JsonRpcError::internal_error(format!("crypto.decrypt failed: {e}")))?;

        Ok(serde_json::from_slice(&response.data).unwrap_or_else(|_| {
            serde_json::Value::String(
                base64::engine::general_purpose::STANDARD.encode(&response.data),
            )
        }))
    }

    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "production code uses dispatch_submit_with_context; tests use this convenience wrapper"
        )
    )]
    pub async fn dispatch_submit(
        &self,
        params: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, JsonRpcError> {
        self.dispatch_submit_with_context(params, &CallerContext::anonymous())
            .await
    }

    /// Submit with full caller context for JH-2 envelope enforcement.
    ///
    /// **IPC contract**: All param values (BDF addresses, buffer references) must
    /// be pre-resolved by the caller. No `${VAR}` expansion is performed —
    /// env expansion is a CLI-only convenience for local workload files.
    pub async fn dispatch_submit_with_context(
        &self,
        params: Option<&serde_json::Value>,
        ctx: &CallerContext,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let p = params.ok_or_else(|| {
            JsonRpcError::invalid_params(
                "Expected { binary|binary_b64, bdf?, workgroup_size|dispatch_dims?, \
                 buffers?, shader_info?, dispatch_mode?, timeout_ms? }",
            )
        })?;

        let binary_bytes = resolve_binary_param(p)?;

        if binary_bytes.is_empty() {
            return Err(JsonRpcError::invalid_params("binary must not be empty"));
        }

        let shader_info = p.get("shader_info").cloned();

        let (bdf, dispatch_mode) = resolve_dispatch_target(p)?;

        self.pre_dispatch_resource_check(&bdf, Some(ctx), Some(p))
            .await?;

        #[cfg(target_os = "linux")]
        let thermal = super::super::hw_learn::check_thermal_for_bdf_pub(&bdf);
        #[cfg(target_os = "linux")]
        if let Some(ref status) = thermal
            && !status.compute_safe()
        {
            return Err(JsonRpcError::internal_error(format!(
                "GPU {bdf} thermal status {status:?} — refusing dispatch"
            )));
        }
        #[cfg(target_os = "linux")]
        let thermal_checked = thermal.is_some();
        #[cfg(not(target_os = "linux"))]
        let thermal_checked = false;

        let workgroup_size = resolve_workgroup_size(p);

        let buffer_descs = resolve_buffers(p);

        let timeout_ms = p
            .get("timeout_ms")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(
                toadstool_common::constants::timeouts::DISPATCH_DEFAULT_TIMEOUT.as_millis() as u64,
            );

        let workgroup_total = u64::from(workgroup_size[0])
            * u64::from(workgroup_size[1])
            * u64::from(workgroup_size[2]);
        enforce_envelope(ctx, binary_bytes.len(), workgroup_total, timeout_ms)?;

        #[cfg(target_os = "linux")]
        self.acquire_device_handle(&bdf).await;

        let job_id = uuid::Uuid::new_v4().to_string();
        let submit_instant = std::time::Instant::now();
        let binary_size = binary_bytes.len();
        let job = DispatchJob {
            bdf: bdf.clone(),
            status: DispatchStatus::Submitted,
            submitted_at: submit_instant,
            binary_size,
            result: None,
        };

        {
            let mut jobs = self
                .jobs
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            jobs.insert(job_id.clone(), job);
        }

        self.dispatch_count.fetch_add(1, Ordering::Relaxed);

        {
            let mut jobs = self
                .jobs
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(job) = jobs.get_mut(&job_id) {
                job.status = DispatchStatus::Running;
            }
        }

        let needs_shader_service = matches!(&*dispatch_mode, "vfio" | "drm");

        // Phase D: try local dispatch via cylinder before shader service IPC.
        #[cfg(target_os = "linux")]
        if needs_shader_service
            && let Some(local_result) = self
                .try_local_dispatch(
                    &bdf,
                    &binary_bytes,
                    workgroup_size,
                    shader_info.as_ref(),
                    &buffer_descs,
                )
                .await
        {
            let dispatch_ms = submit_instant.elapsed().as_millis() as u64;
            match local_result {
                Ok(local_output) => {
                    let readback_ms = local_output
                        .get("readback_ms")
                        .and_then(serde_json::Value::as_u64)
                        .unwrap_or(0);
                    super::telemetry::emit_dispatch_completion_telemetry(
                        &super::telemetry::DispatchTelemetryEmit {
                            ctx,
                            method: "compute.dispatch.submit",
                            dispatch_ms,
                            readback_ms,
                            dispatch_mode: "local_cylinder",
                            bdf: &bdf,
                            binary_size,
                            workgroup_size,
                            timeout_ms,
                            success: true,
                        },
                    );
                    let readback_ms_out = local_output
                        .get("readback_ms")
                        .and_then(serde_json::Value::as_u64)
                        .unwrap_or(0);
                    {
                        let mut jobs = self
                            .jobs
                            .write()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        if let Some(job) = jobs.get_mut(&job_id) {
                            job.status = DispatchStatus::Completed;
                            job.result = Some(local_output.clone());
                        }
                    }
                    return Ok(serde_json::json!({
                        "domain": "compute.dispatch",
                        "operation": "submit",
                        "job_id": job_id,
                        "status": "completed",
                        "output": local_output,
                        "error": null,
                        "timing": {
                            "dispatch_ms": dispatch_ms,
                            "readback_ms": readback_ms_out,
                        },
                        "metadata": {
                            "bdf": bdf,
                            "dispatch_mode": "local_cylinder",
                            "binary_size": binary_size,
                            "thermal_checked": thermal_checked,
                            "workgroup_size": workgroup_size,
                        },
                    }));
                }
                Err(e) => {
                    tracing::warn!(bdf, error = %e, "local dispatch failed — falling through to shader service");
                }
            }
        }

        if needs_shader_service && !self.shader_service.is_available().await {
            let dispatch_ms = submit_instant.elapsed().as_millis() as u64;
            super::telemetry::emit_dispatch_completion_telemetry(
                &super::telemetry::DispatchTelemetryEmit {
                    ctx,
                    method: "compute.dispatch.submit",
                    dispatch_ms,
                    readback_ms: 0,
                    dispatch_mode: &dispatch_mode,
                    bdf: &bdf,
                    binary_size,
                    workgroup_size,
                    timeout_ms,
                    success: false,
                },
            );
            let mut jobs = self
                .jobs
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(job) = jobs.get_mut(&job_id) {
                job.status = DispatchStatus::Failed(
                    "visualization service not available — sovereign dispatch requires shader compiler".into(),
                );
            }
            return Ok(serde_json::json!({
                "domain": "compute.dispatch",
                "operation": "submit",
                "job_id": job_id,
                "status": "failed",
                "output": null,
                "error": "visualization service not available — sovereign dispatch requires shader compiler driver",
                "timing": { "dispatch_ms": dispatch_ms, "readback_ms": 0 },
                "metadata": {
                    "bdf": bdf,
                    "dispatch_mode": dispatch_mode,
                    "binary_size": binary_size,
                    "note": "Start a visualization capability provider — ToadStool discovers providers at runtime via capability registry",
                },
            }));
        }

        if self.shader_service.is_available().await {
            let mut encrypted = self.crypto_client.is_some();
            let dispatch_binary = if encrypted {
                match self.encrypt_payload(&binary_bytes).await {
                    Ok(enc) => enc,
                    Err(e) => {
                        tracing::warn!(
                            error = %e.message,
                            "crypto encryption unavailable \u{2014} dispatching unencrypted"
                        );
                        encrypted = false;
                        binary_bytes
                    }
                }
            } else {
                binary_bytes
            };

            let mut dispatch_params = serde_json::json!({
                "binary": dispatch_binary,
                "bdf": bdf,
                "workgroup_size": workgroup_size,
                "buffers": buffer_descs,
                "timeout_ms": timeout_ms,
                "dispatch_mode": dispatch_mode,
                "encrypted": encrypted,
            });
            if let Some(ref si) = shader_info {
                dispatch_params["shader_info"] = si.clone();
            }

            let client = &self.shader_service;
            if let Some(inner) = client.client_ref().await
                && let Some(compiler) = inner.get()
            {
                let pre_dispatch = std::time::Instant::now();
                match compiler
                    .call("compute.dispatch.submit", dispatch_params)
                    .await
                {
                    Ok(result) => {
                        let dispatch_ms = pre_dispatch.elapsed().as_millis() as u64;
                        let readback_start = std::time::Instant::now();
                        let decrypted = self.decrypt_result(result).await?;
                        let readback_ms = readback_start.elapsed().as_millis() as u64;
                        super::telemetry::emit_dispatch_completion_telemetry(
                            &super::telemetry::DispatchTelemetryEmit {
                                ctx,
                                method: "compute.dispatch.submit",
                                dispatch_ms,
                                readback_ms,
                                dispatch_mode: &dispatch_mode,
                                bdf: &bdf,
                                binary_size,
                                workgroup_size,
                                timeout_ms,
                                success: true,
                            },
                        );
                        {
                            let mut jobs = self
                                .jobs
                                .write()
                                .unwrap_or_else(std::sync::PoisonError::into_inner);
                            if let Some(job) = jobs.get_mut(&job_id) {
                                job.status = DispatchStatus::Completed;
                                job.result = Some(decrypted.clone());
                            }
                        }
                        return Ok(serde_json::json!({
                            "domain": "compute.dispatch",
                            "operation": "submit",
                            "job_id": job_id,
                            "status": "completed",
                            "output": decrypted,
                            "error": null,
                            "timing": { "dispatch_ms": dispatch_ms, "readback_ms": readback_ms },
                            "metadata": {
                                "bdf": bdf,
                                "dispatch_mode": dispatch_mode,
                                "binary_size": binary_size,
                                "thermal_checked": thermal_checked,
                                "workgroup_size": workgroup_size,
                                "encrypted": encrypted,
                                "shader_info": shader_info,
                            },
                        }));
                    }
                    Err(e) => {
                        let dispatch_ms = pre_dispatch.elapsed().as_millis() as u64;
                        let err_msg = e.to_string();
                        super::telemetry::emit_dispatch_completion_telemetry(
                            &super::telemetry::DispatchTelemetryEmit {
                                ctx,
                                method: "compute.dispatch.submit",
                                dispatch_ms,
                                readback_ms: 0,
                                dispatch_mode: &dispatch_mode,
                                bdf: &bdf,
                                binary_size,
                                workgroup_size,
                                timeout_ms,
                                success: false,
                            },
                        );
                        {
                            let mut jobs = self
                                .jobs
                                .write()
                                .unwrap_or_else(std::sync::PoisonError::into_inner);
                            if let Some(job) = jobs.get_mut(&job_id) {
                                job.status = DispatchStatus::Failed(err_msg.clone());
                            }
                        }
                        return Ok(serde_json::json!({
                            "domain": "compute.dispatch",
                            "operation": "submit",
                            "job_id": job_id,
                            "status": "failed",
                            "output": null,
                            "error": err_msg,
                            "timing": { "dispatch_ms": dispatch_ms, "readback_ms": 0 },
                            "metadata": {
                                "bdf": bdf,
                                "dispatch_mode": dispatch_mode,
                                "binary_size": binary_size,
                                "thermal_checked": thermal_checked,
                                "workgroup_size": workgroup_size,
                            },
                        }));
                    }
                }
            }
        }

        let dispatch_ms = submit_instant.elapsed().as_millis() as u64;
        super::telemetry::emit_dispatch_completion_telemetry(
            &super::telemetry::DispatchTelemetryEmit {
                ctx,
                method: "compute.dispatch.submit",
                dispatch_ms,
                readback_ms: 0,
                dispatch_mode: &dispatch_mode,
                bdf: &bdf,
                binary_size,
                workgroup_size,
                timeout_ms,
                success: false,
            },
        );
        Ok(serde_json::json!({
            "domain": "compute.dispatch",
            "operation": "submit",
            "job_id": job_id,
            "status": "submitted",
            "output": null,
            "error": null,
            "timing": { "dispatch_ms": dispatch_ms, "readback_ms": 0 },
            "metadata": {
                "bdf": bdf,
                "dispatch_mode": dispatch_mode,
                "binary_size": binary_size,
                "thermal_checked": thermal_checked,
                "workgroup_size": workgroup_size,
                "shader_info": shader_info,
            },
        }))
    }
}
