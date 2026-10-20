// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;
use crate::pure_jsonrpc::types::{JsonRpcError, JsonRpcErrorExt};
use serde_json::json;
use toadstool_core::TransportDirection;
use toadstool_display::{HardwareTransportDispatch, TestLoopbackTransport};

async fn register_rx_tx_pair(handler: &TransportHandler) {
    let mut router = handler
        .transport_router
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let rx = TestLoopbackTransport::with_default_bandwidth("rx", TransportDirection::Rx)
        .with_initial_recv_data(b"chunk");
    router.register(HardwareTransportDispatch::TestLoopback(rx));
    router.register(HardwareTransportDispatch::TestLoopback(
        TestLoopbackTransport::with_default_bandwidth("tx", TransportDirection::Tx),
    ));
}

#[tokio::test]
async fn new_creates_handler_with_empty_router() {
    let h = TransportHandler::new();
    let router = h.transport_router.lock().unwrap_or_else(|e| e.into_inner());
    assert!(router.list().is_empty());
}

#[tokio::test]
async fn transport_discover_returns_transports_and_count() {
    let _h = TransportHandler::new();
    let v = TransportHandler::transport_discover(None);
    assert!(v.get("transports").is_some());
    assert!(v.get("count").is_some());
    assert_eq!(
        v["count"].as_u64().unwrap() as usize,
        v["transports"].as_array().unwrap().len()
    );
}

