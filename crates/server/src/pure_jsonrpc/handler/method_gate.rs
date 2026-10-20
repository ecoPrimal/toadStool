// SPDX-License-Identifier: AGPL-3.0-or-later
//! Pre-dispatch capability gate (JH-0 / JH-2 ecosystem standard)
//!
//! Classifies every JSON-RPC method as [`MethodVisibility::Public`] or
//! [`MethodVisibility::Protected`] and gates dispatch based on the current
//! [`GateMode`]. Ships in [`GateMode::Permissive`] (all calls allowed)
//! per the primalSpring `METHOD_GATE_STANDARD.md` adoption guide.
//!
//! JH-2 adds [`ResourceEnvelope`] enforcement: ionic tokens carry resource
//! limits (`mem_mb`, `cpu_cores`, `method_allowlist`) that are checked at
//! dispatch time. When no token is present and the gate is permissive,
//! dispatch proceeds without limits (backward compatible).

use serde::{Deserialize, Serialize};
use tracing::trace;

use crate::pure_jsonrpc::types::{JsonRpcError, JsonRpcErrorExt};

/// Whether a method is freely callable or requires authorization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MethodVisibility {
    /// Callable by any peer without credentials.
    Public,
    /// Requires a valid token / caller identity when the gate is enforcing.
    Protected,
}

/// Gate operating mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GateMode {
    /// All calls allowed regardless of caller identity (JH-0 default).
    Permissive,
    /// Protected methods require valid authentication (JH-2 future).
    Enforcing,
}

/// Resource limits carried in an ionic token (JH-2).
///
/// When present, dispatch handlers enforce that the requested resources
/// fall within these bounds. Fields are optional — `None` means
/// "unlimited for this dimension".
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceEnvelope {
    /// Maximum memory in MB the token grants for a single dispatch.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mem_mb: Option<u64>,
    /// Maximum CPU cores the token grants.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_cores: Option<u32>,
    /// Maximum timeout in milliseconds the token permits per dispatch.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_timeout_ms: Option<u64>,
    /// Methods this token is allowed to call. Empty means "all methods allowed".
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub method_allowlist: Vec<String>,
}

impl ResourceEnvelope {
    /// Check whether the envelope allows calling `method`.
    ///
    /// An empty allowlist means "all methods permitted".
    pub fn allows_method(&self, method: &str) -> bool {
        self.method_allowlist.is_empty() || self.method_allowlist.iter().any(|m| m == method)
    }
}

/// Trust level established for a dispatch request (Dark Forest Invariant 3).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DispatchTrustLevel {
    #[default]
    Anonymous,
    LocalTransport,
    BtspVerified,
    MutuallyAuthenticated,
}

impl DispatchTrustLevel {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Anonymous => "anonymous",
            Self::LocalTransport => "local_transport",
            Self::BtspVerified => "btsp_verified",
            Self::MutuallyAuthenticated => "mutually_authenticated",
        }
    }
}

/// Per-connection transport hints for provenance extraction.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ConnectionTrustHints {
    pub transport: ConnectionTransport,
    pub btsp_verified: bool,
    /// Completed mutual BTSP handshake (crypto provider JH-1).
    pub mutually_authenticated: bool,
}

/// Transport kind for the active JSON-RPC connection.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ConnectionTransport {
    #[default]
    Unknown,
    Unix,
    Tcp,
}

impl ConnectionTrustHints {
    pub const UNIX_LOCAL: Self = Self {
        transport: ConnectionTransport::Unix,
        btsp_verified: false,
        mutually_authenticated: false,
    };
    pub const UNIX_BTSP: Self = Self {
        transport: ConnectionTransport::Unix,
        btsp_verified: true,
        mutually_authenticated: false,
    };
    pub const UNIX_MUTUAL_BTSP: Self = Self {
        transport: ConnectionTransport::Unix,
        btsp_verified: true,
        mutually_authenticated: true,
    };
    pub const TCP: Self = Self {
        transport: ConnectionTransport::Tcp,
        btsp_verified: false,
        mutually_authenticated: false,
    };
}

/// Caller identity and resource context extracted from a request (JH-2).
///
/// Threaded through the dispatch path so that handlers can enforce
/// per-caller resource limits. `None` fields mean "no token / unknown".
#[derive(Debug, Clone, Default)]
pub struct CallerContext {
    /// Caller identity (e.g. DID from ionic token). `None` = anonymous.
    pub identity: Option<String>,
    /// Resource envelope from the ionic token. `None` = no token presented.
    pub envelope: Option<ResourceEnvelope>,
    /// Gate identity of the requesting node (from BTSP session or gate.update).
    pub gate_id: Option<String>,
    /// Trust level established during BTSP handshake.
    pub trust_level: DispatchTrustLevel,
}

