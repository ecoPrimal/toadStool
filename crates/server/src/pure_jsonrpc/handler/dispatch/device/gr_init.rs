// SPDX-License-Identifier: AGPL-3.0-or-later
//! `device.gr.init` JSON-RPC handler — GR context init method entries.

use super::DispatchHandler;
use crate::pure_jsonrpc::types::{JsonRpcError, JsonRpcErrorExt};

impl DispatchHandler {
    /// `device.gr.init` — submit GR context init method entries to a VFIO device.
    ///
    /// Accepts `(register, value)` pairs captured from warm-catch experiments and
    /// submits them as a GR context init pushbuffer. Required before first compute
    /// dispatch on warm-caught Volta+ GPUs (Kepler does not need this).
    pub(crate) async fn device_gr_init(
        &self,
        params: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let p = params.ok_or_else(|| {
            JsonRpcError::invalid_params(
                "Expected { bdf, method_entries: [[register, value], ...] }",
            )
        })?;

        let bdf = p
            .get("bdf")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| JsonRpcError::invalid_params("Missing 'bdf' string parameter"))?;

        let entries_arr = p
            .get("method_entries")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| {
                JsonRpcError::invalid_params(
                    "Missing 'method_entries' array of [register, value] pairs",
                )
            })?;

        let method_entries: Vec<(u32, u32)> = entries_arr
            .iter()
            .filter_map(|entry| {
                let pair = entry.as_array()?;
                let reg = pair.first()?.as_u64()? as u32;
                let val = pair.get(1)?.as_u64()? as u32;
                Some((reg, val))
            })
            .collect();

        if method_entries.is_empty() {
            return Err(JsonRpcError::invalid_params(
                "method_entries must contain at least one [register, value] pair",
            ));
        }

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

        let start = std::time::Instant::now();

        match device.init_gr_context(&method_entries) {
            Ok(()) => {
                let elapsed_ms = start.elapsed().as_millis() as u64;
                Ok(serde_json::json!({
                    "domain": "device.gr",
                    "operation": "init",
                    "bdf": bdf,
                    "status": "completed",
                    "entries_submitted": method_entries.len(),
                    "timing": { "init_ms": elapsed_ms },
                }))
            }
            Err(e) => {
                let elapsed_ms = start.elapsed().as_millis() as u64;
                Ok(serde_json::json!({
                    "domain": "device.gr",
                    "operation": "init",
                    "bdf": bdf,
                    "status": "failed",
                    "error": format!("{e}"),
                    "entries_submitted": 0,
                    "timing": { "init_ms": elapsed_ms },
                }))
            }
        }
    }
}
