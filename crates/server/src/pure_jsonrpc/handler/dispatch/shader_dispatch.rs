// SPDX-License-Identifier: AGPL-3.0-or-later
//! `shader.dispatch` — accepts a compiled shader binary (from the visualization/shader
//! service's `shader.compile.wgsl` response or raw bytes) and dispatches it to the
//! target GPU via VFIO or DRM, returning readback results.
//!
//! Pipeline: **shader compile (visualization service) → ToadStool dispatch → consumer validate**

use base64::{Engine as _, engine::general_purpose::STANDARD};

use super::DispatchHandler;
use super::routing::resolve_dispatch_target;
use super::submit_params::enforce_envelope;
use super::types::{DispatchJob, DispatchStatus};
use crate::pure_jsonrpc::handler::method_gate::CallerContext;
use crate::pure_jsonrpc::types::{JsonRpcError, JsonRpcErrorExt};
use std::sync::atomic::Ordering;

impl DispatchHandler {
    /// Handle `shader.dispatch` — the dispatch half of the sovereign shader pipeline.
    ///
    /// Accepts compiled GPU binary in two forms:
    /// - **Direct**: `{ "binary": "<base64 string>" }` or `{ "binary": [u8, ...] }`
    /// - **Pipeline chaining**: `{ "compile_result": { "binary": [...], "arch": "sm89" } }`
    ///
    /// Auto-detects binary encoding: base64 string or JSON array of u8 numbers.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "production code uses shader_dispatch_with_context; tests use this convenience wrapper"
        )
    )]
    pub async fn shader_dispatch(
        &self,
        params: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, JsonRpcError> {
        self.shader_dispatch_with_context(params, &CallerContext::anonymous())
            .await
    }

    /// Context-aware shader dispatch for JH-2 envelope enforcement.
    pub async fn shader_dispatch_with_context(
        &self,
        params: Option<&serde_json::Value>,
        ctx: &CallerContext,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let p = params.ok_or_else(|| {
            JsonRpcError::invalid_params(
                "Expected { binary | compile_result, workgroup_size?, buffers?, \
                 bdf?, dispatch_mode?, readback?, timeout_ms? }",
            )
        })?;

        let (binary_bytes, source_arch) = extract_binary(p)?;

        if binary_bytes.is_empty() {
            return Err(JsonRpcError::invalid_params(
                "Shader binary must not be empty",
            ));
        }

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
                "GPU {bdf} thermal status {status:?} — refusing shader dispatch"
            )));
        }
        #[cfg(target_os = "linux")]
        let thermal_checked = thermal.is_some();
        #[cfg(not(target_os = "linux"))]
        let thermal_checked = false;

        let workgroup_size =
            p.get("workgroup_size")
                .and_then(|v| v.as_array())
                .map_or([256, 1, 1], |arr| {
                    let x = arr
                        .first()
                        .and_then(serde_json::Value::as_u64)
                        .unwrap_or(256) as u32;
                    let y = arr.get(1).and_then(serde_json::Value::as_u64).unwrap_or(1) as u32;
                    let z = arr.get(2).and_then(serde_json::Value::as_u64).unwrap_or(1) as u32;
                    [x, y, z]
                });

        let buffer_descs = p.get("buffers").cloned().unwrap_or(serde_json::json!([]));
        let readback = p
            .get("readback")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true);

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

        let submit_instant = std::time::Instant::now();
        let job_id = uuid::Uuid::new_v4().to_string();
        let job = DispatchJob {
            bdf: bdf.clone(),
            status: DispatchStatus::Submitted,
            submitted_at: submit_instant,
            binary_size: binary_bytes.len(),
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

        let needs_shader_service = matches!(&*dispatch_mode, "vfio" | "drm");

        // Phase D: try local dispatch via cylinder before shader service IPC.
        #[cfg(target_os = "linux")]
        if needs_shader_service {
            self.acquire_device_handle(&bdf).await;

            if let Some(local_result) = self
                .try_local_dispatch(&bdf, &binary_bytes, workgroup_size, None, &buffer_descs)
                .await
            {
                match local_result {
                    Ok(local_output) => {
                        let dispatch_ms = submit_instant.elapsed().as_millis() as u64;
                        let readback_ms = local_output
                            .get("readback_ms")
                            .and_then(serde_json::Value::as_u64)
                            .unwrap_or(0);
                        super::telemetry::emit_dispatch_completion_telemetry(
                            &super::telemetry::DispatchTelemetryEmit {
                                ctx,
                                method: "shader.dispatch",
                                dispatch_ms,
                                readback_ms,
                                dispatch_mode: "local_cylinder",
                                bdf: &bdf,
                                binary_size: binary_bytes.len(),
                                workgroup_size,
                                timeout_ms,
                                success: true,
                            },
                        );
                        let mut jobs = self
                            .jobs
                            .write()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        if let Some(job) = jobs.get_mut(&job_id) {
                            job.status = DispatchStatus::Completed;
                            job.result = Some(local_output.clone());
                        }
                        return Ok(serde_json::json!({
                            "domain": "compute.dispatch",
                            "operation": "shader",
                            "job_id": job_id,
                            "status": "completed",
                            "output": local_output,
                            "error": null,
                            "metadata": {
                                "bdf": bdf,
                                "dispatch_mode": "local_cylinder",
                                "binary_size": binary_bytes.len(),
                                "arch": source_arch,
                                "thermal_checked": thermal_checked,
                                "workgroup_size": workgroup_size,
                                "readback": readback,
                            },
                        }));
                    }
                    Err(e) => {
                        tracing::warn!(bdf, error = %e, "shader local dispatch failed — falling through to shader service");
                    }
                }
            }
        }

        // Phase E: try wgpu dispatch for DRM-bound GPUs (Vulkan compute path).
        if needs_shader_service {
            let wgsl_source = p.get("wgsl_source").and_then(serde_json::Value::as_str);

            // Adapter selection: honor explicit adapter_index or adapter_name params.
            #[cfg(feature = "gpu-discovery")]
            let adapter_selector = if let Some(idx) =
                p.get("adapter_index").and_then(serde_json::Value::as_u64)
            {
                super::wgpu_dispatch::AdapterSelector::Index(idx as usize)
            } else if let Some(name) = p.get("adapter_name").and_then(serde_json::Value::as_str) {
                super::wgpu_dispatch::AdapterSelector::Name(name.to_string())
            } else {
                super::wgpu_dispatch::AdapterSelector::Default
            };

            #[cfg(feature = "gpu-discovery")]
            let wgpu_result = super::wgpu_dispatch::try_wgpu_dispatch_on_adapter(
                &adapter_selector,
                &binary_bytes,
                wgsl_source,
                workgroup_size,
                &buffer_descs,
            );
            #[cfg(not(feature = "gpu-discovery"))]
            let wgpu_result = super::wgpu_dispatch::try_wgpu_dispatch(
                &binary_bytes,
                wgsl_source,
                workgroup_size,
                &buffer_descs,
            );

            if let Some(wgpu_result) = wgpu_result {
                match wgpu_result {
                    Ok(wgpu_output) => {
                        let dispatch_ms = submit_instant.elapsed().as_millis() as u64;
                        super::telemetry::emit_dispatch_completion_telemetry(
                            &super::telemetry::DispatchTelemetryEmit {
                                ctx,
                                method: "shader.dispatch",
                                dispatch_ms,
                                readback_ms: 0,
                                dispatch_mode: "wgpu",
                                bdf: &bdf,
                                binary_size: binary_bytes.len(),
                                workgroup_size,
                                timeout_ms,
                                success: true,
                            },
                        );
                        let mut jobs = self
                            .jobs
                            .write()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        if let Some(job) = jobs.get_mut(&job_id) {
                            job.status = DispatchStatus::Completed;
                            job.result = Some(wgpu_output.clone());
                        }
                        return Ok(serde_json::json!({
                            "domain": "compute.dispatch",
                            "operation": "shader",
                            "job_id": job_id,
                            "status": "completed",
                            "output": wgpu_output,
                            "error": null,
                            "metadata": {
                                "bdf": bdf,
                                "dispatch_mode": "wgpu",
                                "binary_size": binary_bytes.len(),
                                "arch": source_arch,
                                "thermal_checked": thermal_checked,
                                "workgroup_size": workgroup_size,
                                "readback": readback,
                            },
                        }));
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "wgpu dispatch failed — falling through to shader service");
                    }
                }
            }
        }

        if needs_shader_service && !self.shader_service.is_available().await {
            let dispatch_ms = submit_instant.elapsed().as_millis() as u64;
            super::telemetry::emit_dispatch_completion_telemetry(
                &super::telemetry::DispatchTelemetryEmit {
                    ctx,
                    method: "shader.dispatch",
                    dispatch_ms,
                    readback_ms: 0,
                    dispatch_mode: &dispatch_mode,
                    bdf: &bdf,
                    binary_size: binary_bytes.len(),
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
                    "visualization/shader service not available — vfio/drm dispatch requires it"
                        .into(),
                );
            }
            return Ok(serde_json::json!({
                "domain": "compute.dispatch",
                "operation": "shader",
                "job_id": job_id,
                "status": "failed",
                "output": null,
                "error": "visualization/shader service not available — sovereign GPU dispatch requires a running shader provider",
                "metadata": {
                    "bdf": bdf,
                    "dispatch_mode": dispatch_mode,
                    "binary_size": binary_bytes.len(),
                    "note": "Start a shader/visualization capability provider — ToadStool discovers providers at runtime via capability registry",
                },
            }));
        }

        if self.shader_service.is_available().await {
            let mut dispatch_params = serde_json::json!({
                "binary": binary_bytes,
                "bdf": bdf,
                "workgroup_size": workgroup_size,
                "buffers": buffer_descs,
                "timeout_ms": timeout_ms,
                "dispatch_mode": dispatch_mode,
                "readback": readback,
            });

            if let Some(arch) = &source_arch {
                dispatch_params["arch"] = serde_json::Value::String(arch.clone());
            }

            let client = &self.shader_service;
            if let Some(inner) = client.client_ref().await
                && let Some(compiler) = inner.get()
            {
                match compiler
                    .call("compute.dispatch.submit", dispatch_params)
                    .await
                {
                    Ok(result) => {
                        let dispatch_ms = submit_instant.elapsed().as_millis() as u64;
                        super::telemetry::emit_dispatch_completion_telemetry(
                            &super::telemetry::DispatchTelemetryEmit {
                                ctx,
                                method: "shader.dispatch",
                                dispatch_ms,
                                readback_ms: 0,
                                dispatch_mode: &dispatch_mode,
                                bdf: &bdf,
                                binary_size: binary_bytes.len(),
                                workgroup_size,
                                timeout_ms,
                                success: true,
                            },
                        );
                        let mut jobs = self
                            .jobs
                            .write()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        if let Some(job) = jobs.get_mut(&job_id) {
                            job.status = DispatchStatus::Completed;
                            job.result = Some(result.clone());
                        }
                        return Ok(serde_json::json!({
                            "domain": "compute.dispatch",
                            "operation": "shader",
                            "job_id": job_id,
                            "status": "completed",
                            "output": result,
                            "error": null,
                            "metadata": {
                                "bdf": bdf,
                                "dispatch_mode": dispatch_mode,
                                "binary_size": binary_bytes.len(),
                                "arch": source_arch,
                                "thermal_checked": thermal_checked,
                                "workgroup_size": workgroup_size,
                                "readback": readback,
                            },
                        }));
                    }
                    Err(e) => {
                        let dispatch_ms = submit_instant.elapsed().as_millis() as u64;
                        super::telemetry::emit_dispatch_completion_telemetry(
                            &super::telemetry::DispatchTelemetryEmit {
                                ctx,
                                method: "shader.dispatch",
                                dispatch_ms,
                                readback_ms: 0,
                                dispatch_mode: &dispatch_mode,
                                bdf: &bdf,
                                binary_size: binary_bytes.len(),
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
                            job.status = DispatchStatus::Failed(e.to_string());
                        }
                        return Ok(serde_json::json!({
                            "domain": "compute.dispatch",
                            "operation": "shader",
                            "job_id": job_id,
                            "status": "failed",
                            "output": null,
                            "error": e.to_string(),
                            "metadata": {
                                "bdf": bdf,
                                "dispatch_mode": dispatch_mode,
                                "binary_size": binary_bytes.len(),
                                "arch": source_arch,
                                "thermal_checked": thermal_checked,
                                "workgroup_size": workgroup_size,
                                "readback": readback,
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
                method: "shader.dispatch",
                dispatch_ms,
                readback_ms: 0,
                dispatch_mode: &dispatch_mode,
                bdf: &bdf,
                binary_size: binary_bytes.len(),
                workgroup_size,
                timeout_ms,
                success: false,
            },
        );
        Ok(serde_json::json!({
            "domain": "compute.dispatch",
            "operation": "shader",
            "job_id": job_id,
            "status": "submitted",
            "output": null,
            "error": null,
            "metadata": {
                "bdf": bdf,
                "dispatch_mode": dispatch_mode,
                "binary_size": binary_bytes.len(),
                "arch": source_arch,
                "thermal_checked": thermal_checked,
                "workgroup_size": workgroup_size,
                "readback": readback,
            },
        }))
    }
}

/// Extract binary bytes and optional architecture from the request.
///
/// Supports three shapes:
/// 1. `{ "compile_result": { "binary": [...], "arch": "sm89" } }` (pipeline chaining)
/// 2. `{ "binary": "<base64 string>" }` (compact transport)
/// 3. `{ "binary": [u8, u8, ...] }` (backward-compatible array)
pub(crate) fn extract_binary(
    p: &serde_json::Value,
) -> Result<(Vec<u8>, Option<String>), JsonRpcError> {
    if let Some(cr) = p.get("compile_result") {
        let arch = cr
            .get("arch")
            .and_then(serde_json::Value::as_str)
            .map(String::from);
        let binary_val = cr.get("binary").ok_or_else(|| {
            JsonRpcError::invalid_params("compile_result is missing 'binary' field")
        })?;
        let bytes = decode_binary_value(binary_val)?;
        return Ok((bytes, arch));
    }

    let binary_val = p.get("binary").ok_or_else(|| {
        JsonRpcError::invalid_params(
            "Missing 'binary' (base64 string or u8 array) or 'compile_result' object",
        )
    })?;
    let arch = p
        .get("arch")
        .and_then(serde_json::Value::as_str)
        .map(String::from);
    let bytes = decode_binary_value(binary_val)?;
    Ok((bytes, arch))
}

/// Decode a binary value from either a base64 string or a JSON array of u8.
pub(crate) fn decode_binary_value(val: &serde_json::Value) -> Result<Vec<u8>, JsonRpcError> {
    if let Some(s) = val.as_str() {
        return STANDARD
            .decode(s)
            .map_err(|e| JsonRpcError::invalid_params(format!("Invalid base64 binary: {e}")));
    }

    if let Some(arr) = val.as_array() {
        return Ok(arr.iter().map(|v| v.as_u64().unwrap_or(0) as u8).collect());
    }

    Err(JsonRpcError::invalid_params(
        "'binary' must be a base64 string or a JSON array of u8 values",
    ))
}

#[cfg(test)]
#[path = "shader_dispatch_tests.rs"]
mod tests;
