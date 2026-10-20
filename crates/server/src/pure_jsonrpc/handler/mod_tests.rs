// SPDX-License-Identifier: AGPL-3.0-or-later

use std::borrow::Cow;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use super::JsonRpcHandler;
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
async fn test_health_returns_valid_status() {
    let handler = test_handler();
    let request = mk_request("toadstool.health", None, 1);
    let response = handler.handle_request(&request).await;

    assert!(response.error.is_none());
    let result = response.result.expect("result present");
    assert!(result["healthy"].as_bool().unwrap());
    assert!(result["details"]["version"].as_str().is_some());
    assert!(result["uptime_secs"].as_u64().is_some());
    assert!(result["details"]["error_count"].as_u64().is_some());
}

#[tokio::test]
async fn test_health_triad_liveness_readiness_check() {
    let handler = test_handler();

    let live = handler
        .handle_request(&mk_request("health.liveness", None, 10))
        .await;
    assert!(live.error.is_none());
    let r = live.result.expect("liveness");
    assert_eq!(r["status"], "alive");
    assert!(r.get("healthy").is_none(), "liveness must be minimal");

    let ready = handler
        .handle_request(&mk_request("health.readiness", None, 11))
        .await;
    assert!(ready.error.is_none());
    let r = ready.result.expect("readiness");
    assert_eq!(r["status"], "ready");
    assert_eq!(r["version"], "test-1.0.0");

    let check = handler
        .handle_request(&mk_request("health.check", None, 12))
        .await;
    assert!(check.error.is_none());
    let r = check.result.expect("check");
    assert!(r["healthy"].as_bool().unwrap());
    assert_eq!(r["status"], "alive");
}

/// Wave 47: health.liveness always returns "alive" (liveness = socket is up).
/// health.readiness returns "starting" → "ready" based on ready flag (PG-62).
#[tokio::test]
async fn test_health_liveness_always_alive_readiness_tracks_boot() {
    use std::sync::atomic::Ordering;

    let ready = Arc::new(AtomicBool::new(false));
    let executor = Arc::new(crate::tarpc_server::WorkloadExecutorDispatch::Standalone(
        crate::tarpc_server::StandaloneExecutor::new(),
    ));
    let handler = JsonRpcHandler::new(
        executor,
        "test-1.0.0".to_string(),
        None,
        Arc::clone(&ready),
        None,
    );

    let live = handler
        .handle_request(&mk_request("health.liveness", None, 20))
        .await;
    assert!(live.error.is_none());
    let r = live.result.expect("liveness alive before ready");
    assert_eq!(r["status"], "alive");

    let rdns = handler
        .handle_request(&mk_request("health.readiness", None, 21))
        .await;
    assert!(rdns.error.is_none());
    let r = rdns.result.expect("readiness starting");
    assert_eq!(r["status"], "starting");

    ready.store(true, Ordering::Release);

    let live = handler
        .handle_request(&mk_request("health.liveness", None, 22))
        .await;
    assert!(live.error.is_none());
    let r = live.result.expect("liveness alive after ready");
    assert_eq!(r["status"], "alive");

    let rdns = handler
        .handle_request(&mk_request("health.readiness", None, 23))
        .await;
    assert!(rdns.error.is_none());
    let r = rdns.result.expect("readiness ready");
    assert_eq!(r["status"], "ready");
    assert_eq!(r["version"], "test-1.0.0");
}

#[tokio::test]
async fn test_version_info_returns_expected_fields() {
    let handler = test_handler();
    let request = mk_request("toadstool.version", None, 1);
    let response = handler.handle_request(&request).await;

    assert!(response.error.is_none());
    let result = response.result.expect("result present");
    assert_eq!(result["version"], "test-1.0.0");
    assert_eq!(result["protocol"], "JSON-RPC 2.0");
    assert_eq!(result["service"], "ToadStool Compute");
    assert!(result["implementation"].as_str().is_some());
}

