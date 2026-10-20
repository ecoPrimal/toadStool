// SPDX-License-Identifier: AGPL-3.0-or-later
//! `device.vfio.*` JSON-RPC handlers — VFIO open and roundtrip dispatch.

use super::DispatchHandler;
use crate::pure_jsonrpc::handler::dispatch::submit_params as submit;
use crate::pure_jsonrpc::handler::method_gate::CallerContext;
use crate::pure_jsonrpc::types::{JsonRpcError, JsonRpcErrorExt};
use std::sync::atomic::Ordering;

impl DispatchHandler {
    /// `device.vfio.open` — open a VFIO device by BDF, return capabilities and status.
    ///
    /// The opened device is cached persistently — VFIO iommufd FDs and DMA
    /// mappings survive across calls. Dropping them triggers GPU reset.
    pub(crate) async fn device_vfio_open(
        &self,
        params: Option<&serde_json::Value>,
        ctx: &CallerContext,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let bdf = params
            .and_then(|p| p.get("bdf"))
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| JsonRpcError::invalid_params("Missing 'bdf' string parameter"))?;

        self.pre_dispatch_resource_check(bdf, Some(ctx), params)
            .await?;
        self.acquire_device_handle(bdf).await;
        crate::background::pcie_keepalive::activity_tracker().record();

        match self.get_or_create_device(bdf).await {
            Some(cache) => {
                let device = cache.get(bdf).ok_or_else(|| {
                    JsonRpcError::internal_error("device cache miss after insert")
                })?;
                let caps = device.capabilities();
                Ok(serde_json::json!({
                    "domain": "device.vfio",
                    "operation": "open",
                    "bdf": bdf,
                    "status": "ready",
                    "capabilities": {
                        "vendor": format!("{:?}", caps.vendor),
                        "device_name": caps.device_name,
                        "generation": caps.generation_name,
                        "has_f64": caps.has_hardware_f64,
                        "max_shared_mem_bytes": caps.max_shared_mem_bytes,
                    },
                }))
            }
            None => Ok(serde_json::json!({
                "domain": "device.vfio",
                "operation": "open",
                "bdf": bdf,
                "status": "unavailable",
                "error": "device not available — FECS cold or not VFIO-bound",
            })),
        }
    }

    /// `device.vfio.roundtrip` — alloc→upload→dispatch→sync→readback in one call.
    ///
    /// Convenience endpoint for springs that want a single RPC for the full
    /// compute lifecycle on a VFIO device. Returns a `job_id` and inline results.
    pub(crate) async fn device_vfio_roundtrip(
        &self,
        params: Option<&serde_json::Value>,
        ctx: &CallerContext,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let p = params.ok_or_else(|| {
            JsonRpcError::invalid_params(
                "Expected { bdf, binary_b64|binary, workgroup_size?, buffers?, shader_info? }",
            )
        })?;

        let bdf = p
            .get("bdf")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| JsonRpcError::invalid_params("Missing 'bdf' string parameter"))?;

        let binary_bytes = submit::resolve_binary_param(p)?;
        if binary_bytes.is_empty() {
            return Err(JsonRpcError::invalid_params("binary must not be empty"));
        }

        self.pre_dispatch_resource_check(bdf, Some(ctx), params)
            .await?;
        self.acquire_device_handle(bdf).await;
        crate::background::pcie_keepalive::activity_tracker().record();

        let mut cache = self.get_or_create_device(bdf).await.ok_or_else(|| {
            JsonRpcError::internal_error(format!(
                "VFIO device {bdf} not available — FECS cold or not VFIO-bound"
            ))
        })?;

        let device = cache.get_mut(bdf).ok_or_else(|| {
            JsonRpcError::internal_error(format!("VFIO device {bdf} not in cache after creation"))
        })?;

        let workgroup_size = submit::resolve_workgroup_size(p);
        let buffer_descs = submit::resolve_buffers(p);
        let shader_info = p.get("shader_info").cloned();

        let dims = toadstool_cylinder::DispatchDims::new(
            workgroup_size[0],
            workgroup_size[1],
            workgroup_size[2],
        );

        let info = if let Some(ref si) = shader_info {
            submit::resolve_shader_info(si, workgroup_size)
        } else {
            toadstool_cylinder::ShaderInfo {
                workgroup: workgroup_size,
                ..Default::default()
            }
        };

        if let Some(entries_arr) = p
            .get("gr_init_entries")
            .and_then(serde_json::Value::as_array)
        {
            let method_entries: Vec<(u32, u32)> = entries_arr
                .iter()
                .filter_map(|entry| {
                    let pair = entry.as_array()?;
                    let reg = pair.first()?.as_u64()? as u32;
                    let val = pair.get(1)?.as_u64()? as u32;
                    Some((reg, val))
                })
                .collect();

            if !method_entries.is_empty()
                && let Err(e) = device.init_gr_context(&method_entries)
            {
                return Err(JsonRpcError::internal_error(format!(
                    "GR context init failed: {e}"
                )));
            }
        }

        let job_id = uuid::Uuid::new_v4().to_string();
        let start = std::time::Instant::now();

        self.dispatch_count.fetch_add(1, Ordering::Relaxed);

        match Self::run_local_lifecycle(&mut **device, &binary_bytes, &dims, &info, &buffer_descs) {
            Ok(output) => {
                let dispatch_ms = start.elapsed().as_millis() as u64;
                Ok(serde_json::json!({
                    "domain": "device.vfio",
                    "operation": "roundtrip",
                    "job_id": job_id,
                    "bdf": bdf,
                    "status": "completed",
                    "output": output,
                    "timing": { "dispatch_ms": dispatch_ms },
                }))
            }
            Err(e) => {
                let dispatch_ms = start.elapsed().as_millis() as u64;
                Ok(serde_json::json!({
                    "domain": "device.vfio",
                    "operation": "roundtrip",
                    "job_id": job_id,
                    "bdf": bdf,
                    "status": "failed",
                    "error": e,
                    "timing": { "dispatch_ms": dispatch_ms },
                }))
            }
        }
    }

    /// Internal VFIO open — acquire handle and ensure a cached compute device exists.
    ///
    /// Returns the DMA method label on success (`"vfio_cdev"` when DMA backend is live).
    pub(crate) async fn device_vfio_open_internal(
        &self,
        bdf: &str,
        ctx: Option<&CallerContext>,
        params: Option<&serde_json::Value>,
    ) -> Result<String, String> {
        self.pre_dispatch_resource_check(bdf, ctx, params)
            .await
            .map_err(|e| e.message.to_string())?;
        self.acquire_device_handle(bdf).await;
        crate::background::pcie_keepalive::activity_tracker().record();

        let cache = self
            .get_or_create_device(bdf)
            .await
            .ok_or_else(|| String::from("device not available — FECS cold or not VFIO-bound"))?;

        let device = cache
            .get(bdf)
            .ok_or_else(|| String::from("device cache miss after insert"))?;

        if device.dma_backend().is_some() {
            Ok(String::from("vfio_cdev"))
        } else {
            Err(String::from(
                "VFIO session open but DMA backend unavailable (caps-only mode)",
            ))
        }
    }
}
