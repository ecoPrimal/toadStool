// SPDX-License-Identifier: AGPL-3.0-or-later
//! `dispatch/submit.rs` param resolution, thermal gate, and envelope integration tests.

use base64::Engine;

use super::{submit_params, test_handler};
use crate::pure_jsonrpc::handler::dispatch::routing::resolve_dispatch_target;
use crate::pure_jsonrpc::handler::dispatch::submit_params::{
    enforce_envelope, resolve_binary_param, resolve_buffers, resolve_shader_info,
    resolve_workgroup_size,
};
use crate::pure_jsonrpc::handler::method_gate::{CallerContext, ResourceEnvelope};
use crate::pure_jsonrpc::types::JsonRpcError;

fn ctx_with(env: ResourceEnvelope) -> CallerContext {
    CallerContext {
        identity: Some("did:key:z6Mk_test".into()),
        envelope: Some(env),
        ..CallerContext::anonymous()
    }
}

#[test]
fn resolve_binary_param_missing_binary_returns_invalid_params() {
    let params = serde_json::json!({ "bdf": "0000:03:00.0" });
    let err = resolve_binary_param(&params).unwrap_err();
    assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
    assert!(err.message.contains("binary") || err.message.contains("binary_b64"));
}

#[test]
fn resolve_binary_param_invalid_base64_returns_invalid_params() {
    let params = serde_json::json!({ "binary_b64": "!!!not-base64!!!" });
    let err = resolve_binary_param(&params).unwrap_err();
    assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
    assert!(err.message.contains("base64"));
}

#[test]
fn resolve_workgroup_size_defaults_without_dims() {
    assert_eq!(resolve_workgroup_size(&serde_json::json!({})), [256, 1, 1]);
}

#[test]
fn resolve_workgroup_size_prefers_dispatch_dims() {
    let params = serde_json::json!({
        "dispatch_dims": [32, 4, 2],
        "workgroup_size": [1, 1, 1],
    });
    assert_eq!(resolve_workgroup_size(&params), [32, 4, 2]);
}

#[test]
fn resolve_buffers_returns_empty_array_when_absent() {
    assert_eq!(
        resolve_buffers(&serde_json::json!({})),
        serde_json::json!([])
    );
}

#[test]
fn resolve_shader_info_accepts_coral_reef_field_aliases() {
    let info = serde_json::json!({
        "gprs": 32,
        "shared_memory": 4096,
        "barriers": 2,
        "wave_size": 64,
        "local_memory": 1024,
    });
    let shader = resolve_shader_info(&info, [128, 1, 1]);
    assert_eq!(shader.gpr_count, 32);
    assert_eq!(shader.shared_mem_bytes, 4096);
    assert_eq!(shader.barrier_count, 2);
    assert_eq!(shader.wave_size, 64);
    assert_eq!(shader.local_mem_bytes, Some(1024));
    assert_eq!(shader.workgroup, [128, 1, 1]);
}

#[test]
fn resolve_dispatch_target_honors_explicit_mode_string() {
    let params = serde_json::json!({ "bdf": "0000:03:00.0", "dispatch_mode": "passthrough" });
    let (bdf, mode) = resolve_dispatch_target(&params).expect("target");
    assert_eq!(bdf, "0000:03:00.0");
    assert_eq!(&*mode, "passthrough");
}

#[test]
fn resolve_dispatch_target_non_string_mode_falls_back_to_auto_detect() {
    let params = serde_json::json!({ "bdf": "0000:03:00.0", "dispatch_mode": 42 });
    let (_, mode) = resolve_dispatch_target(&params).expect("target");
    assert!(
        &*mode == "vfio" || &*mode == "drm",
        "non-string dispatch_mode should auto-detect, got {mode}"
    );
}

#[test]
fn thermal_critical_status_is_not_compute_safe() {
    assert!(!nvpmu::SafetyStatus::ThermalCritical.compute_safe());
}

#[tokio::test]
async fn dispatch_submit_missing_params_returns_invalid_params() {
    let handler = test_handler();
    let err = handler
        .dispatch_submit(None)
        .await
        .expect_err("missing params");
    assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
}

#[tokio::test]
async fn dispatch_submit_missing_binary_returns_invalid_params() {
    let handler = test_handler();
    let params = serde_json::json!({
        "bdf": "0000:03:00.0",
        "dispatch_mode": "passthrough",
    });
    let err = handler
        .dispatch_submit(Some(&params))
        .await
        .expect_err("missing binary");
    assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
}

#[tokio::test]
async fn dispatch_submit_custom_dispatch_mode_is_preserved_in_metadata() {
    let handler = test_handler();
    let params = serde_json::json!({
        "binary": [1, 2, 3],
        "bdf": "0000:03:00.0",
        "dispatch_mode": "custom-experimental-mode",
    });
    let result = handler
        .dispatch_submit(Some(&params))
        .await
        .expect("submit with custom mode");
    assert_eq!(
        result["metadata"]["dispatch_mode"],
        "custom-experimental-mode"
    );
}

#[tokio::test]
async fn dispatch_submit_passthrough_records_thermal_checked_flag() {
    let handler = test_handler();
    let params = submit_params("0000:03:00.0", "passthrough");
    let result = handler
        .dispatch_submit(Some(&params))
        .await
        .expect("passthrough submit");
    assert!(result["metadata"]["thermal_checked"].is_boolean());
}

