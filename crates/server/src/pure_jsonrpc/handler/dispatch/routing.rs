// SPDX-License-Identifier: AGPL-3.0-or-later

use std::borrow::Cow;

use crate::pure_jsonrpc::types::{JsonRpcError, JsonRpcErrorExt};

/// Resolve BDF and dispatch mode from request params with a single GPU scan.
///
/// Previous implementation called `discover_gpus()` twice (once per function).
/// This unified version scans once and derives both values.
pub(super) fn resolve_dispatch_target<'a>(
    params: &'a serde_json::Value,
) -> Result<(String, Cow<'a, str>), JsonRpcError> {
    let explicit_mode = params
        .get("dispatch_mode")
        .and_then(serde_json::Value::as_str);

    if let Some(bdf) = params.get("bdf").and_then(serde_json::Value::as_str) {
        let mode = if let Some(m) = explicit_mode {
            Cow::Borrowed(m)
        } else {
            let gpus = toadstool_sysmon::discover_gpus();
            if gpus
                .iter()
                .any(|g| g.pci_slot == bdf && g.driver == "vfio-pci")
            {
                Cow::Borrowed("vfio")
            } else {
                Cow::Borrowed("drm")
            }
        };
        return Ok((bdf.to_string(), mode));
    }

    let gpus = toadstool_sysmon::discover_gpus();

    let bdf = if let Some(vfio_gpu) = gpus.iter().find(|g| g.driver == "vfio-pci") {
        vfio_gpu.pci_slot.clone()
    } else {
        gpus.first()
            .map(|g| g.pci_slot.clone())
            .ok_or_else(|| JsonRpcError::internal_error("No GPUs found for dispatch"))?
    };

    let mode = if let Some(m) = explicit_mode {
        Cow::Borrowed(m)
    } else if gpus
        .iter()
        .any(|g| g.pci_slot == bdf && g.driver == "vfio-pci")
    {
        Cow::Borrowed("vfio")
    } else {
        Cow::Borrowed("drm")
    };

    Ok((bdf, mode))
}
