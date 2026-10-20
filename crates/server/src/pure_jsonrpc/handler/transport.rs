// SPDX-License-Identifier: AGPL-3.0-or-later
//! Hardware transport layer for JSON-RPC handler.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use std::sync::Mutex;
use toadstool_core::HardwareTransport;
use toadstool_display::{HardwareTransportDispatch, TransportRouter};
use tokio_util::sync::CancellationToken;

use crate::pure_jsonrpc::types::{JsonRpcError, JsonRpcErrorExt};

/// Active stream metadata tracked by the handler.
struct ActiveStream {
    rx_id: String,
    tx_id: String,
    cancel: CancellationToken,
    bytes_transferred: Arc<std::sync::atomic::AtomicU64>,
    started_at: std::time::Instant,
}

/// Handles hardware transport discovery and routing.
pub(super) struct TransportHandler {
    pub(super) transport_router: Arc<Mutex<TransportRouter>>,
    active_streams: Mutex<HashMap<String, ActiveStream>>,
    next_stream_id: Mutex<u64>,
}

impl TransportHandler {
    pub(super) fn new() -> Self {
        Self {
            transport_router: Arc::new(Mutex::new(TransportRouter::new())),
            active_streams: Mutex::new(HashMap::new()),
            next_stream_id: Mutex::new(1),
        }
    }

    pub(super) fn transport_discover(_params: Option<&serde_json::Value>) -> serde_json::Value {
        #[cfg(target_os = "linux")]
        let all: Vec<serde_json::Value> = {
            let display = toadstool_display::discover_display_transports();
            let capture = toadstool_display::discover_capture_transports();
            let serial = toadstool_display::serial_transport::discover_serial_transports();
            let pcie = toadstool_display::discover_pcie_transports();

            display
                .iter()
                .chain(capture.iter())
                .chain(serial.iter())
                .chain(pcie.iter())
                .map(|t| {
                    serde_json::json!({
                        "id": t.id,
                        "label": t.label,
                        "medium": format!("{}", t.medium),
                        "direction": format!("{}", t.direction),
                    })
                })
                .collect()
        };
        #[cfg(not(target_os = "linux"))]
        let all: Vec<serde_json::Value> = Vec::new();

        serde_json::json!({"transports": all, "count": all.len()})
    }

    pub(super) async fn transport_list(&self) -> Result<serde_json::Value, JsonRpcError> {
        let router = self
            .transport_router
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let list: Vec<_> = router
            .list()
            .iter()
            .map(|t| {
                serde_json::json!({
                    "id": t.id,
                    "label": t.label,
                    "medium": format!("{}", t.medium),
                    "direction": format!("{}", t.direction),
                })
            })
            .collect();
        Ok(serde_json::json!({"transports": list, "count": list.len()}))
    }

    pub(super) async fn transport_route(
        &self,
        params: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let params = params.ok_or_else(|| JsonRpcError::invalid_params("Missing params"))?;
        let rx_id = params
            .get("rx_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| JsonRpcError::invalid_params("Missing 'rx_id'"))?;
        let tx_id = params
            .get("tx_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| JsonRpcError::invalid_params("Missing 'tx_id'"))?;
        #[expect(
            clippy::cast_possible_truncation,
            reason = "buf_size is capped at practical values"
        )]
        let buf_size = params
            .get("buf_size")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(65536) as usize;

        let mut router = self
            .transport_router
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let bytes = router
            .route_once(rx_id, tx_id, buf_size)
            .map_err(|e| JsonRpcError::internal_error(e.to_string()))?;
        Ok(serde_json::json!({
            "bytes_transferred": bytes,
            "rx_id": rx_id,
            "tx_id": tx_id
        }))
    }

    /// Register a discovered `PCIe` transport into the router by its link endpoints.
    ///
    /// Params: `{ "source_slot": "0000:25:00.0", "target_slot": "0000:41:00.0" }`
    #[cfg(target_os = "linux")]
    pub(super) async fn transport_open(
        &self,
        params: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let params = params.ok_or_else(|| JsonRpcError::invalid_params("Missing params"))?;
        let source_slot = params
            .get("source_slot")
            .and_then(|v| v.as_str())
            .ok_or_else(|| JsonRpcError::invalid_params("Missing 'source_slot'"))?;
        let target_slot = params
            .get("target_slot")
            .and_then(|v| v.as_str())
            .ok_or_else(|| JsonRpcError::invalid_params("Missing 'target_slot'"))?;

        let links = toadstool_display::pcie_transport::discover_pcie_links();
        let link = links
            .iter()
            .find(|l| l.source.pci_slot == source_slot && l.target.pci_slot == target_slot)
            .ok_or_else(|| {
                JsonRpcError::invalid_params(format!(
                    "No PCIe link found between {source_slot} and {target_slot}"
                ))
            })?;

        let transport =
            toadstool_display::PcieTransport::open(link.source.clone(), link.target.clone())
                .map_err(|e| JsonRpcError::internal_error(e.to_string()))?;

        let id = transport.info().id.clone();
        let bandwidth = transport.bandwidth_bps();

        let mut router = self
            .transport_router
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        router.register(HardwareTransportDispatch::Pcie(transport));

        Ok(serde_json::json!({
            "id": id,
            "bandwidth_bps": bandwidth,
            "status": "registered"
        }))
    }

