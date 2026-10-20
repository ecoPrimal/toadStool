// SPDX-License-Identifier: AGPL-3.0-or-later
//! Wave 8 trio-standard IPC contract tests — binary_b64, dispatch_dims, shader_info, timing.

use super::{JsonRpcError, submit_params, test_handler};


#[tokio::test]
async fn dispatch_submit_accepts_binary_b64() {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let handler = test_handler();
    let binary_data = vec![0xDE, 0xAD, 0xBE, 0xEF];
    let params = serde_json::json!({
        "binary_b64": STANDARD.encode(&binary_data),
        "bdf": "0000:03:00.0",
        "dispatch_mode": "passthrough",
    });
    let result = handler
        .dispatch_submit(Some(&params))
        .await
        .expect("binary_b64 should be accepted");
    assert_eq!(result["domain"], "compute.dispatch");
    assert_eq!(result["metadata"]["binary_size"], binary_data.len());
    assert!(result["job_id"].as_str().is_some());
}

#[tokio::test]
async fn dispatch_submit_binary_b64_preferred_over_binary() {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let handler = test_handler();
    let preferred = vec![0xCA, 0xFE];
    let params = serde_json::json!({
        "binary_b64": STANDARD.encode(&preferred),
        "binary": [1, 2, 3],
        "bdf": "0000:03:00.0",
        "dispatch_mode": "passthrough",
    });
    let result = handler
        .dispatch_submit(Some(&params))
        .await
        .expect("binary_b64 should take precedence");
    assert_eq!(result["metadata"]["binary_size"], 2);
}

#[tokio::test]
async fn dispatch_submit_invalid_binary_b64_returns_error() {
    let handler = test_handler();
    let params = serde_json::json!({
        "binary_b64": "!!!invalid-base64!!!",
        "bdf": "0000:03:00.0",
    });
    let err = handler
        .dispatch_submit(Some(&params))
        .await
        .expect_err("invalid base64 should fail");
    assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
    assert!(err.message.contains("base64"));
}

#[tokio::test]
async fn dispatch_submit_accepts_dispatch_dims() {
    let handler = test_handler();
    let params = serde_json::json!({
        "binary": [1, 2, 3],
        "bdf": "0000:03:00.0",
        "dispatch_dims": [64, 4, 2],
        "dispatch_mode": "passthrough",
    });
    let result = handler
        .dispatch_submit(Some(&params))
        .await
        .expect("dispatch_dims should be accepted");
    assert_eq!(
        result["metadata"]["workgroup_size"],
        serde_json::json!([64, 4, 2])
    );
}

#[tokio::test]
async fn dispatch_submit_dispatch_dims_preferred_over_workgroup_size() {
    let handler = test_handler();
    let params = serde_json::json!({
        "binary": [1, 2, 3],
        "bdf": "0000:03:00.0",
        "dispatch_dims": [32, 1, 1],
        "workgroup_size": [128, 2, 4],
        "dispatch_mode": "passthrough",
    });
    let result = handler
        .dispatch_submit(Some(&params))
        .await
        .expect("dispatch_dims should take precedence");
    assert_eq!(
        result["metadata"]["workgroup_size"],
        serde_json::json!([32, 1, 1])
    );
}

#[tokio::test]
async fn dispatch_submit_response_includes_timing() {
    let handler = test_handler();
    let params = submit_params("0000:03:00.0", "passthrough");
    let result = handler
        .dispatch_submit(Some(&params))
        .await
        .expect("submit");
    let timing = &result["timing"];
    assert!(
        timing["dispatch_ms"].is_u64(),
        "timing.dispatch_ms should be present"
    );
    assert!(
        timing["readback_ms"].is_u64(),
        "timing.readback_ms should be present"
    );
}

#[tokio::test]
async fn dispatch_submit_accepts_shader_info() {
    let handler = test_handler();
    let params = serde_json::json!({
        "binary": [1, 2, 3],
        "bdf": "0000:03:00.0",
        "dispatch_mode": "passthrough",
        "shader_info": {
            "gprs": 32,
            "shared_memory": 16384,
            "barriers": 1,
            "workgroup": [256, 1, 1],
            "wave_size": 32,
        },
    });
    let result = handler
        .dispatch_submit(Some(&params))
        .await
        .expect("shader_info should be accepted");
    assert_eq!(result["metadata"]["shader_info"]["gprs"], 32);
    assert_eq!(result["metadata"]["shader_info"]["wave_size"], 32);
}

