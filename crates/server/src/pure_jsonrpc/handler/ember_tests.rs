// SPDX-License-Identifier: AGPL-3.0-or-later
//! Tests extracted from ember.rs (S333).

use std::borrow::Cow;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use super::*;
use crate::pure_jsonrpc::types::{JsonRpcError, ZeroCopyJsonRpcRequest};

fn test_handler() -> JsonRpcHandler {
    let executor = Arc::new(crate::tarpc_server::WorkloadExecutorDispatch::Standalone(
        crate::tarpc_server::StandaloneExecutor::new(),
    ));
    JsonRpcHandler::new(
        executor,
        "test-1.0.0".to_string(),
        None,
        Arc::new(AtomicBool::new(true)),
        None,
    )
}

fn mk_request(method: &str, params: Option<serde_json::Value>, id: i32) -> ZeroCopyJsonRpcRequest<'static> {
    ZeroCopyJsonRpcRequest {
        jsonrpc: "2.0".into(),
        method: Cow::Owned(method.to_string()),
        params,
        id: Some(serde_json::json!(id)),
    }
}

#[tokio::test]
async fn ember_list_returns_devices_array() {
    let handler = test_handler();
    let response = handler
        .handle_request(&mk_request("ember.list", None, 1))
        .await;
    assert!(response.error.is_none());
    let result = response.result.expect("ember.list result");
    assert!(
        result
            .get("devices")
            .and_then(serde_json::Value::as_array)
            .is_some(),
        "ember.list must return a devices array (possibly empty)"
    );
}

#[tokio::test]
async fn ember_status_returns_service_fields() {
    let handler = test_handler();
    let response = handler
        .handle_request(&mk_request("ember.status", None, 2))
        .await;
    assert!(response.error.is_none());
    let result = response.result.expect("ember.status result");
    assert!(
        result
            .get("devices")
            .and_then(serde_json::Value::as_array)
            .is_some(),
        "ember.status must include devices list"
    );
    assert!(
        result
            .get("uptime_secs")
            .and_then(serde_json::Value::as_u64)
            .is_some(),
        "ember.status must include uptime_secs"
    );
}

#[tokio::test]
async fn ember_reacquire_missing_bdf_returns_invalid_params() {
    let handler = test_handler();
    let response = handler
        .handle_request(&mk_request(
            "ember.reacquire",
            Some(serde_json::json!({})),
            3,
        ))
        .await;
    let err = response.error.expect("expected invalid params");
    assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
    assert!(err.message.contains("bdf"));
}

#[tokio::test]
async fn ember_reacquire_non_string_bdf_returns_invalid_params() {
    let handler = test_handler();
    let response = handler
        .handle_request(&mk_request(
            "ember.reacquire",
            Some(serde_json::json!({ "bdf": 12345 })),
            4,
        ))
        .await;
    let err = response.error.expect("expected invalid params");
    assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
    assert!(err.message.contains("bdf"));
}

#[tokio::test]
async fn device_get_missing_bdf_returns_invalid_params() {
    let handler = test_handler();
    let response = handler
        .handle_request(&mk_request("device.get", None, 5))
        .await;
    let err = response.error.expect("expected invalid params");
    assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
    assert!(err.message.contains("bdf"));
}

#[tokio::test]
async fn device_get_unknown_bdf_returns_invalid_params() {
    let handler = test_handler();
    let response = handler
        .handle_request(&mk_request(
            "device.get",
            Some(serde_json::json!({ "bdf": "0000:ff:00.0" })),
            6,
        ))
        .await;
    let err = response.error.expect("expected device not found");
    assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
    assert!(err.message.contains("Device not found"));
}

#[tokio::test]
async fn device_swap_missing_bdf_returns_invalid_params() {
    let handler = test_handler();
    let response = handler
        .handle_request(&mk_request(
            "device.swap",
            Some(serde_json::json!({ "target": "vfio-pci" })),
            7,
        ))
        .await;
    let err = response.error.expect("expected invalid params");
    assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
    assert!(err.message.contains("bdf"));
}

#[tokio::test]
async fn device_swap_missing_target_returns_invalid_params() {
    let handler = test_handler();
    let response = handler
        .handle_request(&mk_request(
            "device.swap",
            Some(serde_json::json!({ "bdf": "0000:03:00.0" })),
            8,
        ))
        .await;
    let err = response.error.expect("expected invalid params");
    assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
    assert!(err.message.contains("target"));
}

#[tokio::test]
async fn device_warm_catch_missing_bdf_returns_invalid_params() {
    let handler = test_handler();
    let response = handler
        .handle_request(&mk_request(
            "device.warm_catch",
            Some(serde_json::json!({})),
            9,
        ))
        .await;
    let err = response.error.expect("expected invalid params");
    assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
    assert!(err.message.contains("bdf"));
}

#[tokio::test]
async fn device_experiment_lifecycle_missing_action_returns_invalid_params() {
    let handler = test_handler();
    let response = handler
        .handle_request(&mk_request(
            "device.experiment_lifecycle",
            Some(serde_json::json!({ "bdf": "0000:03:00.0" })),
            10,
        ))
        .await;
    let err = response.error.expect("expected invalid params");
    assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
    assert!(err.message.contains("action"));
}
