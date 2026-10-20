// SPDX-License-Identifier: AGPL-3.0-or-later
//! Device handle and DMA lifecycle — pool/cache queries and ember DMA prep/cleanup.

use super::DispatchHandler;
use crate::pure_jsonrpc::types::{JsonRpcError, JsonRpcErrorExt};

impl DispatchHandler {
    /// Check whether a BDF has an active ember device handle or cached VFIO session.
    pub(crate) async fn has_device_handle(&self, bdf: &str) -> bool {
        {
            let pool = self
                .device_pool
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if pool.contains_key(bdf) {
                return true;
            }
        }
        let cache = self.cached_devices.lock().await;
        cache.contains_key(bdf)
    }

    /// `ember.prepare_dma` — prepare the DMA backend for a device by BDF.
    pub(crate) async fn ember_prepare_dma(
        &self,
        params: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let bdf = params
            .and_then(|p| p.get("bdf"))
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| JsonRpcError::invalid_params("Missing 'bdf' string parameter"))?;

        if self.has_device_handle(bdf).await {
            let cache = self.cached_devices.lock().await;
            let dma_ready = cache
                .get(bdf)
                .is_some_and(|dev| dev.dma_backend().is_some());
            drop(cache);
            let in_pool = self
                .device_pool
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .contains_key(bdf);
            let dma_ready = dma_ready || in_pool;
            return Ok(serde_json::json!({
                "bdf": bdf,
                "dma_ready": dma_ready,
                "ok": dma_ready,
                "method": if dma_ready { "cached" } else { "pending" },
            }));
        }

        match self.device_vfio_open_internal(bdf, None, params).await {
            Ok(method) => Ok(serde_json::json!({
                "bdf": bdf,
                "dma_ready": true,
                "ok": true,
                "method": method,
            })),
            Err(e) => Ok(serde_json::json!({
                "bdf": bdf,
                "dma_ready": false,
                "ok": false,
                "error": e,
            })),
        }
    }

    /// `ember.cleanup_dma` — release DMA resources held for a device.
    pub(crate) async fn ember_cleanup_dma(
        &self,
        params: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let bdf = params
            .and_then(|p| p.get("bdf"))
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| JsonRpcError::invalid_params("Missing 'bdf' string parameter"))?;

        {
            let mut pool = self
                .device_pool
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            pool.remove(bdf);
        }
        {
            let mut cache = self.cached_devices.lock().await;
            cache.remove(bdf);
        }

        #[cfg(target_os = "linux")]
        toadstool_cylinder::nv::registers::pmc::disable_bus_master(bdf, "ember.cleanup_dma");

        Ok(serde_json::json!({
            "bdf": bdf,
            "cleaned": true,
            "ok": true,
        }))
    }
}