#[tokio::test]
async fn dispatch_submit_with_context_envelope_rejects_cpu_cores() {
    let handler = test_handler();
    let params = serde_json::json!({
        "binary": [1, 2, 3],
        "bdf": "0000:03:00.0",
        "dispatch_mode": "passthrough",
        "dispatch_dims": [2048, 2, 1],
    });
    let ctx = ctx_with(ResourceEnvelope {
        mem_mb: None,
        cpu_cores: Some(1),
        max_timeout_ms: None,
        method_allowlist: vec![],
    });
    let err = handler
        .dispatch_submit_with_context(Some(&params), &ctx)
        .await
        .unwrap_err();
    assert_eq!(
        err.code,
        toadstool_common::constants::jsonrpc::error_codes::RESOURCE_EXHAUSTED
    );
    assert!(err.message.contains("cpu_cores"));
}

#[tokio::test]
async fn dispatch_submit_with_context_envelope_rejects_all_limits() {
    let handler = test_handler();
    let mut large_binary = vec![0u8; 3 * 1024 * 1024];
    large_binary[0] = 1;
    let params = serde_json::json!({
        "binary": large_binary,
        "bdf": "0000:03:00.0",
        "dispatch_mode": "passthrough",
        "dispatch_dims": [1024, 2, 2],
        "timeout_ms": 120_000,
    });
    let ctx = ctx_with(ResourceEnvelope {
        mem_mb: Some(1),
        cpu_cores: Some(1),
        max_timeout_ms: Some(30_000),
        method_allowlist: vec![],
    });
    let err = handler
        .dispatch_submit_with_context(Some(&params), &ctx)
        .await
        .unwrap_err();
    assert_eq!(
        err.code,
        toadstool_common::constants::jsonrpc::error_codes::RESOURCE_EXHAUSTED
    );
}

#[test]
fn resolve_dispatch_target_uses_explicit_bdf() {
    let params = serde_json::json!({ "bdf": "0000:ab:00.0" });
    let (bdf, _) = resolve_dispatch_target(&params).expect("target");
    assert_eq!(bdf, "0000:ab:00.0");
}

#[test]
fn enforce_envelope_without_limits_always_passes() {
    let ctx = ctx_with(ResourceEnvelope {
        mem_mb: None,
        cpu_cores: None,
        max_timeout_ms: None,
        method_allowlist: vec![],
    });
    assert!(enforce_envelope(&ctx, 1024, 4096, 60_000).is_ok());
}

#[test]
fn enforce_envelope_no_envelope_passes() {
    let ctx = CallerContext::anonymous();
    assert!(enforce_envelope(&ctx, 10 * 1024 * 1024, 999_999, 999_999).is_ok());
}

#[test]
fn enforce_envelope_mem_mb_rejects_oversized_binary() {
    let ctx = ctx_with(ResourceEnvelope {
        mem_mb: Some(1),
        cpu_cores: None,
        max_timeout_ms: None,
        method_allowlist: vec![],
    });
    let err = enforce_envelope(&ctx, 2 * 1024 * 1024, 0, 0).unwrap_err();
    assert!(err.message.contains("mem_mb"));
}

#[test]
fn enforce_envelope_timeout_rejects_excessive() {
    let ctx = ctx_with(ResourceEnvelope {
        mem_mb: None,
        cpu_cores: None,
        max_timeout_ms: Some(5_000),
        method_allowlist: vec![],
    });
    let err = enforce_envelope(&ctx, 0, 0, 10_000).unwrap_err();
    assert!(err.message.contains("timeout"));
}

#[test]
fn resolve_binary_param_valid_base64() {
    let data = [0xDE, 0xAD, 0xBE, 0xEF];
    let b64 = base64::engine::general_purpose::STANDARD.encode(data);
    let params = serde_json::json!({ "binary_b64": b64 });
    let result = resolve_binary_param(&params).unwrap();
    assert_eq!(result, data);
}

#[test]
fn resolve_binary_param_legacy_u8_array() {
    let params = serde_json::json!({ "binary": [1, 2, 3, 255] });
    let result = resolve_binary_param(&params).unwrap();
    assert_eq!(result, vec![1, 2, 3, 255]);
}

#[test]
fn resolve_binary_param_b64_preferred_over_legacy() {
    let b64 = base64::engine::general_purpose::STANDARD.encode([42]);
    let params = serde_json::json!({
        "binary_b64": b64,
        "binary": [99, 99, 99],
    });
    let result = resolve_binary_param(&params).unwrap();
    assert_eq!(result, vec![42]);
}

#[test]
fn resolve_buffers_decodes_data_b64() {
    let b64 = base64::engine::general_purpose::STANDARD.encode([1, 2, 3]);
    let params = serde_json::json!({
        "buffers": [
            { "name": "input", "data_b64": b64, "size": 3 },
            { "name": "output", "size": 128 },
        ]
    });
    let resolved = resolve_buffers(&params);
    let arr = resolved.as_array().unwrap();
    assert_eq!(arr.len(), 2);
    assert!(arr[0].get("data").is_some());
    assert!(arr[0].get("data_b64").is_none());
    assert_eq!(arr[0]["name"], "input");
    assert_eq!(arr[1]["name"], "output");
    assert!(arr[1].get("data").is_none());
}

#[test]
fn resolve_buffers_non_object_items_pass_through() {
    let params = serde_json::json!({ "buffers": ["raw_string", 42] });
    let resolved = resolve_buffers(&params);
    let arr = resolved.as_array().unwrap();
    assert_eq!(arr.len(), 2);
}

#[test]
fn resolve_workgroup_size_partial_dimensions() {
    let params = serde_json::json!({ "dispatch_dims": [64] });
    assert_eq!(resolve_workgroup_size(&params), [64, 1, 1]);
}

#[test]
fn resolve_workgroup_size_falls_back_to_workgroup_size() {
    let params = serde_json::json!({ "workgroup_size": [16, 8, 4] });
    assert_eq!(resolve_workgroup_size(&params), [16, 8, 4]);
}
