// SPDX-License-Identifier: AGPL-3.0-or-later
//! Core dispatch handler tests — capabilities, submit, status, result, forward, crypto paths.

use super::{DispatchHandler, JsonRpcError, submit_params, test_handler};
use crate::pure_jsonrpc::types::JsonRpcErrorExt;

#[tokio::test]
async fn dispatch_capabilities_returns_expected_structure() {
    let handler = test_handler();
    let result = handler
        .dispatch_capabilities(None)
        .await
        .expect("capabilities");
    assert_eq!(result["domain"], "compute.dispatch");
    assert_eq!(result["operation"], "capabilities");
    assert_eq!(result["status"], "completed");
    assert!(result["output"]["sovereign_pipeline"].as_bool().unwrap());
    assert!(result["output"]["dispatch_modes"].as_array().is_some());
    assert!(result["output"]["vfio_gpus"].as_array().is_some());
    assert!(result["output"]["drm_gpus"].as_array().is_some());
    assert!(result["output"]["total_dispatch_count"].as_u64().is_some());
    assert!(result["output"]["shader_compiler_available"].is_boolean());
    assert!(result["output"]["gpu_count"].is_u64());
    assert!(result["output"]["architectures"].as_array().is_some());
    assert!(result["output"]["vfio_status"]["available"].is_boolean());
    assert!(result["output"]["vfio_status"]["device_count"].is_u64());
}

#[tokio::test]
async fn dispatch_capabilities_total_dispatch_count_increments_after_submit() {
    let handler = test_handler();
    let before = handler
        .dispatch_capabilities(None)
        .await
        .expect("capabilities")["output"]["total_dispatch_count"]
        .as_u64()
        .expect("total_dispatch_count");

    let params = submit_params("0000:03:00.0", "passthrough");
    handler
        .dispatch_submit(Some(&params))
        .await
        .expect("submit");

    let after = handler
        .dispatch_capabilities(None)
        .await
        .expect("capabilities")["output"]["total_dispatch_count"]
        .as_u64()
        .expect("total_dispatch_count");
    assert_eq!(after, before + 1);
}

#[tokio::test]
async fn dispatch_submit_missing_params_returns_invalid_params() {
    let handler = test_handler();
    let err = handler
        .dispatch_submit(None)
        .await
        .expect_err("expected error");
    assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
    assert!(err.message.contains("binary"));
}

#[tokio::test]
async fn dispatch_submit_empty_binary_returns_invalid_params() {
    let handler = test_handler();
    let params = serde_json::json!({ "binary": [] });
    let err = handler
        .dispatch_submit(Some(&params))
        .await
        .expect_err("expected error");
    assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
    assert!(err.message.contains("empty"));
}

#[tokio::test]
async fn dispatch_submit_missing_binary_field_returns_invalid_params() {
    let handler = test_handler();
    let params = serde_json::json!({ "bdf": "0000:03:00.0" });
    let err = handler
        .dispatch_submit(Some(&params))
        .await
        .expect_err("expected error");
    assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
}

#[tokio::test]
async fn dispatch_submit_binary_not_array_returns_invalid_params() {
    let handler = test_handler();
    let params = serde_json::json!({
        "binary": "not-an-array",
        "bdf": "0000:03:00.0",
    });
    let err = handler
        .dispatch_submit(Some(&params))
        .await
        .expect_err("expected error");
    assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
}

#[tokio::test]
async fn dispatch_submit_vfio_mode_without_coral_returns_failed_payload() {
    let coral = crate::visualization_client::create_visualization_client();
    if coral.is_available().await {
        return;
    }
    let handler = DispatchHandler::new(coral, None);
    let params = submit_params("0000:03:00.0", "vfio");
    let result = handler
        .dispatch_submit(Some(&params))
        .await
        .expect("submit");
    assert_eq!(result["domain"], "compute.dispatch");
    assert_eq!(result["status"], "failed");
    assert!(
        result["error"]
            .as_str()
            .is_some_and(|s| s.contains("visualization") || s.contains("shader"))
    );
}

#[tokio::test]
async fn dispatch_submit_drm_mode_without_coral_returns_failed_payload() {
    let coral = crate::visualization_client::create_visualization_client();
    if coral.is_available().await {
        return;
    }
    let handler = DispatchHandler::new(coral, None);
    let params = submit_params("0000:03:00.0", "drm");
    let result = handler
        .dispatch_submit(Some(&params))
        .await
        .expect("submit");
    assert_eq!(result["status"], "failed");
    assert!(
        result["error"]
            .as_str()
            .is_some_and(|s| s.contains("visualization") || s.contains("shader"))
    );
}

