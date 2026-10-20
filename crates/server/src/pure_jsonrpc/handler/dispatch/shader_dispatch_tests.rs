// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;
use crate::pure_jsonrpc::types::JsonRpcError;
use crate::visualization_client::create_visualization_client;

#[test]
fn decode_binary_value_rejects_invalid_base64() {
    let err = decode_binary_value(&serde_json::json!("not!!!valid!!!base64")).unwrap_err();
    assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
    assert!(
        err.message.contains("Invalid base64"),
        "unexpected message: {}",
        err.message
    );
}

#[test]
fn decode_binary_value_rejects_non_string_non_array() {
    let err = decode_binary_value(&serde_json::json!(42)).unwrap_err();
    assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
    assert!(
        err.message.contains("base64 string") || err.message.contains("JSON array"),
        "unexpected message: {}",
        err.message
    );
}

#[test]
fn decode_binary_value_accepts_empty_u8_array() {
    let bytes = decode_binary_value(&serde_json::json!([])).unwrap();
    assert!(bytes.is_empty());
}

#[test]
fn decode_binary_value_maps_non_u8_json_numbers_to_zero_byte() {
    let bytes = decode_binary_value(&serde_json::json!([300, 1])).unwrap();
    assert_eq!(bytes, vec![44u8, 1]);
}

#[test]
fn extract_binary_requires_binary_or_compile_result() {
    let err = extract_binary(&serde_json::json!({})).unwrap_err();
    assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
    assert!(err.message.contains("binary") || err.message.contains("compile_result"));
}

#[test]
fn extract_binary_compile_result_requires_nested_binary() {
    let err =
        extract_binary(&serde_json::json!({ "compile_result": { "arch": "sm89" } })).unwrap_err();
    assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
    assert!(err.message.contains("compile_result"));
}

#[test]
fn extract_binary_from_compile_result_preserves_arch() {
    let (bytes, arch) = extract_binary(&serde_json::json!({
        "compile_result": { "binary": [1, 2, 3], "arch": "sm89" }
    }))
    .unwrap();
    assert_eq!(bytes, vec![1, 2, 3]);
    assert_eq!(arch.as_deref(), Some("sm89"));
}

#[test]
fn extract_binary_from_top_level_base64() {
    let (bytes, arch) = extract_binary(&serde_json::json!({ "binary": "AQID" })).unwrap();
    assert_eq!(bytes, vec![1, 2, 3]);
    assert!(arch.is_none());
}

#[test]
fn extract_binary_top_level_arch_is_optional() {
    let (bytes, arch) = extract_binary(&serde_json::json!({
        "binary": [9],
        "arch": "sm90"
    }))
    .unwrap();
    assert_eq!(bytes, vec![9]);
    assert_eq!(arch.as_deref(), Some("sm90"));
}

#[tokio::test]
async fn shader_dispatch_rejects_empty_binary_after_decode() {
    let handler = DispatchHandler::new(create_visualization_client(), None);
    let params = serde_json::json!({
        "binary": [],
        "bdf": "0000:00:00.0",
        "dispatch_mode": "drm",
    });
    let err = handler.shader_dispatch(Some(&params)).await.unwrap_err();
    assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
    assert!(err.message.contains("empty"));
}

#[tokio::test]
async fn shader_dispatch_vfio_without_shader_service_returns_failed_capability_response() {
    let handler = DispatchHandler::new(create_visualization_client(), None);
    let params = serde_json::json!({
        "binary": [1u8],
        "bdf": "0000:00:00.0",
        "dispatch_mode": "vfio",
    });
    let result = handler
        .shader_dispatch(Some(&params))
        .await
        .expect("handler returns Ok JSON envelope on service miss");
    assert_eq!(result["status"], "failed", "result: {result}");
    assert_eq!(result["domain"], "compute.dispatch");
    assert_eq!(result["operation"], "shader");
    assert!(
        result["error"]
            .as_str()
            .is_some_and(|s| s.contains("shader") || s.contains("dispatch")),
        "error field should mention shader or dispatch, got: {:?}",
        result["error"]
    );
    let meta = result["metadata"].as_object().expect("metadata object");
    assert_eq!(meta["dispatch_mode"], "vfio");
    assert_eq!(meta["bdf"], "0000:00:00.0");
}
