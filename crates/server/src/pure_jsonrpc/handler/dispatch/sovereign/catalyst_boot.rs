// SPDX-License-Identifier: AGPL-3.0-or-later

use super::DispatchHandler;
use super::init;

const CATALYST_BOOT_WATCHDOG_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);
const CATALYST_BOOT_RPC_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(90);

/// Catalyst-free boot: nouveau warm handoff + golden state replay + tier classification.
///
/// The end-state pipeline: no proprietary driver at runtime. Uses nouveau
/// for HBM2 training and basic engine init, then replays the catalyst's
/// golden state to bring TPC PRI stations alive.
pub(crate) async fn sovereign_catalyst_boot(
    handler: &DispatchHandler,
    params: Option<&serde_json::Value>,
) -> Result<serde_json::Value, crate::pure_jsonrpc::types::JsonRpcError> {
    use crate::pure_jsonrpc::types::{JsonRpcError, JsonRpcErrorExt};
    use toadstool_cylinder::vfio::sovereign_handoff::HandoffConfig;

    let bdf = params
        .and_then(|p| p.get("bdf"))
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| JsonRpcError::invalid_params("Missing 'bdf' string parameter"))?;

    let engine_init_path = params
        .and_then(|p| p.get("engine_init_path"))
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            JsonRpcError::invalid_params(
                "Missing 'engine_init_path' — path to catalyst replay JSON",
            )
        })?;

    // Validate the replay file exists and parses before starting handoff
    let replay_json = std::fs::read_to_string(engine_init_path).map_err(|e| {
        JsonRpcError::invalid_params(format!(
            "Cannot read engine_init_path '{engine_init_path}': {e}"
        ))
    })?;
    let replay_seq = toadstool_cylinder::nv::gr_init::GrInitSequence::from_json(&replay_json)
        .map_err(|e| {
            JsonRpcError::invalid_params(format!(
                "Invalid GrInitSequence JSON in '{engine_init_path}': {e}"
            ))
        })?;

    tracing::info!(
        bdf,
        engine_init_path,
        replay_writes = replay_seq.len(),
        "sovereign.catalyst_boot: starting catalyst-free boot"
    );

    // Step 1: Nouveau warm handoff
    let mut config = HandoffConfig::nouveau_titanv(bdf);
    if let Some(secs) = params
        .and_then(|p| p.get("settle_secs"))
        .and_then(serde_json::Value::as_u64)
    {
        config.settle = std::time::Duration::from_secs(secs);
    }

    // Exp 229 fix: exclude target BDF + siblings + upstream bridges from keepalive.
    let mut excluded_bdfs = vec![bdf.to_string()];
    for sib in toadstool_cylinder::vfio::guarded_sysfs::iommu_group_siblings(bdf) {
        excluded_bdfs.push(sib);
    }
    for bridge_bdf in &toadstool_ember::plx_keepalive::detect_pcie_bridges(bdf) {
        if !excluded_bdfs.contains(bridge_bdf) {
            excluded_bdfs.push(bridge_bdf.clone());
        }
    }
    let _keepalive_exclusion =
        crate::background::pcie_keepalive::HandoffExclusionGuard::new(excluded_bdfs);

    // Suppress FLR before releasing anchor (Exp 225 fix).
    // catalyst_boot always uses nouveau which doesn't need RM DEVINIT,
    // so always suppress SBR to preserve any existing warm state.
    toadstool_cylinder::vfio::guarded_sysfs::prepare_anchor_release(bdf, true);

    // Release VFIO resources — FLR already suppressed above
    {
        let mut anchors = handler.anchor_store.lock().await;
        if let Some(anchor) = anchors.remove(bdf) {
            anchor.release_prepared();
        }
    }
    {
        let mut cache = handler.cached_devices.lock().await;
        if cache.remove(bdf).is_some() {
            tracing::info!(bdf, "catalyst_boot: released cached device");
        }
    }
    {
        let closed = toadstool_cylinder::vfio::guarded_sysfs::release_bar0_fds(bdf);
        if closed > 0 {
            tracing::info!(
                bdf,
                closed,
                "catalyst_boot: released leaked BAR0 resource0 fds"
            );
        }
    }

    let bdf_owned = bdf.to_string();
    let watchdog_profile = toadstool_cylinder::nv::registers::pmc::InterruptProfile::for_sm(
        config.sm_version.unwrap_or(70),
    );
    let _watchdog_guard = crate::background::catalyst_watchdog::activate(
        &bdf_owned,
        watchdog_profile,
        Some(CATALYST_BOOT_WATCHDOG_TIMEOUT),
        &config.module_name,
    );

    let rpc_timeout = CATALYST_BOOT_RPC_TIMEOUT;
    let blocking_future = tokio::task::spawn_blocking(move || {
        toadstool_cylinder::vfio::sovereign_handoff::execute_handoff_with_heartbeat(
            &config,
            None,
            crate::background::catalyst_watchdog::heartbeat,
        )
    });

    let handoff_result = match tokio::time::timeout(rpc_timeout, blocking_future).await {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => {
            return Err(JsonRpcError::internal_error(format!(
                "handoff task panicked: {e}"
            )));
        }
        Err(_) => {
            return Err(JsonRpcError::internal_error(
                "catalyst_boot: nouveau warm handoff timed out".to_string(),
            ));
        }
    };

    if !handoff_result.success {
        tracing::warn!(
            bdf = bdf_owned.as_str(),
            halted_at = ?handoff_result.halted_at,
            "catalyst_boot: nouveau warm handoff failed"
        );
        return serde_json::to_value(serde_json::json!({
            "success": false,
            "phase": "warm_handoff",
            "handoff": handoff_result,
        }))
        .map_err(|e| JsonRpcError::internal_error(format!("serialization: {e}")));
    }

    tracing::info!(
        bdf = bdf_owned.as_str(),
        handoff_ms = handoff_result.total_ms,
        handoff_tier = ?handoff_result.tier.as_ref().map(|t| t.tier),
        "catalyst_boot: nouveau handoff complete, replaying golden state"
    );

    // Step 2: Replay golden state via sovereign.init
    let init_params = serde_json::json!({
        "bdf": bdf_owned,
        "engine_init_path": engine_init_path,
    });
    let init_result = init::sovereign_init_ember(handler, Some(&init_params)).await;

    match init_result {
        Ok(init_val) => {
            let final_tier = init_val
                .get("compute_ready")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);

            tracing::info!(
                bdf = bdf_owned.as_str(),
                compute_ready = final_tier,
                "catalyst_boot: pipeline complete"
            );

            Ok(serde_json::json!({
                "success": true,
                "bdf": bdf_owned,
                "handoff": {
                    "success": handoff_result.success,
                    "tier": handoff_result.tier,
                    "total_ms": handoff_result.total_ms,
                },
                "init": init_val,
                "catalyst_free": true,
            }))
        }
        Err(e) => Ok(serde_json::json!({
            "success": false,
            "phase": "sovereign_init",
            "handoff": {
                "success": handoff_result.success,
                "tier": handoff_result.tier,
                "total_ms": handoff_result.total_ms,
            },
            "error": format!("{e:?}"),
        })),
    }
}