#[tokio::test]
async fn test_handle_method_returns_method_not_found_for_unknown() {
    let handler = test_handler();
    let request = mk_request("unknown.nonexistent.method", None, 99);
    let response = handler.handle_request(&request).await;

    assert!(response.result.is_none());
    let err = response.error.expect("error present");
    assert_eq!(err.code, JsonRpcError::METHOD_NOT_FOUND);
    assert!(err.message.contains("unknown.nonexistent.method"));
}

#[tokio::test]
async fn test_discover_capabilities_includes_shader_methods() {
    let handler = test_handler();
    let request = mk_request("compute.discover_capabilities", None, 1);
    let response = handler.handle_request(&request).await;

    assert!(response.error.is_none());
    let result = response.result.expect("result present");
    let methods = result["methods"].as_array().expect("methods is array");
    let has_shader_dispatch = methods
        .iter()
        .any(|m| m.as_str() == Some("shader.dispatch"));
    assert!(
        has_shader_dispatch,
        "methods should include shader.dispatch"
    );
}

#[tokio::test]
async fn test_shader_dispatch_routes_and_returns_domain() {
    let handler = test_handler();
    let params = serde_json::json!({
        "binary": [0xDE, 0xAD, 0xBE, 0xEF],
        "bdf": "0000:03:00.0",
        "dispatch_mode": "passthrough",
    });
    let request = mk_request("shader.dispatch", Some(params), 1);
    let response = handler.handle_request(&request).await;
    assert!(
        response.error.is_none(),
        "shader.dispatch should route without error"
    );
    let result = response.result.expect("result present");
    assert_eq!(result["domain"], "compute.dispatch");
    assert_eq!(result["operation"], "shader");
    assert!(result["job_id"].as_str().is_some());
    assert_eq!(result["metadata"]["binary_size"], 4);
}

#[tokio::test]
async fn test_auth_check_returns_allowed_permissive() {
    let handler = test_handler();
    let params = serde_json::json!({"method": "compute.dispatch.submit"});
    let request = mk_request("auth.check", Some(params), 1);
    let response = handler.handle_request(&request).await;

    assert!(response.error.is_none());
    let result = response.result.expect("result");
    assert_eq!(result["allowed"], true);
    assert_eq!(result["visibility"], "protected");
    assert_eq!(result["mode"], "permissive");
    assert_eq!(result["method"], "compute.dispatch.submit");
}

#[tokio::test]
async fn test_auth_mode_returns_permissive() {
    let handler = test_handler();
    let request = mk_request("auth.mode", None, 1);
    let response = handler.handle_request(&request).await;

    assert!(response.error.is_none());
    let result = response.result.expect("result");
    assert_eq!(result["mode"], "permissive");
}

#[tokio::test]
async fn test_auth_mode_enforcing_gate() {
    use super::method_gate::{GateMode, MethodGate};

    let gate = MethodGate::new(GateMode::Enforcing);
    let result = super::auth::auth_mode(&gate).unwrap();
    assert_eq!(result["mode"], "enforcing");
}

#[tokio::test]
async fn test_enforcing_gate_denies_anonymous_protected_method() {
    use super::method_gate::{GateMode, MethodGate};

    let gate = MethodGate::new(GateMode::Enforcing);
    let err = gate.check("compute.dispatch.submit").unwrap_err();
    assert_eq!(err.code, -32000);
}

#[tokio::test]
async fn test_auth_peer_info_returns_unknown() {
    let handler = test_handler();
    let request = mk_request("auth.peer_info", None, 1);
    let response = handler.handle_request(&request).await;

    assert!(response.error.is_none());
    let result = response.result.expect("result");
    assert_eq!(result["transport"], "unknown");
    assert_eq!(result["authenticated"], false);
}

// --- extract_caller_context / resolve_local_gate_id (P0) ---

use super::method_gate::{ConnectionTrustHints, DispatchTrustLevel};
use super::{extract_caller_context, resolve_local_gate_id};
use toadstool_common::interned_strings::socket_env;

fn test_gate_id() -> Option<std::sync::Arc<str>> {
    resolve_local_gate_id().map(|s| std::sync::Arc::from(s.as_str()))
}

