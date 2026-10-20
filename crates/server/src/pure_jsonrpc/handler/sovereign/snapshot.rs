// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::pure_jsonrpc::types::{JsonRpcError, JsonRpcErrorExt};
use serde_json::Value;
use tracing::info;

use super::DEFAULT_BAR0_SIZE;

/// `sovereign.snapshot` — read-only register snapshot + tier classification.
///
/// Captures a [`SovereignSnapshot`] and [`TierEvidence`] without performing
/// any mutating BAR0 writes. Suitable for baseline captures before experiments
/// and cross-GPU comparison.
///
/// Params:
/// - `bdf` (required): PCI BDF address (e.g. `"0000:02:00.0"`)
pub fn sovereign_snapshot(params: Option<&Value>) -> Result<Value, JsonRpcError> {
    let bdf = params
        .and_then(|p| p.get("bdf"))
        .and_then(Value::as_str)
        .ok_or_else(|| JsonRpcError::invalid_params("Missing 'bdf' string parameter"))?;

    info!(bdf, "sovereign.snapshot: capturing read-only snapshot");

    let bar0 = toadstool_cylinder::vfio::device::MappedBar::from_sysfs_rw(bdf, DEFAULT_BAR0_SIZE)
        .map_err(|e| {
        JsonRpcError::internal_error(format!("BAR0 open failed for {bdf}: {e}"))
    })?;

    let (snapshot, tier) =
        toadstool_cylinder::vfio::sovereign_stages::sovereign_snapshot_only(&bar0);

    info!(
        bdf,
        tier = ?tier.tier,
        pmc_popcount = tier.pmc_popcount,
        "sovereign.snapshot: captured"
    );

    Ok(serde_json::json!({
        "bdf": bdf,
        "snapshot": snapshot,
        "tier": {
            "tier": tier.tier,
            "tier_level": tier.tier.level(),
            "tier_description": tier.tier.description(),
            "evidence": {
                "pmc_enable": format!("{:#010x}", tier.pmc_enable),
                "pmc_popcount": tier.pmc_popcount,
                "pramin_accessible": tier.pramin_accessible,
                "fecs_pc": tier.fecs_pc.map(|v| format!("{:#010x}", v)),
                "gpc_enables": tier.gpc_enables.map(|v| format!("{:#010x}", v)),
                "ce_status": tier.ce_status.map(|v| format!("{:#010x}", v)),
                "gr_status": tier.gr_status.map(|v| format!("{:#010x}", v)),
                "tpc_status": tier.tpc_status.map(|v| format!("{:#010x}", v)),
                "tpc_alive": tier.tpc_alive,
            }
        }
    }))
}

/// `sovereign.compare` — twin-card structured diff.
///
/// Captures [`SovereignSnapshot`] from two BDFs and returns both snapshots
/// plus a structured list of register deltas. This is the twin-study primitive
/// for cross-GPU comparison.
///
/// Params:
/// - `bdf_a` (required): First PCI BDF address
/// - `bdf_b` (required): Second PCI BDF address
pub fn sovereign_compare(params: Option<&Value>) -> Result<Value, JsonRpcError> {
    use toadstool_cylinder::vfio::sovereign_stages::{SovereignSnapshot, sovereign_snapshot_only};

    let bdf_a = params
        .and_then(|p| p.get("bdf_a"))
        .and_then(Value::as_str)
        .ok_or_else(|| JsonRpcError::invalid_params("Missing 'bdf_a' string parameter"))?;

    let bdf_b = params
        .and_then(|p| p.get("bdf_b"))
        .and_then(Value::as_str)
        .ok_or_else(|| JsonRpcError::invalid_params("Missing 'bdf_b' string parameter"))?;

    info!(bdf_a, bdf_b, "sovereign.compare: capturing twin snapshots");

    let bar0_a =
        toadstool_cylinder::vfio::device::MappedBar::from_sysfs_rw(bdf_a, DEFAULT_BAR0_SIZE)
            .map_err(|e| {
                JsonRpcError::internal_error(format!("BAR0 open failed for {bdf_a}: {e}"))
            })?;

    let bar0_b =
        toadstool_cylinder::vfio::device::MappedBar::from_sysfs_rw(bdf_b, DEFAULT_BAR0_SIZE)
            .map_err(|e| {
                JsonRpcError::internal_error(format!("BAR0 open failed for {bdf_b}: {e}"))
            })?;

    let (snap_a, tier_a) = sovereign_snapshot_only(&bar0_a);
    let (snap_b, tier_b) = sovereign_snapshot_only(&bar0_b);

    let deltas = SovereignSnapshot::diff_structured(&snap_a, &snap_b);

    info!(
        bdf_a,
        bdf_b,
        delta_count = deltas.len(),
        tier_a = ?tier_a.tier,
        tier_b = ?tier_b.tier,
        "sovereign.compare: diff complete"
    );

    Ok(serde_json::json!({
        "bdf_a": bdf_a,
        "bdf_b": bdf_b,
        "snapshot_a": snap_a,
        "snapshot_b": snap_b,
        "tier_a": {
            "tier": tier_a.tier,
            "tier_level": tier_a.tier.level(),
        },
        "tier_b": {
            "tier": tier_b.tier,
            "tier_level": tier_b.tier.level(),
        },
        "deltas": deltas,
        "delta_count": deltas.len(),
    }))
}

