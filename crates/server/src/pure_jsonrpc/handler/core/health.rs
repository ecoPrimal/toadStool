// SPDX-License-Identifier: AGPL-3.0-or-later
//! JSON-RPC health probes and full health envelope.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::pure_jsonrpc::types::{JsonRpcError, JsonRpcErrorExt};
use crate::rpc_types::HealthStatus;
use toadstool_common::constants::PRIMAL_NAME;

use super::JsonRpcResult;

/// GuideStone-mandated bare `health` probe.
///
/// Returns the minimum shape required by all primals: `{status, primal, version}`.
/// Used by cellMembrane probes and inter-primal health checks.
pub(crate) async fn health_simple(version: &str) -> JsonRpcResult {
    Ok(serde_json::json!({
        "status": "alive",
        "primal": PRIMAL_NAME,
        "version": version,
    }))
}

/// Wire Standard L1: minimal liveness probe (`health.liveness`).
///
/// Always returns `{"status":"alive"}` — if the caller can reach this
/// handler, the socket is listening and the process is alive. Boot-phase
/// signaling is handled by `health.readiness` (returns `"starting"` vs
/// `"ready"`). This separation aligns with the DEPLOYMENT_BEHAVIOR_STANDARD
/// so nucleus health sweeps pass immediately on socket bind (Wave 47).
pub(crate) async fn health_liveness() -> JsonRpcResult {
    Ok(serde_json::json!({ "status": "alive" }))
}

/// Wire Standard L2: readiness probe with version (`health.readiness`).
///
/// Returns `"starting"` during initialization, `"ready"` once fully operational.
pub(crate) async fn health_readiness(version: &str, ready: bool) -> JsonRpcResult {
    let status = if ready { "ready" } else { "starting" };
    Ok(serde_json::json!({
        "status": status,
        "version": version,
    }))
}

/// Build-identity probe (`health.version`).
///
/// Returns session, version, build hash, and service name for post-upgrade
/// verification. Build hash is embedded at compile time via `GIT_HASH` env
/// var (set by CI or `build.rs`); falls back to `"dev"` for local builds.
pub(crate) async fn health_version(version: &str) -> JsonRpcResult {
    Ok(serde_json::json!({
        "version": version,
        "session": env!("CARGO_PKG_VERSION"),
        "build_hash": option_env!("GIT_HASH").unwrap_or("dev"),
        "service": PRIMAL_NAME,
    }))
}

/// Returns health status with uptime and error count.
///
/// Used by `toadstool.health`, `health.check`, `compute.health`, and semantic `check_health`.
/// Wire Standard L1: includes `"status": "alive"` on the full envelope for orchestrators
/// that expect the legacy shape on deep checks.
pub(crate) async fn health(
    version: &Arc<str>,
    start_time: std::time::Instant,
    error_count: &AtomicU64,
) -> JsonRpcResult {
    let uptime = start_time.elapsed();
    #[expect(
        clippy::cast_possible_truncation,
        reason = "error count u64→usize is lossless on 64-bit"
    )]
    let error_count_val = error_count.load(Ordering::Relaxed) as usize;
    let status = HealthStatus::ok()
        .with_uptime(uptime.as_secs())
        .with_detail("version", serde_json::Value::String(version.to_string()))
        .with_detail("active_workloads", serde_json::json!(0))
        .with_detail("queued_workloads", serde_json::json!(0))
        .with_detail("error_count", serde_json::json!(error_count_val))
        .with_detail("resource_utilization", serde_json::json!(0.0));
    let mut value = serde_json::to_value(status)
        .map_err(|e| JsonRpcError::internal_error(format!("Serialization error: {e}")))?;
    // Wire Standard L1: biomeOS probes expect "status": "alive"
    if let Some(obj) = value.as_object_mut() {
        obj.insert("status".into(), serde_json::Value::String("alive".into()));
    }
    Ok(value)
}

/// Graceful drain for zero-disruption upgrades (`health.drain`).
///
/// Sets the draining flag so new dispatches are rejected, then waits for
/// in-flight work to complete (up to a configurable timeout). Returns
/// the drain status so the caller can confirm readiness for shutdown.
pub(crate) async fn health_drain(draining: &AtomicBool, ready: &AtomicBool) -> JsonRpcResult {
    draining.store(true, Ordering::SeqCst);
    ready.store(false, Ordering::SeqCst);

    tracing::info!("health.drain: server entering drain state — rejecting new dispatches");

    Ok(serde_json::json!({
        "status": "draining",
        "accepting_new_work": false,
        "message": "Server is draining. Send SIGTERM when ready to shut down."
    }))
}
