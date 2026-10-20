// SPDX-License-Identifier: AGPL-3.0-or-later
//! `compute.dispatch.multi_gpu` — topology-aware multi-adapter dispatch.
//!
//! Accepts a compute workload with an explicit GPU count or adapter list,
//! uses `WorkloadRouter::route_multi_gpu` for topology-aware placement,
//! then dispatches the shader on each selected adapter and aggregates results.

use crate::pure_jsonrpc::types::{JsonRpcError, JsonRpcErrorExt};

use super::DispatchHandler;

impl DispatchHandler {
    /// `compute.dispatch.multi_gpu` — dispatch a shader across multiple GPUs.
    ///
    /// Params:
    /// - `binary_b64` or `wgsl_source`: shader code
    /// - `workgroup_size`: [x, y, z]
    /// - `buffers`: array of buffer descriptors
    /// - `gpu_count`: number of GPUs to use (default: all available)
    /// - `adapter_indices`: explicit adapter indices (overrides gpu_count)
    /// - `partition_strategy`: "replicate" (default) or "split"
    pub(crate) async fn compute_dispatch_multi_gpu(
        &self,
        params: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let p = params.ok_or_else(|| JsonRpcError::invalid_params("missing params"))?;

        #[cfg(not(feature = "gpu-discovery"))]
        {
            let _ = p;
            return Err(JsonRpcError::internal_error(
                "multi-GPU dispatch requires gpu-discovery feature",
            ));
        }

        #[cfg(feature = "gpu-discovery")]
        {
            use super::wgpu_dispatch::{AdapterSelector, wgpu_adapter_count, wgpu_list_adapters};

            let total_adapters = wgpu_adapter_count();
            if total_adapters == 0 {
                return Err(JsonRpcError::internal_error(
                    "no wgpu adapters available for multi-GPU dispatch",
                ));
            }

            // Determine which adapters to use
            let target_indices: Vec<usize> = if let Some(explicit) =
                p.get("adapter_indices").and_then(|v| v.as_array())
            {
                explicit
                    .iter()
                    .filter_map(|v| v.as_u64().map(|i| i as usize))
                    .filter(|&i| i < total_adapters)
                    .collect()
            } else {
                let gpu_count = p
                    .get("gpu_count")
                    .and_then(serde_json::Value::as_u64)
                    .map_or(total_adapters, |n| n as usize);

                // Use topology-aware placement via WorkloadRouter
                let available: Vec<u32> = (0..total_adapters as u32).collect();
                let topology = toadstool_sysmon::pcie_topology::discover_topology();
                let router =
                    toadstool_runtime_orchestration::workload_routing::WorkloadRouter::new();

                if let Some(placement) = router.route_multi_gpu(&available, gpu_count, &topology) {
                    tracing::info!(
                        gpu_indices = ?placement.gpu_indices,
                        shared_switch = placement.shared_switch,
                        min_bw = placement.min_interconnect_bps,
                        "multi-GPU placement via topology routing"
                    );
                    placement.gpu_indices.iter().map(|&i| i as usize).collect()
                } else {
                    (0..gpu_count.min(total_adapters)).collect()
                }
            };

            if target_indices.is_empty() {
                return Err(JsonRpcError::invalid_params(
                    "no valid adapter indices resolved",
                ));
            }

            // Extract shader and dispatch params
            let wgsl_source = p.get("wgsl_source").and_then(serde_json::Value::as_str);
            let binary_b64 = p.get("binary_b64").and_then(serde_json::Value::as_str);
            let binary_bytes = if let Some(b64) = binary_b64 {
                base64::Engine::decode(&base64::engine::general_purpose::STANDARD, b64)
                    .unwrap_or_default()
            } else {
                Vec::new()
            };

            let workgroup_size = {
                let arr = p.get("workgroup_size").and_then(|v| v.as_array());
                match arr {
                    Some(a) if a.len() >= 3 => [
                        a[0].as_u64().unwrap_or(1) as u32,
                        a[1].as_u64().unwrap_or(1) as u32,
                        a[2].as_u64().unwrap_or(1) as u32,
                    ],
                    _ => [1, 1, 1],
                }
            };

            let buffer_descs = p
                .get("buffers")
                .cloned()
                .unwrap_or(serde_json::Value::Array(vec![]));

            // Dispatch on each selected adapter
            let mut results = Vec::with_capacity(target_indices.len());
            for &idx in &target_indices {
                let selector = AdapterSelector::Index(idx);
                let result = super::wgpu_dispatch::try_wgpu_dispatch_on_adapter(
                    &selector,
                    &binary_bytes,
                    wgsl_source,
                    workgroup_size,
                    &buffer_descs,
                );

                match result {
                    Some(Ok(output)) => {
                        results.push(serde_json::json!({
                            "adapter_index": idx,
                            "status": "completed",
                            "output": output,
                        }));
                    }
                    Some(Err(e)) => {
                        results.push(serde_json::json!({
                            "adapter_index": idx,
                            "status": "error",
                            "error": e,
                        }));
                    }
                    None => {
                        results.push(serde_json::json!({
                            "adapter_index": idx,
                            "status": "unavailable",
                        }));
                    }
                }
            }

            let all_ok = results
                .iter()
                .all(|r| r.get("status").and_then(|s| s.as_str()) == Some("completed"));

            // List known adapters for diagnostics
            let adapter_list: Vec<_> = wgpu_list_adapters()
                .into_iter()
                .map(|(i, name)| serde_json::json!({"index": i, "name": name}))
                .collect();

            Ok(serde_json::json!({
                "method": "compute.dispatch.multi_gpu",
                "status": if all_ok { "completed" } else { "partial" },
                "adapter_count": total_adapters,
                "dispatched_on": target_indices,
                "results": results,
                "adapters": adapter_list,
            }))
        }
    }
}