#[tokio::test]
async fn dispatch_submit_custom_dispatch_mode_registers_job_for_status_and_result() {
    let handler = test_handler();
    let params = serde_json::json!({
        "binary": [1u8, 2, 3],
        "bdf": "0000:03:00.0",
        "dispatch_mode": "passthrough",
        "workgroup_size": [128, 2, 4],
        "buffers": [{ "name": "a", "size": 16 }],
        "timeout_ms": 9999u64,
    });
    let result = handler
        .dispatch_submit(Some(&params))
        .await
        .expect("submit");
    assert_eq!(result["domain"], "compute.dispatch");
    let job_id = result["job_id"].as_str().expect("job_id");
    assert_eq!(
        result["metadata"]["workgroup_size"],
        serde_json::json!([128, 2, 4])
    );

    let status = handler
        .dispatch_status(Some(&serde_json::json!({ "job_id": job_id })))
        .await
        .expect("status");
    assert_eq!(status["job_id"], job_id);
    assert!(status["status"].as_str().is_some());
    assert_eq!(status["metadata"]["bdf"], "0000:03:00.0");

    let got = handler
        .dispatch_result(Some(&serde_json::json!({ "job_id": job_id })))
        .await
        .expect("result");
    assert_eq!(got["job_id"], job_id);
}

#[tokio::test]
async fn dispatch_status_unknown_job_returns_error() {
    let handler = test_handler();
    let params = serde_json::json!({ "job_id": "nonexistent-uuid" });
    let err = handler
        .dispatch_status(Some(&params))
        .await
        .expect_err("expected error");
    assert_eq!(err.code, JsonRpcError::INTERNAL_ERROR);
    assert!(err.message.contains("not found"));
}

#[tokio::test]
async fn dispatch_status_missing_job_id_returns_invalid_params() {
    let handler = test_handler();
    let err = handler
        .dispatch_status(None)
        .await
        .expect_err("expected error");
    assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);

    let err = handler
        .dispatch_status(Some(&serde_json::json!({})))
        .await
        .expect_err("expected error");
    assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
}

#[tokio::test]
async fn dispatch_status_job_id_not_string_returns_invalid_params() {
    let handler = test_handler();
    let params = serde_json::json!({ "job_id": 12345 });
    let err = handler
        .dispatch_status(Some(&params))
        .await
        .expect_err("expected error");
    assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
}

#[tokio::test]
async fn dispatch_result_unknown_job_returns_error() {
    let handler = test_handler();
    let params = serde_json::json!({ "job_id": "nonexistent-uuid" });
    let err = handler
        .dispatch_result(Some(&params))
        .await
        .expect_err("expected error");
    assert_eq!(err.code, JsonRpcError::INTERNAL_ERROR);
    assert!(err.message.contains("not found"));
}

#[tokio::test]
async fn dispatch_result_missing_job_id_returns_invalid_params() {
    let handler = test_handler();
    let err = handler
        .dispatch_result(None)
        .await
        .expect_err("expected error");
    assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
}

#[tokio::test]
async fn dispatch_result_job_id_not_string_returns_invalid_params() {
    let handler = test_handler();
    let params = serde_json::json!({ "job_id": true });
    let err = handler
        .dispatch_result(Some(&params))
        .await
        .expect_err("expected error");
    assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
}

#[tokio::test]
async fn dispatch_forward_missing_params_returns_invalid_params() {
    let handler = test_handler();
    let err = handler
        .dispatch_forward(None)
        .await
        .expect_err("expected error");
    assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
}

#[tokio::test]
async fn dispatch_forward_missing_endpoint_returns_invalid_params() {
    let handler = test_handler();
    let params = serde_json::json!({ "binary": [1] });
    let err = handler
        .dispatch_forward(Some(&params))
        .await
        .expect_err("expected error");
    assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
    assert!(err.message.contains("endpoint"));
}

#[tokio::test]
async fn dispatch_forward_unreachable_returns_internal_error() {
    let handler = test_handler();
    let params = serde_json::json!({
        "endpoint": "127.0.0.1:1",
        "binary": [1, 2],
        "bdf": "0000:03:00.0",
    });
    let err = handler
        .dispatch_forward(Some(&params))
        .await
        .expect_err("expected error");
    assert_eq!(err.code, JsonRpcError::INTERNAL_ERROR);
    assert!(err.message.contains("127.0.0.1:1") || err.message.contains("failed"));
}

#[tokio::test]
async fn dispatch_forward_uses_nested_params_when_present() {
    let handler = test_handler();
    let params = serde_json::json!({
        "endpoint": "127.0.0.1:1",
        "params": {
            "binary": [9],
            "bdf": "0000:03:00.0",
        },
    });
    let err = handler
        .dispatch_forward(Some(&params))
        .await
        .expect_err("expected transport error");
    assert_eq!(err.code, JsonRpcError::INTERNAL_ERROR);
}

#[tokio::test]
async fn dispatch_submit_standalone_mode_has_no_encrypted_flag() {
    let handler = test_handler();
    let params = submit_params("0000:03:00.0", "passthrough");
    let result = handler
        .dispatch_submit(Some(&params))
        .await
        .expect("submit");
    assert!(
        result["metadata"].get("encrypted").is_none() || result["metadata"]["encrypted"] == false,
        "standalone dispatch must not set encrypted=true"
    );
}

#[tokio::test]
async fn dispatch_handler_new_with_none_security_client_works() {
    let handler = DispatchHandler::new(
        crate::visualization_client::create_visualization_client(),
        None,
    );
    let params = submit_params("0000:03:00.0", "passthrough");
    let result = handler
        .dispatch_submit(Some(&params))
        .await
        .expect("submit");
    assert_eq!(result["domain"], "compute.dispatch");
}
