// SPDX-License-Identifier: AGPL-3.0-or-later
//! `shader.dispatch` tests — binary formats, compile_result shapes, readback, job tracking.

use super::{DispatchHandler, JsonRpcError, test_handler};

use crate::pure_jsonrpc::handler::method_gate::{CallerContext, ResourceEnvelope};

#[tokio::test]
async fn shader_dispatch_missing_params_returns_invalid_params() {
    let handler = test_handler();
    let err = handler
        .shader_dispatch(None)
        .await
        .expect_err("expected error");
    assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
    assert!(err.message.contains("binary"));
}

#[tokio::test]
async fn shader_dispatch_missing_binary_returns_invalid_params() {
    let handler = test_handler();
    let params = serde_json::json!({ "bdf": "0000:03:00.0" });
    let err = handler
        .shader_dispatch(Some(&params))
        .await
        .expect_err("expected error");
    assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
    assert!(err.message.contains("binary") || err.message.contains("compile_result"));
}

#[tokio::test]
async fn shader_dispatch_empty_binary_array_returns_invalid_params() {
    let handler = test_handler();
    let params = serde_json::json!({ "binary": [], "bdf": "0000:03:00.0" });
    let err = handler
        .shader_dispatch(Some(&params))
        .await
        .expect_err("expected error");
    assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
    assert!(err.message.contains("empty"));
}

#[tokio::test]
async fn shader_dispatch_empty_base64_returns_invalid_params() {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let handler = test_handler();
    let params = serde_json::json!({
        "binary": STANDARD.encode([]),
        "bdf": "0000:03:00.0"
    });
    let err = handler
        .shader_dispatch(Some(&params))
        .await
        .expect_err("expected error");
    assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
}

#[tokio::test]
async fn shader_dispatch_invalid_base64_returns_invalid_params() {
    let handler = test_handler();
    let params = serde_json::json!({
        "binary": "!!!not-valid-base64!!!",
        "bdf": "0000:03:00.0"
    });
    let err = handler
        .shader_dispatch(Some(&params))
        .await
        .expect_err("expected error");
    assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
    assert!(err.message.contains("base64"));
}

#[tokio::test]
async fn shader_dispatch_binary_not_string_or_array_returns_invalid_params() {
    let handler = test_handler();
    let params = serde_json::json!({
        "binary": 42,
        "bdf": "0000:03:00.0"
    });
    let err = handler
        .shader_dispatch(Some(&params))
        .await
        .expect_err("expected error");
    assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
    assert!(err.message.contains("base64 string") || err.message.contains("array"));
}

#[tokio::test]
async fn shader_dispatch_accepts_base64_binary() {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let handler = test_handler();
    let binary_data = vec![0xDE, 0xAD, 0xBE, 0xEF, 0x01, 0x02];
    let params = serde_json::json!({
        "binary": STANDARD.encode(&binary_data),
        "bdf": "0000:03:00.0",
        "dispatch_mode": "passthrough",
    });
    let result = handler
        .shader_dispatch(Some(&params))
        .await
        .expect("shader dispatch");
    assert_eq!(result["domain"], "compute.dispatch");
    assert_eq!(result["operation"], "shader");
    assert_eq!(result["metadata"]["binary_size"], binary_data.len());
    assert!(result["job_id"].as_str().is_some());
}

#[tokio::test]
async fn shader_dispatch_accepts_u8_array_binary() {
    let handler = test_handler();
    let params = serde_json::json!({
        "binary": [0xDE, 0xAD, 0xBE, 0xEF],
        "bdf": "0000:03:00.0",
        "dispatch_mode": "passthrough",
    });
    let result = handler
        .shader_dispatch(Some(&params))
        .await
        .expect("shader dispatch");
    assert_eq!(result["domain"], "compute.dispatch");
    assert_eq!(result["metadata"]["binary_size"], 4);
}

#[tokio::test]
async fn shader_dispatch_accepts_compile_result_shape() {
    let handler = test_handler();
    let params = serde_json::json!({
        "compile_result": {
            "binary": [1, 2, 3, 4, 5],
            "arch": "sm89",
            "target_device": 0
        },
        "workgroup_size": [64, 1, 1],
        "bdf": "0000:03:00.0",
        "dispatch_mode": "passthrough",
    });
    let result = handler
        .shader_dispatch(Some(&params))
        .await
        .expect("shader dispatch");
    assert_eq!(result["domain"], "compute.dispatch");
    assert_eq!(result["metadata"]["arch"], "sm89");
    assert_eq!(result["metadata"]["binary_size"], 5);
    assert_eq!(
        result["metadata"]["workgroup_size"],
        serde_json::json!([64, 1, 1])
    );
}

