// SPDX-License-Identifier: AGPL-3.0-or-later

use super::*;
use crate::pure_jsonrpc::types::{JsonRpcError, JsonRpcErrorExt};
use tracing::debug;

const DEVICE_INIT_WAIT: std::time::Duration = std::time::Duration::from_secs(5);
const POST_RESET_SETTLE: std::time::Duration = std::time::Duration::from_millis(500);

impl JsonRpcHandler {
    pub(super) fn ember_list(&self) -> serde_json::Value {
        let list = self.glowplug.list_devices();
        serde_json::to_value(list).unwrap_or_else(|_| serde_json::json!({"devices": []}))
    }

    pub(super) fn ember_status(&self) -> serde_json::Value {
        let status = self.glowplug.status();
        serde_json::to_value(status).unwrap_or_else(|_| serde_json::json!({"available": false}))
    }

    pub(super) async fn ember_reacquire(
        &self,
        params: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let bdf = params
            .and_then(|p| p.get("bdf"))
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| JsonRpcError::invalid_params("Missing 'bdf' string parameter"))?;

        let result = self.glowplug.reacquire(bdf).await;
        serde_json::to_value(&result)
            .map_err(|e| JsonRpcError::internal_error(format!("serialization failed: {e}")))
    }

    /// `ember.adopt_device` — claim a device under toadStool ember management.
    ///
    /// Ensures the BDF is visible in `ember.list`, swaps to `vfio-pci` when needed,
    /// and acquires a dispatch device handle.
    pub(super) async fn ember_adopt_device(
        &self,
        params: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let bdf = params
            .and_then(|p| p.get("bdf"))
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| JsonRpcError::invalid_params("Missing 'bdf' string parameter"))?;

        let list = self.glowplug.list_devices();
        let device = list.devices.iter().find(|d| d.bdf == bdf).ok_or_else(|| {
            JsonRpcError::invalid_params(format!("Device not found in ember.list: {bdf}"))
        })?;

        let personality = if device.personality == "vfio-pci" {
            device.personality.clone()
        } else {
            let swap = self
                .device_swap(Some(&serde_json::json!({
                    "bdf": bdf,
                    "target": "vfio-pci",
                })))
                .await?;
            if !swap
                .get("success")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
            {
                return Err(JsonRpcError::internal_error(format!(
                    "swap to vfio-pci failed for {bdf}"
                )));
            }
            String::from("vfio-pci")
        };

        let _ = self
            .dispatch
            .device_vfio_open_internal(bdf, None, None)
            .await;

        Ok(serde_json::json!({
            "bdf": bdf,
            "adopted": true,
            "personality": personality,
        }))
    }

    /// `device.swap` — swap a GPU to a target personality (e.g. "vfio-pci", "nouveau").
    pub(super) async fn device_swap(
        &self,
        params: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let bdf = params
            .and_then(|p| p.get("bdf"))
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| JsonRpcError::invalid_params("Missing 'bdf' string parameter"))?;
        let target = params
            .and_then(|p| p.get("target"))
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                JsonRpcError::invalid_params("Missing 'target' string parameter (driver name)")
            })?;

        let result = self.glowplug.swap(bdf, target).await;
        serde_json::to_value(&result)
            .map_err(|e| JsonRpcError::internal_error(format!("serialization failed: {e}")))
    }

    /// `device.warm_catch` — detect warm GPU state via PMC_ENABLE probe.
    pub(super) fn device_warm_catch(
        &self,
        params: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let bdf = params
            .and_then(|p| p.get("bdf"))
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| JsonRpcError::invalid_params("Missing 'bdf' string parameter"))?;

        Ok(self.glowplug.warm_detect(bdf))
    }

    pub(super) fn device_experiment_lifecycle(
        &self,
        params: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let bdf = params
            .and_then(|p| p.get("bdf"))
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| JsonRpcError::invalid_params("Missing 'bdf' string parameter"))?;
        let action = params
            .and_then(|p| p.get("action"))
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| JsonRpcError::invalid_params("Missing 'action' string parameter"))?;
        let result = self.glowplug.experiment_lifecycle(bdf, action);
        serde_json::to_value(&result)
            .map_err(|e| JsonRpcError::internal_error(format!("serialization failed: {e}")))
    }

    /// `device.get` — enriched metadata for a single GPU by BDF.
    pub(super) fn device_get(
        &self,
        params: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let bdf = params
            .and_then(|p| p.get("bdf"))
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| JsonRpcError::invalid_params("Missing 'bdf' string parameter"))?;
        match self.glowplug.get_device(bdf) {
            Some(info) => serde_json::to_value(&info)
                .map_err(|e| JsonRpcError::internal_error(format!("serialization failed: {e}"))),
            None => Err(JsonRpcError::invalid_params(format!(
                "Device not found: {bdf}"
            ))),
        }
    }

    /// `ember.warm_cycle` — warm driver handoff cycle (seeder → wait → vfio-pci).
    pub(super) async fn ember_warm_cycle(
        &self,
        params: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let bdf = params
            .and_then(|p| p.get("bdf"))
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| JsonRpcError::invalid_params("Missing 'bdf' string parameter"))?;
        let seeder = params
            .and_then(|p| p.get("seeder_driver"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("nouveau");

        let _keepalive_guard = crate::background::pcie_keepalive::SwapGuard::enter();
        let mut stages = Vec::new();

        let seeder_start = std::time::Instant::now();
        let seeder_result = self.glowplug.swap_device_orchestrated(bdf, seeder).await;
        stages.push(serde_json::json!({
            "name": format!("swap_to_{seeder}"),
            "success": seeder_result.success,
            "duration_ms": seeder_start.elapsed().as_millis() as u64,
        }));

        if !seeder_result.success {
            return Ok(serde_json::json!({
                "bdf": bdf,
                "success": false,
                "seeder_used": seeder,
                "stages": stages,
            }));
        }

        let wait_start = std::time::Instant::now();
        std::thread::sleep(DEVICE_INIT_WAIT);
        stages.push(serde_json::json!({
            "name": "wait_init",
            "success": true,
            "duration_ms": wait_start.elapsed().as_millis() as u64,
        }));

        let vfio_start = std::time::Instant::now();
        let vfio_result = self
            .glowplug
            .swap_device_orchestrated(bdf, "vfio-pci")
            .await;
        stages.push(serde_json::json!({
            "name": "swap_to_vfio-pci",
            "success": vfio_result.success,
            "duration_ms": vfio_start.elapsed().as_millis() as u64,
        }));

        Ok(serde_json::json!({
            "bdf": bdf,
            "success": vfio_result.success,
            "seeder_used": seeder,
            "stages": stages,
        }))
    }

    /// `device.reset` — secondary bus reset via PCI sysfs.
    #[expect(
        clippy::unused_self,
        reason = "method signature matches JSON-RPC handler dispatch table"
    )]
    pub(super) fn device_reset(
        &self,
        params: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let bdf = params
            .and_then(|p| p.get("bdf"))
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| JsonRpcError::invalid_params("Missing 'bdf' string parameter"))?;
        let reset_path = toadstool_cylinder::linux_paths::sysfs_pci_device_file(bdf, "reset");
        std::fs::write(&reset_path, "1")
            .map_err(|e| JsonRpcError::internal_error(format!("SBR failed for {bdf}: {e}")))?;
        debug!(bdf, "device.reset via sysfs SBR");
        Ok(serde_json::json!({
            "bdf": bdf,
            "reset_issued": true,
            "method": "sbr",
        }))
    }

    /// `device.resurrect` — SBR, re-probe BOOT0, and rebind to vfio-pci if alive.
    pub(super) async fn device_resurrect(
        &self,
        params: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let bdf = params
            .and_then(|p| p.get("bdf"))
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| JsonRpcError::invalid_params("Missing 'bdf' string parameter"))?;

        let mut stages = Vec::new();

        let reset = self.device_reset(params)?;
        let reset_issued = reset
            .get("reset_issued")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        stages.push(serde_json::json!({
            "name": "device.reset",
            "success": reset_issued,
        }));

        let wait_start = std::time::Instant::now();
        std::thread::sleep(POST_RESET_SETTLE);
        stages.push(serde_json::json!({
            "name": "wait",
            "success": true,
            "duration_ms": wait_start.elapsed().as_millis() as u64,
        }));

        let probe = mmio::mmio_bar0_probe(params)?;
        let boot0 = probe
            .get("boot0")
            .and_then(serde_json::Value::as_u64)
            .and_then(|v| u32::try_from(v).ok())
            .unwrap_or(0xFFFF_FFFF);
        let alive = probe
            .get("responsive")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        stages.push(serde_json::json!({
            "name": "probe_boot0",
            "success": alive,
            "boot0": boot0,
        }));

        let mut success = reset_issued && alive;
        if alive {
            let _keepalive_guard = crate::background::pcie_keepalive::SwapGuard::enter();
            let swap_start = std::time::Instant::now();
            let swap = self
                .glowplug
                .swap_device_orchestrated(bdf, "vfio-pci")
                .await;
            stages.push(serde_json::json!({
                "name": "swap_to_vfio-pci",
                "success": swap.success,
                "duration_ms": swap_start.elapsed().as_millis() as u64,
            }));
            success = success && swap.success;
        }

        Ok(serde_json::json!({
            "bdf": bdf,
            "success": success,
            "stages": stages,
        }))
    }
}
