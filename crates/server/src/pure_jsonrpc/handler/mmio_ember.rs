// SPDX-License-Identifier: AGPL-3.0-or-later
//! Ember device JSON-RPC handlers.
//!
//! `ember.fecs.state`, `ember.device.health`, `ember.device.recover`,
//! `ember.pramin.read`, `ember.pramin.write`.

use serde_json::Value;
use tracing::debug;

use super::mmio::{
    FECS_CPUCTL, FECS_MAILBOX0, FECS_MAILBOX1, FECS_OS, FECS_PC, PBUS_BAR0_WINDOW, PMC_BOOT_0,
    PMC_ENABLE, PMC_MASK_FULL, PRAMIN_BASE, PRAMIN_READ_MAX, PTIMER_TIME_0, base64_decode,
    base64_encode, boot0_alive, falcon_offsets, open_sysfs_bar0, open_sysfs_bar0_rw, parse_bdf,
    parse_u32_param,
};
use crate::pure_jsonrpc::types::{JsonRpcError, JsonRpcErrorExt};

/// `ember.fecs.state` — read FECS falcon registers via BAR0 alias offsets.
pub fn ember_fecs_state(params: Option<&Value>) -> Result<Value, JsonRpcError> {
    let bdf = parse_bdf(params)?;
    let bar = open_sysfs_bar0(&bdf)?;

    let cpuctl = bar.read_u32(FECS_CPUCTL as usize);
    let mailbox0 = bar.read_u32(FECS_MAILBOX0 as usize);
    let mailbox1 = bar.read_u32(FECS_MAILBOX1 as usize);
    let os = bar.read_u32(FECS_OS as usize);
    let pc = bar.read_u32(FECS_PC as usize);
    let halted = cpuctl & falcon_offsets::CPUCTL_HALTED != 0;
    let pri_fault = cpuctl & 0xBADF_0000 == 0xBADF_0000;
    let running = boot0_alive(cpuctl) && !halted && !pri_fault;

    debug!(
        bdf = %bdf,
        cpuctl = format!("{cpuctl:#010x}"),
        pc = format!("{pc:#010x}"),
        halted,
        running,
        "ember.fecs.state"
    );
    Ok(serde_json::json!({
        "bdf": bdf,
        "cpuctl": format!("{cpuctl:#010x}"),
        "halted": halted,
        "mailbox0": mailbox0,
        "mailbox1": mailbox1,
        "os": os,
        "running": running,
        "pc": pc,
    }))
}

/// `ember.device.health` — per-device BAR0 health probe.
pub fn ember_device_health(params: Option<&Value>) -> Result<Value, JsonRpcError> {
    let bdf = parse_bdf(params)?;
    let bar = open_sysfs_bar0(&bdf)?;

    let boot0 = bar.read_u32(PMC_BOOT_0 as usize);
    let alive = boot0_alive(boot0);

    let ptimer_a = bar.read_u32(PTIMER_TIME_0 as usize);
    std::thread::sleep(std::time::Duration::from_millis(1));
    let ptimer_b = bar.read_u32(PTIMER_TIME_0 as usize);
    let ptimer_ticking = ptimer_a != ptimer_b && ptimer_a != 0xFFFF_FFFF;

    let pmc_enable = bar.read_u32(PMC_ENABLE as usize);
    let engines_enabled = pmc_enable.count_ones();

    debug!(
        bdf = %bdf,
        boot0 = format!("{boot0:#010x}"),
        alive,
        ptimer_ticking,
        engines_enabled,
        "ember.device.health"
    );
    Ok(serde_json::json!({
        "bdf": bdf,
        "alive": alive,
        "ptimer_ticking": ptimer_ticking,
        "engines_enabled": engines_enabled,
        "boot0": boot0,
    }))
}