impl CallerContext {
    /// Anonymous caller with no token (permissive-mode default).
    pub fn anonymous() -> Self {
        Self {
            gate_id: None,
            trust_level: DispatchTrustLevel::Anonymous,
            ..Self::default()
        }
    }

    /// Whether this caller presented a token with an envelope.
    pub fn has_envelope(&self) -> bool {
        self.envelope.is_some()
    }
}

/// Pre-dispatch capability gate.
///
/// Sits between request parsing and method routing. In `Permissive` mode
/// every call passes through. In `Enforcing` mode, protected methods
/// require valid credentials and the caller's [`ResourceEnvelope`]
/// constraints are checked.
pub struct MethodGate {
    mode: GateMode,
}

impl MethodGate {
    /// Create a new gate in the given mode.
    pub fn new(mode: GateMode) -> Self {
        Self { mode }
    }

    /// Create a gate in permissive mode (JH-0 default).
    pub fn permissive() -> Self {
        Self::new(GateMode::Permissive)
    }

    /// Current operating mode.
    pub fn mode(&self) -> GateMode {
        self.mode
    }

    /// Check whether a method call should be allowed (JH-0 basic check).
    ///
    /// In `Permissive` mode this always returns `Ok(())`.
    /// In `Enforcing` mode, `Protected` methods are rejected with
    /// `PERMISSION_DENIED` (-32001, ecosystem standard).
    pub fn check(&self, method: &str) -> Result<(), JsonRpcError> {
        self.check_with_context(method, &CallerContext::anonymous())
    }

    /// Check method access with full caller context (JH-2).
    ///
    /// In `Enforcing` mode:
    /// - Anonymous callers are rejected on `Protected` methods with `UNAUTHORIZED`.
    /// - Callers with a token are checked against their envelope's `method_allowlist`.
    ///
    /// In `Permissive` mode: always allowed unless the token itself restricts the method.
    pub fn check_with_context(
        &self,
        method: &str,
        ctx: &CallerContext,
    ) -> Result<(), JsonRpcError> {
        let visibility = classify_method(method);

        trace!(
            method,
            ?visibility,
            mode = ?self.mode,
            has_identity = ctx.identity.is_some(),
            has_envelope = ctx.has_envelope(),
            "method gate check"
        );

        match self.mode {
            GateMode::Permissive => {
                if let Some(ref env) = ctx.envelope
                    && !env.allows_method(method)
                {
                    return Err(JsonRpcError::permission_denied(method));
                }
                Ok(())
            }
            GateMode::Enforcing => match visibility {
                MethodVisibility::Public => Ok(()),
                MethodVisibility::Protected => {
                    if ctx.identity.is_none() {
                        return Err(JsonRpcError::unauthorized(
                            "Authentication required for protected method",
                        ));
                    }
                    if let Some(ref env) = ctx.envelope
                        && !env.allows_method(method)
                    {
                        return Err(JsonRpcError::permission_denied(method));
                    }
                    Ok(())
                }
            },
        }
    }
}

/// Classify a method name into its visibility tier.
///
/// Public methods are introspection / health / identity / auth — always
/// callable. Everything else (dispatch, workloads, hardware, transport,
/// gate routing) is protected.
pub fn classify_method(method: &str) -> MethodVisibility {
    match method {
        // Health probes — always public (PG-62 fast-path must not be gated)
        "health" | "health.liveness" | "health.readiness" | "health.check" | "health.version"
        | "health.drain" | "toadstool.health" | "compute.health" => MethodVisibility::Public,

        // Identity and capabilities — introspection is always public
        "identity.get"
        | "primal.announce"
        | "toadstool.version"
        | "compute.version"
        | "capabilities.list"
        | "capability.list"
        | "primal.capabilities"
        | "compute.capabilities"
        | "compute.discover_capabilities"
        | "toadstool.query_capabilities" => MethodVisibility::Public,

        // Provenance — read-only introspection
        "provenance.query" | "provenance.get" | "toadstool.provenance" => MethodVisibility::Public,

        // Dispatch telemetry schema — public introspection for ml.mlp_train
        "dispatch.telemetry.schema" => MethodVisibility::Public,

        // Auth methods — must be public so callers can check their own status
        m if m.starts_with("auth.") => MethodVisibility::Public,

        // Everything else is protected
        _ => MethodVisibility::Protected,
    }
}

#[cfg(test)]
#[path = "method_gate_tests.rs"]
mod tests;
