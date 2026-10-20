// SPDX-License-Identifier: AGPL-3.0-or-later
//! `sovereign.runlist_diagnostic` — live PFIFO state inspection for PBDMA
//! runlist debugging.

use super::DispatchHandler;
use crate::pure_jsonrpc::types::JsonRpcErrorExt;

/// Reports full PFIFO register state for PBDMA runlist debugging.
///
/// Returns RUNLIST_BASE, SCHED_EN/DISABLE, PBDMA channel binding, GP_GET/PUT,
/// PCCSR status, and PTOP GR runlist discovery. Use to confirm whether
/// PRI enumerate succeeded and runlist writes are sticking.
pub(crate) async fn sovereign_runlist_diagnostic(
    handler: &DispatchHandler,
    params: Option<&serde_json::Value>,
) -> Result<serde_json::Value, crate::pure_jsonrpc::types::JsonRpcError> {
    use toadstool_cylinder::vfio::device::MappedBar;

    let bdf_from_params = params
        .and_then(|p| p.get("bdf"))
        .and_then(serde_json::Value::as_str)
        .map(String::from);

    let bdf = if let Some(b) = bdf_from_params {
        b
    } else {
        // Fall back to first anchored or cached device
        let anchors = handler.anchor_store.lock().await;
        if let Some(first) = anchors.keys().next() {
            first.clone()
        } else {
            drop(anchors);
            let cache = handler.cached_devices.lock().await;
            cache.keys().next().cloned().unwrap_or_default()
        }
    };

    if bdf.is_empty() {
        return Err(crate::pure_jsonrpc::types::JsonRpcError::invalid_params(
            "no BDF specified and no GPUs discovered",
        ));
    }

    let bar0 = MappedBar::from_sysfs_rw(&bdf, 16 * 1024 * 1024).map_err(|e| {
        crate::pure_jsonrpc::types::JsonRpcError::internal_error(format!(
            "BAR0 open failed for {bdf}: {e}"
        ))
    })?;

    let pfifo_enable = bar0.read_u32(0x2200).unwrap_or(0xDEAD);
    let sched_en = bar0.read_u32(0x2504).unwrap_or(0xDEAD);
    let sched_disable = bar0.read_u32(0x2630).unwrap_or(0xDEAD);
    let pfifo_intr = bar0.read_u32(0x2100).unwrap_or(0xDEAD);
    let pbdma_map = bar0.read_u32(0x2004).unwrap_or(0);

    // Read per-runlist RUNLIST_BASE registers (0-3)
    let mut runlist_bases = serde_json::Map::new();
    for rl in 0..4_u32 {
        let base_reg = 0x2270 + (rl as usize) * 0x10;
        let base_val = bar0.read_u32(base_reg).unwrap_or(0xDEAD);
        runlist_bases.insert(
            format!("rl{rl}"),
            serde_json::json!({
                "register": format!("0x{base_reg:04x}"),
                "value": format!("0x{base_val:08x}"),
                "configured": base_val != 0 && base_val != 0xDEAD_DEAD,
            }),
        );
    }

    // PTOP discovery — find GR engine runlist
    let ptop_base: usize = 0x0002_2700;
    let mut gr_runlist: Option<u32> = None;
    let mut engine_table = Vec::new();
    let mut cur_type: u32 = 0xFFFF;
    let mut cur_runlist: u32 = 0xFFFF;
    for i in 0..64_u32 {
        let data = bar0.read_u32(ptop_base + (i as usize) * 4).unwrap_or(0);
        if data == 0 {
            break;
        }
        let kind = data & 3;
        match kind {
            1 => cur_type = (data >> 2) & 0x3F,
            2 => cur_runlist = (data >> 14) & 0xF,
            _ => {}
        }
        if data & (1 << 31) != 0 {
            let engine_name = match cur_type {
                0 => "GR",
                1 => "CE",
                2 => "NVDEC",
                3 => "SEC2",
                8 => "MSENC",
                _ => "unknown",
            };
            engine_table.push(serde_json::json!({
                "type": cur_type,
                "name": engine_name,
                "runlist": cur_runlist,
            }));
            if cur_type == 0 && gr_runlist.is_none() && cur_runlist != 0xFFFF {
                gr_runlist = Some(cur_runlist);
            }
            cur_type = 0xFFFF;
            cur_runlist = 0xFFFF;
        }
    }

    // PBDMA state for each present PBDMA
    let mut pbdma_state = Vec::new();
    for pid in 0..32_usize {
        if pbdma_map & (1 << pid) == 0 {
            continue;
        }
        let pb = 0x0004_0000 + pid * 0x2000;
        let gp_get = bar0.read_u32(pb + 0x088).unwrap_or(0xDEAD);
        let gp_put = bar0.read_u32(pb + 0x08C).unwrap_or(0xDEAD);
        let gp_state = bar0.read_u32(pb + 0x098).unwrap_or(0xDEAD);
        let channel_info = bar0.read_u32(pb + 0x0B0).unwrap_or(0xDEAD);
        let intr_0 = bar0.read_u32(pb + 0x100).unwrap_or(0xDEAD);

        pbdma_state.push(serde_json::json!({
            "id": pid,
            "gp_get": format!("0x{gp_get:08x}"),
            "gp_put": format!("0x{gp_put:08x}"),
            "gp_state": format!("0x{gp_state:08x}"),
            "channel_info": format!("0x{channel_info:08x}"),
            "intr_0": format!("0x{intr_0:08x}"),
            "gp_advancing": gp_get != 0 || gp_put != 0,
        }));
    }

    // Channel PCCSR status (check first 4 channels)
    let mut channel_status = Vec::new();
    for ch_id in 0..4_u32 {
        let pccsr_val = bar0.read_u32(0x800000 + (ch_id as usize) * 8).unwrap_or(0);
        if pccsr_val == 0 {
            continue;
        }
        let status = (pccsr_val >> 24) & 0xF;
        let status_name = match status {
            0 => "IDLE",
            1 => "PENDING",
            2 => "PENDING_CTX_RELOAD",
            3 => "PENDING_ACQ",
            5 => "ON_PBDMA",
            6 => "ON_PBDMA_AND_ENG",
            7 => "ON_ENG",
            8 => "ON_ENG_PENDING_CTX_RELOAD",
            _ => "unknown",
        };
        channel_status.push(serde_json::json!({
            "channel_id": ch_id,
            "pccsr": format!("0x{pccsr_val:08x}"),
            "status": status,
            "status_name": status_name,
            "enabled": pccsr_val & 1 != 0,
        }));
    }

    // PRI ring master status
    let pri_status = bar0.read_u32(0x12_0058).unwrap_or(0xDEAD);

    Ok(serde_json::json!({
        "bdf": bdf,
        "pfifo": {
            "enable": format!("0x{pfifo_enable:08x}"),
            "sched_en": format!("0x{sched_en:08x}"),
            "sched_disable": format!("0x{sched_disable:08x}"),
            "intr": format!("0x{pfifo_intr:08x}"),
            "pbdma_map": format!("0x{pbdma_map:08x}"),
        },
        "runlist_bases": runlist_bases,
        "ptop": {
            "gr_runlist": gr_runlist,
            "engines": engine_table,
        },
        "pbdma": pbdma_state,
        "channels": channel_status,
        "pri_ring": {
            "master_status": format!("0x{pri_status:08x}"),
            "healthy": pri_status == 0,
        },
        "diagnosis": {
            "runlist_configured": gr_runlist
                .is_some_and(|rl| {
                    let base_reg = 0x2270 + (rl as usize) * 0x10;
                    let val = bar0.read_u32(base_reg).unwrap_or(0);
                    val != 0 && val != 0xDEAD_DEAD
                }),
            "pri_ring_healthy": pri_status == 0,
            "scheduler_active": sched_disable == 0 || sched_en == 1,
        },
    }))
}