#[test]
fn extract_caller_context_anonymous_connection_yields_anonymous_context() {
    let ctx = extract_caller_context(ConnectionTrustHints::default(), None);
    assert_eq!(ctx.trust_level, DispatchTrustLevel::Anonymous);
    assert!(ctx.gate_id.is_none());
}

#[test]
fn extract_caller_context_btsp_verified_sets_trust_level() {
    let gid = test_gate_id();
    let ctx = extract_caller_context(ConnectionTrustHints::UNIX_BTSP, gid.as_ref());
    assert_eq!(ctx.trust_level, DispatchTrustLevel::BtspVerified);
}

#[test]
fn extract_caller_context_btsp_verified_gate_id_matches_resolve_local_gate_id() {
    let gid = test_gate_id();
    let ctx = extract_caller_context(ConnectionTrustHints::UNIX_BTSP, gid.as_ref());
    assert_eq!(ctx.gate_id, resolve_local_gate_id());
}

#[test]
fn extract_caller_context_mutually_authenticated_sets_trust_level() {
    let gid = test_gate_id();
    let ctx = extract_caller_context(ConnectionTrustHints::UNIX_MUTUAL_BTSP, gid.as_ref());
    assert_eq!(ctx.trust_level, DispatchTrustLevel::MutuallyAuthenticated);
}

#[test]
fn extract_caller_context_mutually_authenticated_gate_id_matches_resolve_local_gate_id() {
    let gid = test_gate_id();
    let ctx = extract_caller_context(ConnectionTrustHints::UNIX_MUTUAL_BTSP, gid.as_ref());
    assert_eq!(ctx.gate_id, resolve_local_gate_id());
}

#[test]
fn extract_caller_context_unix_local_sets_local_transport() {
    let gid = test_gate_id();
    let ctx = extract_caller_context(ConnectionTrustHints::UNIX_LOCAL, gid.as_ref());
    assert_eq!(ctx.trust_level, DispatchTrustLevel::LocalTransport);
}

#[test]
fn extract_caller_context_unix_local_gate_id_matches_resolve_local_gate_id() {
    let gid = test_gate_id();
    let ctx = extract_caller_context(ConnectionTrustHints::UNIX_LOCAL, gid.as_ref());
    assert_eq!(ctx.gate_id, resolve_local_gate_id());
}

#[test]
fn extract_caller_context_anonymous_tcp_has_no_gate_id() {
    let ctx = extract_caller_context(ConnectionTrustHints::TCP, None);
    assert_eq!(ctx.trust_level, DispatchTrustLevel::Anonymous);
    assert!(ctx.gate_id.is_none());
}

#[test]
fn resolve_local_gate_id_is_stable_across_calls() {
    let first = resolve_local_gate_id();
    let second = resolve_local_gate_id();
    assert_eq!(first, second);
}

#[test]
fn resolve_local_gate_id_uses_toadstool_gate_id_when_set() {
    if let Ok(expected) = std::env::var(socket_env::TOADSTOOL_GATE_ID) {
        assert_eq!(resolve_local_gate_id().as_deref(), Some(expected.as_str()));
    }
}

#[test]
fn resolve_local_gate_id_falls_back_when_toadstool_gate_id_unset() {
    if std::env::var(socket_env::TOADSTOOL_GATE_ID).is_err() {
        let id = resolve_local_gate_id();
        let from_host = std::env::var(socket_env::HOSTNAME).ok();
        let from_sys = toadstool_sysmon::system::hostname();
        assert_eq!(id.as_deref(), from_host.as_deref().or(from_sys.as_deref()));
    }
}

#[tokio::test]
async fn auth_peer_info_reflects_btsp_caller_context() {
    let handler = test_handler();
    let request = mk_request("auth.peer_info", None, 1);
    let response = handler
        .handle_request_with_connection(&request, ConnectionTrustHints::UNIX_BTSP)
        .await;

    assert!(response.error.is_none());
    let result = response.result.expect("result");
    assert_eq!(result["transport"], "btsp");
    assert_eq!(result["trust_level"], "btsp_verified");
    assert_eq!(
        result["gate_id"].as_str(),
        resolve_local_gate_id().as_deref()
    );
}