#[tokio::test]
async fn shader_dispatch_compile_result_base64_binary() {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let handler = test_handler();
    let binary = vec![0x90, 0x90, 0x90];
    let params = serde_json::json!({
        "compile_result": {
            "binary": STANDARD.encode(&binary),
            "arch": "gfx1030",
        },
        "bdf": "0000:03:00.0",
        "dispatch_mode": "passthrough",
    });
    let result = handler
        .shader_dispatch(Some(&params))
        .await
        .expect("shader dispatch");
    assert_eq!(result["metadata"]["arch"], "gfx1030");
    assert_eq!(result["metadata"]["binary_size"], 3);
}

#[tokio::test]
async fn shader_dispatch_compile_result_missing_binary_returns_invalid_params() {
    let handler = test_handler();
    let params = serde_json::json!({
        "compile_result": { "arch": "sm89" },
        "bdf": "0000:03:00.0",
    });
    let err = handler
        .shader_dispatch(Some(&params))
        .await
        .expect_err("expected error");
    assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
    assert!(err.message.contains("binary"));
}

#[tokio::test]
async fn shader_dispatch_readback_defaults_to_true() {
    let handler = test_handler();
    let params = serde_json::json!({
        "binary": [1, 2, 3],
        "bdf": "0000:03:00.0",
        "dispatch_mode": "passthrough",
    });
    let result = handler
        .shader_dispatch(Some(&params))
        .await
        .expect("shader dispatch");
    assert_eq!(result["metadata"]["readback"], true);
}

#[tokio::test]
async fn shader_dispatch_readback_false_honored() {
    let handler = test_handler();
    let params = serde_json::json!({
        "binary": [1, 2, 3],
        "bdf": "0000:03:00.0",
        "dispatch_mode": "passthrough",
        "readback": false,
    });
    let result = handler
        .shader_dispatch(Some(&params))
        .await
        .expect("shader dispatch");
    assert_eq!(result["metadata"]["readback"], false);
}

#[tokio::test]
async fn shader_dispatch_vfio_without_coral_returns_failed() {
    let coral = crate::visualization_client::create_visualization_client();
    if coral.is_available().await {
        return;
    }
    let handler = DispatchHandler::new(coral, None);
    let params = serde_json::json!({
        "binary": [1, 2, 3],
        "bdf": "0000:03:00.0",
        "dispatch_mode": "vfio",
    });
    let result = handler
        .shader_dispatch(Some(&params))
        .await
        .expect("shader dispatch");
    assert_eq!(result["domain"], "compute.dispatch");
    assert_eq!(result["status"], "failed");
    assert!(
        result["error"]
            .as_str()
            .is_some_and(|s| s.contains("visualization") || s.contains("shader"))
    );
}

#[tokio::test]
async fn shader_dispatch_increments_dispatch_count() {
    let handler = test_handler();
    let before =
        handler.dispatch_capabilities(None).await.expect("caps")["output"]["total_dispatch_count"]
            .as_u64()
            .expect("count");

    let params = serde_json::json!({
        "binary": [1, 2],
        "bdf": "0000:03:00.0",
        "dispatch_mode": "passthrough",
    });
    handler
        .shader_dispatch(Some(&params))
        .await
        .expect("shader dispatch");

    let after =
        handler.dispatch_capabilities(None).await.expect("caps")["output"]["total_dispatch_count"]
            .as_u64()
            .expect("count");
    assert_eq!(after, before + 1);
}

