// SPDX-License-Identifier: AGPL-3.0-or-later

use super::DispatchHandler;

/// Probes the PMU falcon state after nouveau unbind and attempts
/// progressive ungating strategies to cross Tier 1 → Tier 2.
/// No DMA required — purely BAR0 register reads/writes.
pub(crate) async fn sovereign_pmu_investigate(
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
    let bar0_ref: &toadstool_cylinder::vfio::device::MappedBar = if let Some(ref engaged) = clutch {
        engaged.bar0()
    } else {
        tracing::warn!(
            bdf,
            "no clutch for PMU investigate — sysfs BAR0 rw fallback"
        );
        let bar = toadstool_cylinder::vfio::device::MappedBar::from_sysfs_rw(bdf, 16 * 1024 * 1024)
            .map_err(|e| {
                JsonRpcError::internal_error(format!("sysfs BAR0 open failed for {bdf}: {e}"))
            })?;
        sysfs_bar = bar;
        &sysfs_bar
    };

    let result = toadstool_cylinder::vfio::pmu_investigate::investigate_pmu(bar0_ref);

    if let Some(engaged) = clutch {
        engaged.disengage();
    }

    serde_json::to_value(&result)
        .map_err(|e| JsonRpcError::internal_error(format!("serialization failed: {e}")))
}