#[tokio::test]
async fn dispatch_submit_buffers_with_data_b64_decoded() {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let handler = test_handler();
    let buf_data = vec![0x01, 0x02, 0x03, 0x04];
    let params = serde_json::json!({
        "binary": [1, 2, 3],
        "bdf": "0000:03:00.0",
        "dispatch_mode": "passthrough",
        "buffers": [
            { "binding": 0, "data_b64": STANDARD.encode(&buf_data), "size": 4, "usage": "storage" },
            { "binding": 1, "size": 64, "usage": "uniform" },
        ],
    });
    let result = handler
        .dispatch_submit(Some(&params))
        .await
        .expect("buffers with data_b64 should be accepted");
    assert!(result["job_id"].as_str().is_some());
}

#[tokio::test]
async fn dispatch_submit_no_binary_or_binary_b64_returns_error() {
    let handler = test_handler();
    let params = serde_json::json!({
        "bdf": "0000:03:00.0",
        "dispatch_mode": "passthrough",
    });
    let err = handler
        .dispatch_submit(Some(&params))
        .await
        .expect_err("missing both binary fields should fail");
    assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
    assert!(err.message.contains("binary") || err.message.contains("binary_b64"));
}

// ── Phase A+B: ember device lifecycle integration ──

#[tokio::test]
async fn dispatch_submit_acquires_device_handle() {
    let handler = test_handler();
    assert_eq!(handler.held_device_count().await, 0);

    let params = submit_params("0000:03:00.0", "passthrough");
    handler
        .dispatch_submit(Some(&params))
        .await
        .expect("submit");
    assert_eq!(
        handler.held_device_count().await,
        1,
        "dispatch should acquire a device handle"
    );
}

#[tokio::test]
async fn dispatch_submit_reuses_handle_for_same_bdf() {
    let handler = test_handler();
    let params = submit_params("0000:03:00.0", "passthrough");
    handler
        .dispatch_submit(Some(&params))
        .await
        .expect("first submit");
    handler
        .dispatch_submit(Some(&params))
        .await
        .expect("second submit");
    assert_eq!(
        handler.held_device_count().await,
        1,
        "same BDF should reuse the existing handle"
    );
}

#[tokio::test]
async fn dispatch_submit_separate_handles_per_bdf() {
    let handler = test_handler();
    let p1 = submit_params("0000:03:00.0", "passthrough");
    let p2 = submit_params("0000:4a:00.0", "passthrough");
    handler.dispatch_submit(Some(&p1)).await.expect("submit 1");
    handler.dispatch_submit(Some(&p2)).await.expect("submit 2");
    assert_eq!(
        handler.held_device_count().await,
        2,
        "different BDFs should have separate handles"
    );
}

#[tokio::test]
async fn capabilities_includes_ember_info() {
    let handler = test_handler();
    let result = handler
        .dispatch_capabilities(None)
        .await
        .expect("capabilities");
    let ember = &result["output"]["ember"];
    assert_eq!(ember["phase"], "D");
    assert!(
        ember["held_devices"].is_u64(),
        "ember.held_devices should be present"
    );
    assert!(
        ember.get("local_dispatch").is_some(),
        "ember.local_dispatch should be present"
    );
}

// ── Phase B: glowplug orchestration integration ──

#[tokio::test]
async fn capabilities_includes_glowplug_info() {
    let handler = test_handler();
    let result = handler
        .dispatch_capabilities(None)
        .await
        .expect("capabilities");
    let glowplug = &result["output"]["glowplug"];
    assert_eq!(glowplug["lifecycle_steps"], 7);
    assert!(
        glowplug["personalities"].as_array().is_some(),
        "glowplug.personalities should be an array"
    );
    let personalities = glowplug["personalities"].as_array().unwrap();
    assert!(
        personalities.len() >= 10,
        "should have at least 10 personalities"
    );
    assert!(personalities.iter().any(|p| p == "vfio"));
    assert!(personalities.iter().any(|p| p == "akida"));
}