#[tokio::test]
async fn shader_dispatch_job_trackable_via_status_and_result() {
    let handler = test_handler();
    let params = serde_json::json!({
        "binary": [0xCA, 0xFE],
        "bdf": "0000:03:00.0",
        "dispatch_mode": "passthrough",
        "workgroup_size": [32, 2, 1],
    });
    let result = handler
        .shader_dispatch(Some(&params))
        .await
        .expect("shader dispatch");
    let job_id = result["job_id"].as_str().expect("job_id");

    let status = handler
        .dispatch_status(Some(&serde_json::json!({ "job_id": job_id })))
        .await
        .expect("status");
    assert_eq!(status["job_id"], job_id);
    assert!(status["status"].as_str().is_some());

    let got = handler
        .dispatch_result(Some(&serde_json::json!({ "job_id": job_id })))
        .await
        .expect("result");
    assert_eq!(got["job_id"], job_id);
}

fn ctx_with_envelope(cpu_cores: u32) -> CallerContext {
    CallerContext {
        envelope: Some(ResourceEnvelope {
            mem_mb: None,
            cpu_cores: Some(cpu_cores),
            max_timeout_ms: None,
            method_allowlist: vec![],
        }),
        ..CallerContext::anonymous()
    }
}

#[tokio::test]
async fn shader_dispatch_default_workgroup_size() {
    let handler = test_handler();
    let params = serde_json::json!({
        "binary": [1, 2, 3],
        "bdf": "0000:03:00.0",
        "dispatch_mode": "passthrough",
    });
    let result = handler
        .shader_dispatch(Some(&params))
        .await
        .expect("shader dispatch");
    assert_eq!(
        result["metadata"]["workgroup_size"],
        serde_json::json!([256, 1, 1])
    );
}

#[tokio::test]
async fn shader_dispatch_partial_workgroup_size_uses_defaults() {
    let handler = test_handler();
    let params = serde_json::json!({
        "binary": [1],
        "bdf": "0000:03:00.0",
        "dispatch_mode": "passthrough",
        "workgroup_size": [64],
    });
    let result = handler
        .shader_dispatch(Some(&params))
        .await
        .expect("shader dispatch");
    assert_eq!(
        result["metadata"]["workgroup_size"],
        serde_json::json!([64, 1, 1])
    );
}

#[tokio::test]
async fn shader_dispatch_workgroup_envelope_rejects_oversized_total() {
    let handler = test_handler();
    let params = serde_json::json!({
        "binary": [1, 2, 3],
        "bdf": "0000:03:00.0",
        "dispatch_mode": "passthrough",
        "workgroup_size": [2048, 2, 1],
    });
    let ctx = ctx_with_envelope(2);
    let err = handler
        .shader_dispatch_with_context(Some(&params), &ctx)
        .await
        .expect_err("workgroup total exceeds cpu envelope");
    assert_eq!(
        err.code,
        toadstool_common::constants::jsonrpc::error_codes::RESOURCE_EXHAUSTED
    );
    assert!(err.message.contains("Workgroup total"));
}

#[tokio::test]
async fn shader_dispatch_drm_without_shader_service_returns_failed_envelope() {
    let handler = test_handler();
    let params = serde_json::json!({
        "binary": [1, 2, 3],
        "bdf": "0000:03:00.0",
        "dispatch_mode": "drm",
    });
    let result = handler
        .shader_dispatch(Some(&params))
        .await
        .expect("handler returns Ok JSON envelope on service miss");
    assert_eq!(result["domain"], "compute.dispatch");
    assert_eq!(result["operation"], "shader");
    assert_eq!(result["status"], "failed");
    assert!(result["output"].is_null());
    assert!(
        result["error"]
            .as_str()
            .is_some_and(|s| s.contains("shader") || s.contains("visualization"))
    );
    assert_eq!(result["metadata"]["dispatch_mode"], "drm");
    assert!(result["job_id"].as_str().is_some());
}

#[tokio::test]
async fn shader_dispatch_failed_response_has_consistent_shape() {
    let handler = test_handler();
    let params = serde_json::json!({
        "binary": [1],
        "bdf": "0000:03:00.0",
        "dispatch_mode": "vfio",
    });
    let result = handler
        .shader_dispatch(Some(&params))
        .await
        .expect("failed dispatch still returns envelope");
    for key in [
        "domain",
        "operation",
        "job_id",
        "status",
        "output",
        "error",
        "metadata",
    ] {
        assert!(result.get(key).is_some(), "missing key: {key}");
    }
    assert_eq!(result["status"], "failed");
    assert!(result["metadata"]["bdf"].as_str().is_some());
    assert!(result["metadata"]["binary_size"].as_u64().is_some());
}