#[tokio::test]
async fn transport_list_empty_router() {
    let h = TransportHandler::new();
    let v = h.transport_list().await.unwrap();
    assert_eq!(v["count"], 0);
    assert_eq!(v["transports"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn transport_route_missing_params() {
    let h = TransportHandler::new();
    let e = h.transport_route(None).await.unwrap_err();
    assert_eq!(e.code, JsonRpcError::INVALID_PARAMS);
    assert!(e.message.contains("Missing params"));
}

#[tokio::test]
async fn transport_route_missing_rx_id_or_tx_id() {
    let h = TransportHandler::new();
    let e = h
        .transport_route(Some(&json!({ "tx_id": "t" })))
        .await
        .unwrap_err();
    assert_eq!(e.code, JsonRpcError::INVALID_PARAMS);
    assert!(e.message.contains("rx_id"));

    let e = h
        .transport_route(Some(&json!({ "rx_id": "r" })))
        .await
        .unwrap_err();
    assert_eq!(e.code, JsonRpcError::INVALID_PARAMS);
    assert!(e.message.contains("tx_id"));
}

#[tokio::test]
async fn transport_stream_missing_params() {
    let h = TransportHandler::new();
    let e = h.transport_stream(None).await.unwrap_err();
    assert_eq!(e.code, JsonRpcError::INVALID_PARAMS);
    assert!(e.message.contains("Missing params"));
}

#[tokio::test]
async fn transport_stream_missing_rx_id() {
    let h = TransportHandler::new();
    let e = h
        .transport_stream(Some(&json!({ "tx_id": "tx" })))
        .await
        .unwrap_err();
    assert_eq!(e.code, JsonRpcError::INVALID_PARAMS);
    assert!(e.message.contains("rx_id"));
}

#[tokio::test]
async fn transport_stream_missing_tx_id() {
    let h = TransportHandler::new();
    let e = h
        .transport_stream(Some(&json!({ "rx_id": "rx" })))
        .await
        .unwrap_err();
    assert_eq!(e.code, JsonRpcError::INVALID_PARAMS);
    assert!(e.message.contains("tx_id"));
}

#[tokio::test]
async fn transport_stream_unregistered_rx_id() {
    let h = TransportHandler::new();
    let e = h
        .transport_stream(Some(&json!({ "rx_id": "ghost-rx", "tx_id": "ghost-tx" })))
        .await
        .unwrap_err();
    assert_eq!(e.code, JsonRpcError::INVALID_PARAMS);
    assert!(e.message.contains("rx transport not registered"));
}

#[tokio::test]
async fn transport_stream_unregistered_tx_id() {
    let h = TransportHandler::new();
    {
        let mut router = h.transport_router.lock().unwrap_or_else(|e| e.into_inner());
        router.register(HardwareTransportDispatch::TestLoopback(
            TestLoopbackTransport::with_default_bandwidth("rx-only", TransportDirection::Rx),
        ));
    }
    let e = h
        .transport_stream(Some(&json!({ "rx_id": "rx-only", "tx_id": "missing-tx" })))
        .await
        .unwrap_err();
    assert_eq!(e.code, JsonRpcError::INVALID_PARAMS);
    assert!(e.message.contains("tx transport not registered"));
}

#[tokio::test]
async fn transport_status_no_params_lists_all_empty() {
    let h = TransportHandler::new();
    let v = h.transport_status(None).await.unwrap();
    assert_eq!(v["count"], 0);
    assert_eq!(v["streams"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn transport_status_unknown_stream_id() {
    let h = TransportHandler::new();
    let e = h
        .transport_status(Some(&json!({ "stream_id": "stream-999" })))
        .await
        .unwrap_err();
    assert_eq!(e.code, JsonRpcError::INVALID_PARAMS);
    assert!(e.message.contains("Unknown stream"));
}

#[tokio::test]
async fn transport_route_transfers_bytes_when_transports_registered() {
    let h = TransportHandler::new();
    register_rx_tx_pair(&h).await;
    let v = h
        .transport_route(Some(&json!({
            "rx_id": "rx",
            "tx_id": "tx",
            "buf_size": 1024
        })))
        .await
        .unwrap();
    assert_eq!(v["bytes_transferred"], 5);
    assert_eq!(v["rx_id"], "rx");
    assert_eq!(v["tx_id"], "tx");
}

#[tokio::test]
async fn transport_stream_and_status_happy_path() {
    let h = TransportHandler::new();
    register_rx_tx_pair(&h).await;
    let started = h
        .transport_stream(Some(&json!({
            "rx_id": "rx",
            "tx_id": "tx",
            "buf_size": 256
        })))
        .await
        .unwrap();
    let stream_id = started["stream_id"].as_str().unwrap();
    assert_eq!(started["status"], "streaming");

    let one = h
        .transport_status(Some(&json!({ "stream_id": stream_id })))
        .await
        .unwrap();
    assert_eq!(one["stream_id"], stream_id);
    assert_eq!(one["rx_id"], "rx");
    assert_eq!(one["tx_id"], "tx");
    assert_eq!(one["active"], true);

    let all = h.transport_status(None).await.unwrap();
    assert_eq!(all["count"], 1);
    assert_eq!(all["streams"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn transport_open_missing_params() {
    let h = TransportHandler::new();
    let e = h.transport_open(None).await.unwrap_err();
    assert_eq!(e.code, JsonRpcError::INVALID_PARAMS);
    assert!(e.message.contains("Missing params"));
}

#[tokio::test]
async fn transport_open_missing_source_slot() {
    let h = TransportHandler::new();
    let e = h
        .transport_open(Some(&json!({ "target_slot": "0000:00:00.0" })))
        .await
        .unwrap_err();
    assert_eq!(e.code, JsonRpcError::INVALID_PARAMS);
    assert!(e.message.contains("source_slot"));
}

#[tokio::test]
async fn transport_open_missing_target_slot() {
    let h = TransportHandler::new();
    let e = h
        .transport_open(Some(&json!({ "source_slot": "0000:00:00.0" })))
        .await
        .unwrap_err();
    assert_eq!(e.code, JsonRpcError::INVALID_PARAMS);
    assert!(e.message.contains("target_slot"));
}

#[tokio::test]
async fn transport_open_no_pcie_link_for_slots() {
    let h = TransportHandler::new();
    let e = h
        .transport_open(Some(&json!({
            "source_slot": "ffff:ff:00.0",
            "target_slot": "ffff:ff:00.1"
        })))
        .await
        .unwrap_err();
    assert_eq!(e.code, JsonRpcError::INVALID_PARAMS);
    assert!(e.message.contains("No PCIe link"));
}

#[tokio::test]
async fn transport_discover_entry_has_required_fields() {
    let v = TransportHandler::transport_discover(None);
    let transports = v["transports"].as_array().expect("transports array");
    for entry in transports {
        assert!(entry.get("id").and_then(|v| v.as_str()).is_some());
        assert!(entry.get("label").and_then(|v| v.as_str()).is_some());
        assert!(entry.get("medium").and_then(|v| v.as_str()).is_some());
        assert!(entry.get("direction").and_then(|v| v.as_str()).is_some());
    }
}

#[tokio::test]
async fn transport_discover_ignores_params() {
    let with_params = TransportHandler::transport_discover(Some(&json!({ "filter": "rx" })));
    let without = TransportHandler::transport_discover(None);
    assert_eq!(with_params["count"], without["count"]);
}

#[tokio::test]
async fn transport_list_reflects_registered_transports() {
    let h = TransportHandler::new();
    register_rx_tx_pair(&h).await;
    let v = h.transport_list().await.unwrap();
    assert_eq!(v["count"], 2);
    let ids: Vec<_> = v["transports"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|t| t["id"].as_str())
        .collect();
    assert!(ids.contains(&"rx"));
    assert!(ids.contains(&"tx"));
}

#[tokio::test]
async fn transport_route_unknown_rx_endpoint() {
    let h = TransportHandler::new();
    register_rx_tx_pair(&h).await;
    let e = h
        .transport_route(Some(&json!({ "rx_id": "missing-rx", "tx_id": "tx" })))
        .await
        .unwrap_err();
    assert_eq!(e.code, JsonRpcError::INTERNAL_ERROR);
    assert!(e.message.contains("rx transport not found"));
}

#[tokio::test]
async fn transport_route_unknown_tx_endpoint() {
    let h = TransportHandler::new();
    register_rx_tx_pair(&h).await;
    let e = h
        .transport_route(Some(&json!({ "rx_id": "rx", "tx_id": "missing-tx" })))
        .await
        .unwrap_err();
    assert_eq!(e.code, JsonRpcError::INTERNAL_ERROR);
    assert!(e.message.contains("tx transport not found"));
}

#[tokio::test]
async fn transport_route_same_rx_and_tx_rejected() {
    let h = TransportHandler::new();
    register_rx_tx_pair(&h).await;
    let e = h
        .transport_route(Some(&json!({ "rx_id": "rx", "tx_id": "rx" })))
        .await
        .unwrap_err();
    assert_eq!(e.code, JsonRpcError::INTERNAL_ERROR);
    assert!(e.message.contains("same transport"));
}

#[tokio::test]
async fn transport_route_default_buf_size() {
    let h = TransportHandler::new();
    register_rx_tx_pair(&h).await;
    let v = h
        .transport_route(Some(&json!({ "rx_id": "rx", "tx_id": "tx" })))
        .await
        .unwrap();
    assert_eq!(v["bytes_transferred"], 5);
}

#[tokio::test]
async fn transport_open_malformed_slot_types() {
    let h = TransportHandler::new();
    let e = h
        .transport_open(Some(&json!({
            "source_slot": 42,
            "target_slot": "0000:00:00.0"
        })))
        .await
        .unwrap_err();
    assert_eq!(e.code, JsonRpcError::INVALID_PARAMS);
    assert!(e.message.contains("source_slot"));

    let e = h
        .transport_open(Some(&json!({
            "source_slot": "0000:00:00.0",
            "target_slot": false
        })))
        .await
        .unwrap_err();
    assert_eq!(e.code, JsonRpcError::INVALID_PARAMS);
    assert!(e.message.contains("target_slot"));
}

#[tokio::test]
async fn transport_stream_malformed_id_types() {
    let h = TransportHandler::new();
    register_rx_tx_pair(&h).await;

    let e = h
        .transport_stream(Some(&json!({ "rx_id": 1, "tx_id": "tx" })))
        .await
        .unwrap_err();
    assert_eq!(e.code, JsonRpcError::INVALID_PARAMS);
    assert!(e.message.contains("rx_id"));

    let e = h
        .transport_stream(Some(&json!({ "rx_id": "rx", "tx_id": ["tx"] })))
        .await
        .unwrap_err();
    assert_eq!(e.code, JsonRpcError::INVALID_PARAMS);
    assert!(e.message.contains("tx_id"));
}

#[tokio::test]
async fn transport_status_lists_multiple_active_streams() {
    let h = TransportHandler::new();
    register_rx_tx_pair(&h).await;

    let first = h
        .transport_stream(Some(&json!({ "rx_id": "rx", "tx_id": "tx" })))
        .await
        .unwrap();
    let second = h
        .transport_stream(Some(&json!({ "rx_id": "rx", "tx_id": "tx" })))
        .await
        .unwrap();

    let all = h.transport_status(None).await.unwrap();
    assert_eq!(all["count"], 2);
    let streams = all["streams"].as_array().unwrap();
    assert_eq!(streams.len(), 2);
    for stream in streams {
        assert_eq!(stream["active"], true);
        assert!(stream["elapsed_seconds"].as_f64().is_some());
    }

    let one = h
        .transport_status(Some(&json!({
            "stream_id": first["stream_id"]
        })))
        .await
        .unwrap();
    assert_eq!(one["stream_id"], first["stream_id"]);
    assert_ne!(one["stream_id"], second["stream_id"]);
}
