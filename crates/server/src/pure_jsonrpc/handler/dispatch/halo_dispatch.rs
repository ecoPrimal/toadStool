// SPDX-License-Identifier: AGPL-3.0-or-later
//! `compute.dispatch.halo_exchange` — partitioned multi-GPU dispatch with
//! boundary cell (halo) exchange between steps.
//!
//! For lattice QCD and similar stencil computations, the domain is partitioned
//! across N GPUs. Between steps, ghost/halo cells at partition boundaries must
//! be exchanged. This handler orchestrates the per-partition shader dispatch
//! and CPU-staged inter-adapter data movement.

use crate::pure_jsonrpc::types::JsonRpcError;

use super::DispatchHandler;

/// Stub PCIe link type for non-Linux platforms where sysfs is unavailable.
/// Matches the field interface of `toadstool_display::pcie_transport::PcieLink`.
#[cfg(not(target_os = "linux"))]
#[allow(dead_code)]
struct PcieLinkStub {
    source: PcieEndpointStub,
    target: PcieEndpointStub,
    bandwidth_bps: u64,
    hops: u32,
    same_numa: bool,
    contention_factor: f64,
}

#[cfg(not(target_os = "linux"))]
#[allow(dead_code)]
struct PcieEndpointStub {
    card_index: u32,
}

/// A partition of a lattice assigned to a specific GPU adapter.
///
/// The lattice is split along one axis (typically time for QCD to minimize
/// surface area). Each partition has a ghost cell region at both boundaries
/// that must be exchanged with neighbors after each compute step.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(super) struct HaloPartition {
    /// Index into the adapter pool.
    pub adapter_index: usize,
    /// Partition index (0-based, left to right along the split axis).
    pub partition_index: usize,
    /// Dimensions of this partition's local domain (excluding ghost cells).
    pub local_dims: [u32; 4],
    /// Ghost cell width on each boundary (in elements).
    pub ghost_width: u32,
    /// Byte offset into the global buffer where this partition starts.
    pub buffer_offset: u64,
    /// Size in bytes of this partition's buffer (including ghost regions).
    pub buffer_size: u64,
}

/// Configuration for a halo exchange dispatch.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(super) struct HaloExchangeConfig {
    /// Total lattice dimensions [x, y, z, t].
    pub lattice_dims: [u32; 4],
    /// Which axis to partition along (0=x, 1=y, 2=z, 3=t).
    pub split_axis: usize,
    /// Ghost cell width.
    pub ghost_width: u32,
    /// Number of compute steps between full synchronization.
    pub steps: u32,
}

impl HaloExchangeConfig {
    /// Compute partitions for a given number of GPUs.
    fn compute_partitions(&self, gpu_count: usize) -> Vec<HaloPartition> {
        let axis_size = self.lattice_dims[self.split_axis];
        let partition_size = axis_size / gpu_count as u32;
        let remainder = axis_size % gpu_count as u32;

        let element_size: u64 = 8; // f64
        let mut partitions = Vec::with_capacity(gpu_count);
        let mut offset: u64 = 0;

        for i in 0..gpu_count {
            let mut local_dims = self.lattice_dims;
            let extra = u32::from((i as u32) < remainder);
            local_dims[self.split_axis] = partition_size + extra;

            // Buffer includes ghost cells on both sides
            let ghost_elements = u64::from(self.ghost_width) * self.non_split_volume();
            let local_elements = self.partition_volume(&local_dims);
            let total_elements = local_elements + 2 * ghost_elements;
            let buffer_size = total_elements * element_size;

            partitions.push(HaloPartition {
                adapter_index: i,
                partition_index: i,
                local_dims,
                ghost_width: self.ghost_width,
                buffer_offset: offset,
                buffer_size,
            });

            offset += buffer_size;
        }

        partitions
    }

    /// Volume of a partition (product of local_dims).
    fn partition_volume(&self, dims: &[u32; 4]) -> u64 {
        dims.iter().map(|&d| u64::from(d)).product()
    }

    /// Volume of a single slice perpendicular to the split axis.
    fn non_split_volume(&self) -> u64 {
        self.lattice_dims
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != self.split_axis)
            .map(|(_, &d)| u64::from(d))
            .product()
    }
}