#[tokio::test]
async fn capabilities_glowplug_has_orchestrator_type() {
    let handler = test_handler();
    let result = handler
        .dispatch_capabilities(None)
        .await
        .expect("capabilities");
    let orch = &result["output"]["glowplug"]["orchestrator"];
    assert_eq!(orch, "SwapOrchestrator<SysfsSwapExecutor>");
}

// ── Shader metadata alias resolution (coralReef CompileResponse compat) ──

#[test]
fn resolve_shader_info_accepts_toadstool_native_names() {
    use crate::pure_jsonrpc::handler::dispatch::submit_params::resolve_shader_info;
    let si = serde_json::json!({
        "gpr_count": 32,
        "shared_mem_bytes": 16384,
        "barrier_count": 2,
        "wave_size": 32,
        "local_mem_bytes": 512,
    });
    let info = resolve_shader_info(&si, [256, 1, 1]);
    assert_eq!(info.gpr_count, 32);
    assert_eq!(info.shared_mem_bytes, 16384);
    assert_eq!(info.barrier_count, 2);
    assert_eq!(info.wave_size, 32);
    assert_eq!(info.local_mem_bytes, Some(512));
    assert_eq!(info.workgroup, [256, 1, 1]);
}

#[test]
fn resolve_shader_info_accepts_coralreef_field_names() {
    use crate::pure_jsonrpc::handler::dispatch::submit_params::resolve_shader_info;
    let si = serde_json::json!({
        "gprs": 48,
        "shared_memory": 32768,
        "barriers": 3,
        "wave_size": 32,
        "local_memory": 1024,
    });
    let info = resolve_shader_info(&si, [128, 2, 1]);
    assert_eq!(info.gpr_count, 48);
    assert_eq!(info.shared_mem_bytes, 32768);
    assert_eq!(info.barrier_count, 3);
    assert_eq!(info.local_mem_bytes, Some(1024));
    assert_eq!(info.workgroup, [128, 2, 1]);
}

#[test]
fn resolve_shader_info_native_name_preferred_over_alias() {
    use crate::pure_jsonrpc::handler::dispatch::submit_params::resolve_shader_info;
    let si = serde_json::json!({
        "gpr_count": 32,
        "gprs": 64,
        "shared_mem_bytes": 8192,
        "shared_memory": 16384,
    });
    let info = resolve_shader_info(&si, [256, 1, 1]);
    assert_eq!(
        info.gpr_count, 32,
        "native gpr_count should win over gprs alias"
    );
    assert_eq!(
        info.shared_mem_bytes, 8192,
        "native shared_mem_bytes should win"
    );
}

#[test]
fn resolve_shader_info_defaults_without_metadata() {
    use crate::pure_jsonrpc::handler::dispatch::submit_params::resolve_shader_info;
    let si = serde_json::json!({});
    let info = resolve_shader_info(&si, [64, 1, 1]);
    assert_eq!(info.gpr_count, 0);
    assert_eq!(info.shared_mem_bytes, 0);
    assert_eq!(info.barrier_count, 0);
    assert_eq!(info.wave_size, 32);
    assert_eq!(info.local_mem_bytes, None);
}

// ── device.gr.init validation ──

#[tokio::test]
async fn device_gr_init_requires_bdf() {
    let handler = test_handler();
    let params = serde_json::json!({
        "method_entries": [[0x900, 0x1234]],
    });
    let err = handler
        .device_gr_init(Some(&params))
        .await
        .expect_err("missing bdf should fail");
    assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
}

#[tokio::test]
async fn device_gr_init_requires_method_entries() {
    let handler = test_handler();
    let params = serde_json::json!({
        "bdf": "0000:01:00.0",
    });
    let err = handler
        .device_gr_init(Some(&params))
        .await
        .expect_err("missing method_entries should fail");
    assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
}

#[tokio::test]
async fn device_gr_init_rejects_empty_entries() {
    let handler = test_handler();
    let params = serde_json::json!({
        "bdf": "0000:01:00.0",
        "method_entries": [],
    });
    let err = handler
        .device_gr_init(Some(&params))
        .await
        .expect_err("empty entries should fail");
    assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
}

// NOTE: compute.fan_out tests live in fan_out.rs (re-implemented S269 / Wave 38).