/// `ember.device.recover` — attempt MMIO recovery via PMC engine re-enable.
pub fn ember_device_recover(params: Option<&Value>) -> Result<Value, JsonRpcError> {
    let bdf = parse_bdf(params)?;
    let bar_ro = open_sysfs_bar0(&bdf)?;
    let boot0_before = bar_ro.read_u32(PMC_BOOT_0 as usize);

    if !boot0_alive(boot0_before) {
        debug!(bdf = %bdf, boot0_before = format!("{boot0_before:#010x}"), "ember.device.recover: unrecoverable");
        return Ok(serde_json::json!({
            "bdf": bdf,
            "recovered": false,
            "boot0_before": boot0_before,
            "boot0_after": boot0_before,
        }));
    }

    let bar_rw = open_sysfs_bar0_rw(&bdf)?;
    bar_rw
        .write_u32(PMC_ENABLE as usize, PMC_MASK_FULL)
        .map_err(|e| JsonRpcError::internal_error(format!("PMC_ENABLE write failed: {e}")))?;

    let boot0_after = bar_rw.read_u32(PMC_BOOT_0 as usize);
    let recovered = boot0_alive(boot0_after);

    debug!(
        bdf = %bdf,
        boot0_before = format!("{boot0_before:#010x}"),
        boot0_after = format!("{boot0_after:#010x}"),
        recovered,
        "ember.device.recover"
    );
    Ok(serde_json::json!({
        "bdf": bdf,
        "recovered": recovered,
        "boot0_before": boot0_before,
        "boot0_after": boot0_after,
    }))
}

/// `ember.pramin.read` — bulk read from the PRAMIN VRAM window.
pub fn pramin_read(params: Option<&Value>) -> Result<Value, JsonRpcError> {
    let bdf = parse_bdf(params)?;
    let page = parse_u32_param(params, "page", None)?;
    let size = parse_u32_param(params, "size", None)?;
    if size == 0 || size > PRAMIN_READ_MAX {
        return Err(JsonRpcError::invalid_params(format!(
            "Invalid 'size' (must be 1..={PRAMIN_READ_MAX})"
        )));
    }
    if !size.is_multiple_of(4) {
        return Err(JsonRpcError::invalid_params(
            "'size' must be 4-byte aligned",
        ));
    }

    let bar = open_sysfs_bar0_rw(&bdf)?;
    bar.write_u32(PBUS_BAR0_WINDOW as usize, page)
        .map_err(|e| JsonRpcError::internal_error(format!("PRAMIN window select failed: {e}")))?;

    let mut data = Vec::with_capacity(size as usize);
    for i in 0..(size / 4) {
        let offset = PRAMIN_BASE as usize + i as usize * 4;
        let word = bar.read_u32(offset);
        data.extend_from_slice(&word.to_le_bytes());
    }

    debug!(bdf = %bdf, page, bytes = data.len(), "ember.pramin.read");
    Ok(serde_json::json!({
        "bdf": bdf,
        "page": page,
        "data_b64": base64_encode(&data),
        "bytes_read": data.len(),
    }))
}

/// `ember.pramin.write` — stage bytes into VRAM via the PRAMIN window.
pub fn pramin_write(params: Option<&Value>) -> Result<Value, JsonRpcError> {
    let bdf = parse_bdf(params)?;
    let page = parse_u32_param(params, "page", None)?;
    let data_b64 = params
        .and_then(|p| p.get("data_b64"))
        .and_then(Value::as_str)
        .ok_or_else(|| JsonRpcError::invalid_params("Missing 'data_b64' (base64 payload)"))?;

    let data = base64_decode(data_b64)?;
    let bar = open_sysfs_bar0_rw(&bdf)?;

    bar.write_u32(PBUS_BAR0_WINDOW as usize, page)
        .map_err(|e| JsonRpcError::internal_error(format!("PRAMIN window select failed: {e}")))?;

    for (i, chunk) in data.chunks(4).enumerate() {
        let word = u32::from_le_bytes([
            chunk.first().copied().unwrap_or(0),
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
            chunk.get(3).copied().unwrap_or(0),
        ]);
        let offset = PRAMIN_BASE as usize + i * 4;
        bar.write_u32(offset, word).map_err(|e| {
            JsonRpcError::internal_error(format!("PRAMIN write at {offset:#x} failed: {e}"))
        })?;
    }

    debug!(bdf = %bdf, page, bytes = data.len(), "ember.pramin.write");
    Ok(serde_json::json!({
        "bdf": bdf,
        "page": page,
        "bytes_written": data.len(),
    }))
}