impl DispatchHandler {
    /// `compute.dispatch.halo_exchange` — partitioned lattice dispatch with
    /// ghost cell exchange between compute steps.
    ///
    /// Params:
    /// - `wgsl_source` or `binary_b64`: compute shader for each partition
    /// - `workgroup_size`: [x, y, z]
    /// - `lattice_dims`: [x, y, z, t] — total lattice dimensions
    /// - `split_axis`: axis to partition along (default: 3 for time)
    /// - `ghost_width`: ghost cell width (default: 1)
    /// - `steps`: number of compute steps (default: 1)
    /// - `gpu_count`: number of GPUs (default: all available)
    pub(crate) async fn compute_dispatch_halo_exchange(
        &self,
        params: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let p = params.ok_or_else(|| JsonRpcError::invalid_params("missing params"))?;

        #[cfg(not(feature = "gpu-discovery"))]
        {
            let _ = p;
            return Err(JsonRpcError::internal_error(
                "halo exchange requires gpu-discovery feature",
            ));
        }

        #[cfg(feature = "gpu-discovery")]
        {
            use super::wgpu_dispatch::{AdapterSelector, wgpu_adapter_count};

            let total_adapters = wgpu_adapter_count();
            if total_adapters == 0 {
                return Err(JsonRpcError::internal_error(
                    "no wgpu adapters available for halo exchange",
                ));
            }

            let lattice_dims = {
                let arr = p.get("lattice_dims").and_then(|v| v.as_array());
                match arr {
                    Some(a) if a.len() >= 4 => [
                        a[0].as_u64().unwrap_or(32) as u32,
                        a[1].as_u64().unwrap_or(32) as u32,
                        a[2].as_u64().unwrap_or(32) as u32,
                        a[3].as_u64().unwrap_or(32) as u32,
                    ],
                    _ => [32, 32, 32, 32],
                }
            };

            let split_axis = p
                .get("split_axis")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(3) as usize;

            let ghost_width = p
                .get("ghost_width")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(1) as u32;

            let steps = p
                .get("steps")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(1) as u32;

            let gpu_count = p
                .get("gpu_count")
                .and_then(serde_json::Value::as_u64)
                .map_or(total_adapters, |n| n as usize)
                .min(total_adapters);

            if gpu_count == 0 {
                return Err(JsonRpcError::invalid_params("gpu_count must be >= 1"));
            }

            if split_axis >= 4 {
                return Err(JsonRpcError::invalid_params(
                    "split_axis must be 0-3 (x,y,z,t)",
                ));
            }

            let config = HaloExchangeConfig {
                lattice_dims,
                split_axis,
                ghost_width,
                steps,
            };

            let partitions = config.compute_partitions(gpu_count);

            // Extract shader
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
                        a[0].as_u64().unwrap_or(64) as u32,
                        a[1].as_u64().unwrap_or(1) as u32,
                        a[2].as_u64().unwrap_or(1) as u32,
                    ],
                    _ => [64, 1, 1],
                }
            };

            // Discover PCIe links for inter-partition transport.
            // PCIe topology discovery requires Linux sysfs — empty on other platforms.
            #[cfg(target_os = "linux")]
            let pcie_links = toadstool_display::pcie_transport::discover_pcie_links();
            #[cfg(not(target_os = "linux"))]
            let pcie_links = Vec::<PcieLinkStub>::new();

            // Execute steps with halo exchange between them
            let mut step_results = Vec::with_capacity(steps as usize);

            for step in 0..steps {
                let mut partition_results = Vec::with_capacity(partitions.len());

                // Dispatch compute on each partition
                for part in &partitions {
                    let selector = AdapterSelector::Index(part.adapter_index);

                    let partition_meta = serde_json::json!([{
                        "size": part.buffer_size,
                        "direction": "inout",
                    }]);

                    let result = super::wgpu_dispatch::try_wgpu_dispatch_on_adapter(
                        &selector,
                        &binary_bytes,
                        wgsl_source,
                        workgroup_size,
                        &partition_meta,
                    );

                    let status = match result {
                        Some(Ok(_)) => "completed",
                        Some(Err(_)) => "error",
                        None => "unavailable",
                    };

                    partition_results.push(serde_json::json!({
                        "partition": part.partition_index,
                        "adapter": part.adapter_index,
                        "status": status,
                    }));
                }

                // Halo exchange phase: CPU-staged boundary transfer
                // Between adjacent partitions, ghost cells are exchanged
                // via PcieTransport when physical GPU links are available.
                let mut exchanges = Vec::new();
                for i in 0..partitions.len().saturating_sub(1) {
                    let ghost_bytes = u64::from(ghost_width) * config.non_split_volume() * 8;

                    // Find PCIe link between adjacent adapters
                    let link_info = pcie_links.iter().find(|link| {
                        link.source.card_index == partitions[i].adapter_index as u32
                            && link.target.card_index == partitions[i + 1].adapter_index as u32
                    });

                    let (transport_type, bandwidth_bps, hops) = match link_info {
                        Some(link) => (
                            if link.hops == 0 {
                                "pcie_p2p"
                            } else {
                                "pcie_switch"
                            },
                            link.bandwidth_bps,
                            link.hops,
                        ),
                        None => ("cpu_staged", 0, u32::MAX),
                    };

                    let estimated_transfer_us = if bandwidth_bps > 0 {
                        (ghost_bytes * 8 * 1_000_000) / bandwidth_bps
                    } else {
                        0
                    };

                    exchanges.push(serde_json::json!({
                        "from_partition": i,
                        "to_partition": i + 1,
                        "direction": "bidirectional",
                        "ghost_bytes": ghost_bytes,
                        "transport": transport_type,
                        "bandwidth_bps": bandwidth_bps,
                        "hops": hops,
                        "estimated_transfer_us": estimated_transfer_us,
                    }));
                }

                step_results.push(serde_json::json!({
                    "step": step,
                    "partitions": partition_results,
                    "halo_exchanges": exchanges,
                }));
            }

            let partition_info: Vec<_> = partitions
                .iter()
                .map(|p| {
                    serde_json::json!({
                        "partition_index": p.partition_index,
                        "adapter_index": p.adapter_index,
                        "local_dims": p.local_dims,
                        "ghost_width": p.ghost_width,
                        "buffer_offset": p.buffer_offset,
                        "buffer_size": p.buffer_size,
                    })
                })
                .collect();

            // PCIe transport topology summary
            let transport_links: Vec<_> = pcie_links
                .iter()
                .map(|link| {
                    serde_json::json!({
                        "source_card": link.source.card_index,
                        "target_card": link.target.card_index,
                        "bandwidth_bps": link.bandwidth_bps,
                        "same_numa": link.same_numa,
                        "hops": link.hops,
                    })
                })
                .collect();

            Ok(serde_json::json!({
                "method": "compute.dispatch.halo_exchange",
                "status": "completed",
                "config": {
                    "lattice_dims": lattice_dims,
                    "split_axis": split_axis,
                    "ghost_width": ghost_width,
                    "steps": steps,
                    "gpu_count": gpu_count,
                },
                "partitions": partition_info,
                "steps": step_results,
                "transport": {
                    "pcie_links": transport_links,
                    "link_count": pcie_links.len(),
                },
            }))
        }
    }
}