    /// PCIe transport not available on this platform.
    #[cfg(not(target_os = "linux"))]
    pub(super) async fn transport_open(
        &self,
        _params: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, JsonRpcError> {
        Err(JsonRpcError::internal_error(
            "PCIe transport discovery requires Linux sysfs — not available on this platform",
        ))
    }

    /// Start continuous streaming between two registered transports.
    ///
    /// Params: `{ "rx_id": "...", "tx_id": "...", "buf_size": 65536 }`
    /// Returns: `{ "stream_id": "...", "status": "streaming" }`
    pub(super) async fn transport_stream(
        &self,
        params: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let params = params.ok_or_else(|| JsonRpcError::invalid_params("Missing params"))?;
        let rx_id = params
            .get("rx_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| JsonRpcError::invalid_params("Missing 'rx_id'"))?
            .to_string();
        let tx_id = params
            .get("tx_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| JsonRpcError::invalid_params("Missing 'tx_id'"))?
            .to_string();
        #[expect(
            clippy::cast_possible_truncation,
            reason = "buf_size is capped at practical values"
        )]
        let buf_size = params
            .get("buf_size")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(65536) as usize;

        {
            let router = self
                .transport_router
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            if router.get(&rx_id).is_none() {
                return Err(JsonRpcError::invalid_params(format!(
                    "rx transport not registered: {rx_id}"
                )));
            }
            if router.get(&tx_id).is_none() {
                return Err(JsonRpcError::invalid_params(format!(
                    "tx transport not registered: {tx_id}"
                )));
            }
        }

        let mut id_counter = self
            .next_stream_id
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let stream_id = format!("stream-{}", *id_counter);
        *id_counter += 1;

        let cancel = CancellationToken::new();
        let bytes_counter = Arc::new(std::sync::atomic::AtomicU64::new(0));

        let stream = ActiveStream {
            rx_id: rx_id.clone(),
            tx_id: tx_id.clone(),
            cancel: cancel.clone(),
            bytes_transferred: Arc::clone(&bytes_counter),
            started_at: std::time::Instant::now(),
        };

        self.active_streams
            .lock()
            .await
            .insert(stream_id.clone(), stream);

        let router = Arc::clone(&self.transport_router);
        tokio::spawn(async move {
            // Backoff after `route_once` errors: 1ms → 2 → … capped at 100ms; reset on success.
            const MAX_BACKOFF_MS: u64 = 100;
            let mut backoff_ms: u64 = 1;

            loop {
                if cancel.is_cancelled() {
                    break;
                }

                let result = {
                    let mut r = router.lock().unwrap_or_else(|e| e.into_inner());
                    r.route_once(&rx_id, &tx_id, buf_size)
                };

                if let Ok(n) = result {
                    bytes_counter.fetch_add(n as u64, std::sync::atomic::Ordering::Relaxed);
                    backoff_ms = 1;
                } else {
                    tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                    backoff_ms = (backoff_ms * 2).min(MAX_BACKOFF_MS);
                }

                tokio::task::yield_now().await;
            }
        });

        Ok(serde_json::json!({
            "stream_id": stream_id,
            "status": "streaming"
        }))
    }

    /// Query status of active streams.
    ///
    /// Params: `{ "stream_id": "stream-1" }` (optional — all streams if omitted)
    pub(super) async fn transport_status(
        &self,
        params: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let streams = self
            .active_streams
            .lock()
            .unwrap_or_else(|e| e.into_inner());

        if let Some(params) = params
            && let Some(stream_id) = params.get("stream_id").and_then(|v| v.as_str())
        {
            let stream = streams.get(stream_id).ok_or_else(|| {
                JsonRpcError::invalid_params(format!("Unknown stream: {stream_id}"))
            })?;

            let bytes = stream
                .bytes_transferred
                .load(std::sync::atomic::Ordering::Relaxed);
            let elapsed = stream.started_at.elapsed();

            return Ok(serde_json::json!({
                "stream_id": stream_id,
                "rx_id": stream.rx_id,
                "tx_id": stream.tx_id,
                "bytes_transferred": bytes,
                "elapsed_seconds": elapsed.as_secs_f64(),
                "active": !stream.cancel.is_cancelled(),
            }));
        }

        let all: Vec<_> = streams
            .iter()
            .map(|(id, s)| {
                let bytes = s
                    .bytes_transferred
                    .load(std::sync::atomic::Ordering::Relaxed);
                let elapsed = s.started_at.elapsed();
                serde_json::json!({
                    "stream_id": id,
                    "rx_id": s.rx_id,
                    "tx_id": s.tx_id,
                    "bytes_transferred": bytes,
                    "elapsed_seconds": elapsed.as_secs_f64(),
                    "active": !s.cancel.is_cancelled(),
                })
            })
            .collect();

        Ok(serde_json::json!({ "streams": all, "count": all.len() }))
    }
}

#[cfg(test)]
#[path = "transport_tests.rs"]
mod tests;
