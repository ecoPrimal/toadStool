// SPDX-License-Identifier: AGPL-3.0-or-later
//! Auth introspection methods (JH-0 ecosystem standard)
//!
//! Three public methods for callers to inspect the gate state:
//! - `auth.check` — would the caller be allowed to call a given method?
//! - `auth.mode` — what gate mode is active?
//! - `auth.peer_info` — what does the server know about the caller?

#[cfg(test)]
use super::method_gate::GateMode;
use super::method_gate::{DispatchTrustLevel, MethodGate, classify_method};
use crate::pure_jsonrpc::types::{JsonRpcError};

/// `auth.check` — test whether a method would be allowed for the current caller.
///
/// Params: `{"method": "compute.dispatch.submit"}`
/// Returns: `{"allowed": true, "visibility": "protected", "mode": "permissive"}`
#[expect(
    clippy::unnecessary_wraps,
    reason = "Result return required for JSON-RPC handler dispatch consistency"
)]
pub fn auth_check(
    gate: &MethodGate,
    params: Option<&serde_json::Value>,
) -> Result<serde_json::Value, JsonRpcError> {
    let method = params
        .and_then(|p| p.get("method"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");

    let visibility = classify_method(method);
    let allowed = gate.check(method).is_ok();

    Ok(serde_json::json!({
        "allowed": allowed,
        "visibility": visibility,
        "mode": gate.mode(),
        "method": method,
    }))
}

/// `auth.mode` — return the current gate operating mode.
///
/// Returns: `{"mode": "permissive"}` or `{"mode": "enforcing"}`
#[expect(
    clippy::unnecessary_wraps,
    reason = "Result return required for JSON-RPC handler dispatch consistency"
)]
pub fn auth_mode(gate: &MethodGate) -> Result<serde_json::Value, JsonRpcError> {
    Ok(serde_json::json!({
        "mode": gate.mode(),
    }))
}

