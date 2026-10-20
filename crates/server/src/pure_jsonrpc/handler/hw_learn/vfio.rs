// SPDX-License-Identifier: AGPL-3.0-or-later
//! VFIO devices handler — discover GPUs bound to vfio-pci.

use super::HwLearnHandler;
use super::helpers::vendor_name;
use crate::pure_jsonrpc::types::{JsonRpcError};

impl HwLearnHandler {
    /// `compute.hardware.vfio_devices` — Discover GPUs bound to vfio-pci.
    ///
    /// Returns a list of VFIO-bound GPU descriptors suitable for
    /// `VisualizationDevice::from_vfio_device` or `GpuContext::from_vfio`.
    ///
    /// Response: `{ "devices": [{ "pci_address", "vendor_id", "device_id", "iommu_group", "driver", "power_state", "supports_reset" }] }`
    ///
    /// # Errors
    ///
    /// This function does not return errors.
    #[expect(
        clippy::unused_async,
        reason = "async for JSON-RPC handler trait consistency"
    )]
    pub async fn hw_learn_vfio_devices(
        &self,
        _params: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let pci_filter = toadstool_common::pci_discovery::PciFilter::default().with_class(|c| {
            let masked = c & 0x00FF_FF00;
            masked == 0x0003_0000 || masked == 0x0003_0200
        });

        let all_gpus = toadstool_common::pci_discovery::discover_pci_devices(&pci_filter);

        let mut vfio_devices = Vec::new();

        for gpu in &all_gpus {
            if gpu.driver.as_deref() != Some("vfio-pci") {
                continue;
            }

            let iommu_group = std::fs::read_link(gpu.sysfs_path.join("iommu_group"))
                .ok()
                .and_then(|p| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .and_then(|s| s.parse::<u32>().ok())
                })
                .unwrap_or(0);

            let power_controller = nvpmu::GpuPowerController::new(&gpu.bdf);
            let power_state = power_controller
                .power_state()
                .map_or_else(|_| "unknown".to_string(), |s| format!("{s:?}"));
            let supports_reset = power_controller.supports_reset();

            vfio_devices.push(serde_json::json!({
                "pci_address": gpu.bdf,
                "vendor_id": gpu.vendor_id,
                "device_id": gpu.device_id,
                "iommu_group": iommu_group,
                "driver": "vfio-pci",
                "power_state": power_state,
                "supports_reset": supports_reset,
                "vendor_name": vendor_name(gpu.vendor_id),
            }));
        }

        Ok(serde_json::json!({
            "devices": vfio_devices,
            "count": vfio_devices.len(),
        }))
    }
}
