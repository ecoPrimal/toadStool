// SPDX-License-Identifier: AGPL-3.0-or-later

use super::DispatchHandler;

/// Validates the sovereign DMA pipeline by dispatching a CE (Copy Engine)
/// DMA copy and verifying readback. Independent of PGRAPH/GPC state.
pub(crate) async fn sovereign_ce_validate_ember(
    handler: &DispatchHandler,
    params: Option<&serde_json::Value>,
) -> Result<serde_json::Value, crate::pure_jsonrpc::types::JsonRpcError> {
    use crate::pure_jsonrpc::types::{JsonRpcError, JsonRpcErrorExt};

    let bdf = params
        .and_then(|p| p.get("bdf"))
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| JsonRpcError::invalid_params("Missing 'bdf' string parameter"))?;

    handler.acquire_device_handle(bdf).await;
    crate::background::pcie_keepalive::activity_tracker().record();

    let mut clutch = handler.try_engage_clutch(bdf).await;
    if clutch.is_none() {
        let cache = handler.get_or_create_device(bdf).await.ok_or_else(|| {
            JsonRpcError::internal_error(format!(
                "device {bdf} not available — factory returned None"
            ))
        })?;
        drop(cache);
        clutch = handler.try_engage_clutch(bdf).await;
    }

    let sysfs_bar;
    let (bar0_ref, dma_opt): (
        &toadstool_cylinder::vfio::device::MappedBar,
        Option<toadstool_cylinder::vfio::device::DmaBackend>,
    ) = if let Some(ref engaged) = clutch {
        (engaged.bar0(), Some(engaged.dma_backend_clone()))
    } else {
        tracing::warn!(bdf, "no clutch available for CE validate — sysfs fallback");
        let bar = toadstool_cylinder::vfio::device::MappedBar::from_sysfs_rw(bdf, 16 * 1024 * 1024)
            .map_err(|e| {
                JsonRpcError::internal_error(format!("sysfs BAR0 open failed for {bdf}: {e}"))
            })?;
        let dma = {
            let cache = handler.cached_devices.lock().await;
            cache.get(bdf).and_then(|d| d.dma_backend().cloned())
        };
        sysfs_bar = bar;
        (&sysfs_bar, dma)
    };

    let dma_backend = dma_opt.ok_or_else(|| {
        JsonRpcError::internal_error(
            "no DMA backend available — CE validate requires VFIO DMA".to_string(),
        )
    })?;

    let result = toadstool_cylinder::vfio::ce_validate::validate_ce(bar0_ref, dma_backend);

    if let Some(engaged) = clutch {
        engaged.disengage();
    }

    serde_json::to_value(&result)
        .map_err(|e| JsonRpcError::internal_error(format!("serialization failed: {e}")))
}