/// `sovereign.catalyst_diff` — domain-scoped BAR0 twin-card differential for catalyst analysis.
///
/// Captures BAR0 snapshots (known Volta domains only) from two BDFs (cold
/// baseline vs catalyst-warmed), computes the diff, and produces a minimal
/// replay sequence containing only registers the catalyst changed.
///
/// Params:
/// - `bdf_cold` (required): PCI BDF of the cold/baseline GPU
/// - `bdf_warm` (required): PCI BDF of the catalyst-warmed GPU
/// - `persist_path` (optional): Directory to write diff + replay JSONs
pub fn sovereign_catalyst_diff(params: Option<&Value>) -> Result<Value, JsonRpcError> {
    use toadstool_cylinder::nv::gr_init::{ChipFamily, InitSource};
    use toadstool_cylinder::nv::pri::VOLTA_BAR0_DOMAINS;
    use toadstool_cylinder::vfio::warm_capture::{Bar0Diff, Bar0Snapshot};

    let bdf_cold = params
        .and_then(|p| p.get("bdf_cold"))
        .and_then(Value::as_str)
        .ok_or_else(|| JsonRpcError::invalid_params("Missing 'bdf_cold' string parameter"))?;

    let bdf_warm = params
        .and_then(|p| p.get("bdf_warm"))
        .and_then(Value::as_str)
        .ok_or_else(|| JsonRpcError::invalid_params("Missing 'bdf_warm' string parameter"))?;

    let persist_path = params
        .and_then(|p| p.get("persist_path"))
        .and_then(Value::as_str);

    let domains = &VOLTA_BAR0_DOMAINS;
    info!(
        bdf_cold,
        bdf_warm,
        num_domains = domains.len(),
        "sovereign.catalyst_diff: capturing domain-scoped BAR0 snapshots"
    );

    let bar0_cold =
        toadstool_cylinder::vfio::device::MappedBar::from_sysfs_rw(bdf_cold, DEFAULT_BAR0_SIZE)
            .map_err(|e| {
                JsonRpcError::internal_error(format!("BAR0 open failed for cold {bdf_cold}: {e}"))
            })?;

    let bar0_warm =
        toadstool_cylinder::vfio::device::MappedBar::from_sysfs_rw(bdf_warm, DEFAULT_BAR0_SIZE)
            .map_err(|e| {
                JsonRpcError::internal_error(format!("BAR0 open failed for warm {bdf_warm}: {e}"))
            })?;

    let snap_cold = Bar0Snapshot::capture_domains(&bar0_cold, bdf_cold, "cold-baseline", domains);
    let snap_warm = Bar0Snapshot::capture_domains(&bar0_warm, bdf_warm, "catalyst-warm", domains);

    let diff = Bar0Diff::from_snapshots(&snap_cold, &snap_warm);

    let replay = diff.to_replay_sequence(
        ChipFamily::Volta,
        InitSource::Catalyst {
            driver_version: "470.256.02".into(),
            bdf: bdf_warm.to_string(),
        },
        VOLTA_BAR0_DOMAINS,
    );

    info!(
        bdf_cold,
        bdf_warm,
        changed = diff.changed_count(),
        replay_writes = replay.len(),
        domains = replay.domains().len(),
        "sovereign.catalyst_diff: diff complete"
    );

    // Persist artifacts if requested
    let mut persisted = serde_json::json!({});
    if let Some(dir) = persist_path {
        if let Err(e) = std::fs::create_dir_all(dir) {
            info!(err = %e, dir, "catalyst_diff: could not create persist dir");
        } else {
            let diff_path = format!("{dir}/gv100_catalyst_delta.json");
            if let Ok(json) = diff.to_json() {
                let _ = std::fs::write(&diff_path, &json);
                persisted["delta_path"] = serde_json::json!(diff_path);
            }

            let replay_path = format!("{dir}/gv100_catalyst_replay.json");
            if let Ok(json) = replay.to_json() {
                let _ = std::fs::write(&replay_path, &json);
                persisted["replay_path"] = serde_json::json!(replay_path);
            }

            let cold_path = format!("{dir}/gv100_cold_bar0.json");
            if let Ok(json) = snap_cold.to_json() {
                let _ = std::fs::write(&cold_path, &json);
                persisted["cold_snapshot_path"] = serde_json::json!(cold_path);
            }

            let warm_path = format!("{dir}/gv100_catalyst_bar0.json");
            if let Ok(json) = snap_warm.to_json() {
                let _ = std::fs::write(&warm_path, &json);
                persisted["warm_snapshot_path"] = serde_json::json!(warm_path);
            }
        }
    }

    let domain_summary: Vec<_> = replay
        .domain_summary()
        .into_iter()
        .map(|(d, c)| serde_json::json!({"domain": d, "writes": c}))
        .collect();

    Ok(serde_json::json!({
        "bdf_cold": bdf_cold,
        "bdf_warm": bdf_warm,
        "cold_alive_count": snap_cold.alive_count(),
        "warm_alive_count": snap_warm.alive_count(),
        "diff": {
            "changed_count": diff.changed_count(),
            "unchanged_count": diff.unchanged_count,
            "total_compared": diff.total_compared,
        },
        "replay": {
            "writes": replay.len(),
            "domains": domain_summary,
            "description": replay.description,
        },
        "persisted": persisted,
    }))
}