/// `auth.peer_info` — return what the server knows about the caller.
///
/// Returns caller identity and resource envelope when an ionic token
/// is present (JH-2). Without a token, returns anonymous defaults.
///
/// Returns: `{"transport": "unknown", "authenticated": false, ...}`
fn transport_from_trust(trust_level: DispatchTrustLevel) -> &'static str {
    match trust_level {
        DispatchTrustLevel::BtspVerified => "btsp",
        DispatchTrustLevel::LocalTransport => "unix",
        DispatchTrustLevel::MutuallyAuthenticated => "mutual_btsp",
        DispatchTrustLevel::Anonymous => "unknown",
    }
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "Result return required for JSON-RPC handler dispatch consistency"
)]
pub fn auth_peer_info(
    ctx: &super::method_gate::CallerContext,
) -> Result<serde_json::Value, JsonRpcError> {
    Ok(serde_json::json!({
        "transport": transport_from_trust(ctx.trust_level),
        "authenticated": ctx.identity.is_some(),
        "identity": ctx.identity,
        "envelope": ctx.envelope,
        "gate_id": ctx.gate_id,
        "trust_level": ctx.trust_level,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_check_permissive_allows_protected() {
        let gate = MethodGate::permissive();
        let params = serde_json::json!({"method": "compute.dispatch.submit"});
        let result = auth_check(&gate, Some(&params)).unwrap();
        assert_eq!(result["allowed"], true);
        assert_eq!(result["visibility"], "protected");
        assert_eq!(result["mode"], "permissive");
    }

    #[test]
    fn auth_check_enforcing_denies_protected() {
        let gate = MethodGate::new(GateMode::Enforcing);
        let params = serde_json::json!({"method": "compute.dispatch.submit"});
        let result = auth_check(&gate, Some(&params)).unwrap();
        assert_eq!(result["allowed"], false);
        assert_eq!(result["visibility"], "protected");
        assert_eq!(result["mode"], "enforcing");
    }

    #[test]
    fn auth_check_enforcing_allows_public() {
        let gate = MethodGate::new(GateMode::Enforcing);
        let params = serde_json::json!({"method": "health.liveness"});
        let result = auth_check(&gate, Some(&params)).unwrap();
        assert_eq!(result["allowed"], true);
        assert_eq!(result["visibility"], "public");
    }

    #[test]
    fn auth_check_no_method_param() {
        let gate = MethodGate::permissive();
        let result = auth_check(&gate, None).unwrap();
        assert_eq!(result["allowed"], true);
        assert_eq!(result["method"], "");
    }

    #[test]
    fn auth_mode_returns_current() {
        let gate = MethodGate::permissive();
        let result = auth_mode(&gate).unwrap();
        assert_eq!(result["mode"], "permissive");

        let gate = MethodGate::new(GateMode::Enforcing);
        let result = auth_mode(&gate).unwrap();
        assert_eq!(result["mode"], "enforcing");
    }

    #[test]
    fn auth_peer_info_anonymous() {
        let ctx = super::super::method_gate::CallerContext::anonymous();
        let result = auth_peer_info(&ctx).unwrap();
        assert_eq!(result["transport"], "unknown");
        assert_eq!(result["authenticated"], false);
        assert!(result["identity"].is_null());
        assert!(result["envelope"].is_null());
        assert!(result["gate_id"].is_null());
        assert_eq!(result["trust_level"], "anonymous");
    }

    #[test]
    fn auth_peer_info_with_identity() {
        let ctx = super::super::method_gate::CallerContext {
            identity: Some("did:key:z6Mk_test".into()),
            envelope: Some(super::super::method_gate::ResourceEnvelope {
                mem_mb: Some(8192),
                cpu_cores: Some(4),
                max_timeout_ms: Some(30_000),
                method_allowlist: vec!["compute.dispatch.submit".into()],
            }),
            ..super::super::method_gate::CallerContext::anonymous()
        };
        let result = auth_peer_info(&ctx).unwrap();
        assert_eq!(result["transport"], "unknown");
        assert_eq!(result["authenticated"], true);
        assert_eq!(result["identity"], "did:key:z6Mk_test");
        assert_eq!(result["envelope"]["mem_mb"], 8192);
        assert_eq!(result["envelope"]["cpu_cores"], 4);
        assert!(result["gate_id"].is_null());
        assert_eq!(result["trust_level"], "anonymous");
    }

    #[test]
    fn auth_peer_info_local_transport_trust() {
        let ctx = super::super::method_gate::CallerContext {
            trust_level: DispatchTrustLevel::LocalTransport,
            ..super::super::method_gate::CallerContext::anonymous()
        };
        let result = auth_peer_info(&ctx).unwrap();
        assert_eq!(result["transport"], "unix");
        assert_eq!(result["trust_level"], "local_transport");
    }

    #[test]
    fn auth_peer_info_btsp_verified_trust() {
        let ctx = super::super::method_gate::CallerContext {
            trust_level: DispatchTrustLevel::BtspVerified,
            identity: Some("did:key:z6Mk_btsp".into()),
            ..super::super::method_gate::CallerContext::anonymous()
        };
        let result = auth_peer_info(&ctx).unwrap();
        assert_eq!(result["transport"], "btsp");
        assert_eq!(result["authenticated"], true);
        assert_eq!(result["trust_level"], "btsp_verified");
    }

    #[test]
    fn auth_peer_info_mutual_btsp_trust() {
        let ctx = super::super::method_gate::CallerContext {
            trust_level: DispatchTrustLevel::MutuallyAuthenticated,
            gate_id: Some("gate-42".into()),
            identity: Some("did:key:z6Mk_mutual".into()),
            ..super::super::method_gate::CallerContext::anonymous()
        };
        let result = auth_peer_info(&ctx).unwrap();
        assert_eq!(result["transport"], "mutual_btsp");
        assert_eq!(result["gate_id"], "gate-42");
        assert_eq!(result["trust_level"], "mutually_authenticated");
    }

    #[test]
    fn auth_check_unknown_method_classified_protected() {
        let gate = MethodGate::permissive();
        let params = serde_json::json!({"method": "vendor.custom.method"});
        let result = auth_check(&gate, Some(&params)).unwrap();
        assert_eq!(result["visibility"], "protected");
        assert_eq!(result["allowed"], true);
    }

    #[test]
    fn auth_check_public_method_in_enforcing_mode() {
        let gate = MethodGate::new(GateMode::Enforcing);
        let params = serde_json::json!({"method": "auth.mode"});
        let result = auth_check(&gate, Some(&params)).unwrap();
        assert_eq!(result["visibility"], "public");
        assert_eq!(result["allowed"], true);
        assert_eq!(result["method"], "auth.mode");
    }

    #[test]
    fn auth_peer_info_envelope_empty_allowlist_omitted_in_json() {
        let ctx = super::super::method_gate::CallerContext {
            envelope: Some(super::super::method_gate::ResourceEnvelope::default()),
            ..super::super::method_gate::CallerContext::anonymous()
        };
        let result = auth_peer_info(&ctx).unwrap();
        let envelope = result["envelope"].as_object().expect("envelope object");
        assert!(!envelope.contains_key("method_allowlist"));
    }
}
