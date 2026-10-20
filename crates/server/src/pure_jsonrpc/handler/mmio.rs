// SPDX-License-Identifier: AGPL-3.0-or-later
//! Core MMIO JSON-RPC handlers and shared helpers.
//!
//! Provides BAR0 register access via `mmio.read32`, `mmio.write32`, `mmio.batch`,
//! `mmio.pramin.read32`, and `mmio.bar0.probe`.
//!
//! Falcon and ember device handlers live in sibling modules (`mmio_falcon`,
//! `mmio_ember`) and are re-exported here so the router can use `mmio::*`.
//!
//! All MMIO operations use fork-isolated access via `toadstool-cylinder`'s
//! `SysfsBar0` or `MappedBar` to protect the daemon from hardware hangs.

use crate::pure_jsonrpc::types::{JsonRpcError, JsonRpcErrorExt};
use serde_json::Value;
use tracing::debug;

pub use super::mmio_ember::{
    ember_device_health, ember_device_recover, ember_fecs_state, pramin_read, pramin_write,
};
pub use super::mmio_falcon::{
    falcon_poll, falcon_start_cpu, falcon_upload_dmem, falcon_upload_imem, mmio_falcon_status,
};

/// Well-known falcon engine base addresses (BAR0-relative).
pub(super) mod falcon_bases {
    pub const PMU: u32 = 0x0010_A000;
    pub const FECS: u32 = 0x0040_9800;
    pub const GPCCS: u32 = 0x0050_2800;
    pub const SEC2: u32 = 0x0084_0000;
}

/// Falcon register offsets (relative to engine base).
pub(super) mod falcon_offsets {
    pub const MAILBOX0: u32 = 0x040;
    pub const MAILBOX1: u32 = 0x044;
    pub const OS: u32 = 0x080;
    pub const CPUCTL: u32 = 0x100;
    pub const BOOTVEC: u32 = 0x104;
    pub const HWCFG: u32 = 0x108;
    /// CPUCTL bit 1 — release falcon from HRESET (v4+).
    pub const CPUCTL_STARTCPU: u32 = 1 << 1;
    /// CPUCTL bit 5 — falcon halted.
    pub const CPUCTL_HALTED: u32 = 1 << 5;
}

pub(super) const PMC_BOOT_0: u32 = 0x0000_0000;
pub(super) const PMC_ENABLE: u32 = 0x0000_0200;
pub(super) const PMC_MASK_FULL: u32 = 0xFFFF_FFFF;
pub(super) const PRAMIN_BASE: u32 = 0x0070_0000;
pub(super) const PBUS_BAR0_WINDOW: u32 = 0x0000_1700;
pub(super) const PTIMER_TIME_0: u32 = 0x0000_9400;
pub(super) const FECS_CPUCTL: u32 = 0x0040_9100;
pub(super) const FECS_MAILBOX0: u32 = 0x0040_9040;
pub(super) const FECS_MAILBOX1: u32 = 0x0040_9044;
pub(super) const FECS_OS: u32 = 0x0040_9080;
pub(super) const FECS_PC: u32 = 0x0040_9030;
pub(super) const PRAMIN_READ_MAX: u32 = 4096;
const DEFAULT_BAR0_SIZE: usize = 16 * 1024 * 1024;

pub(super) fn base64_decode(s: &str) -> Result<Vec<u8>, JsonRpcError> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(s)
        .map_err(|e| JsonRpcError::invalid_params(format!("base64 decode: {e}")))
}

pub(super) fn base64_encode(data: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(data)
}

pub(super) fn boot0_alive(value: u32) -> bool {
    value != 0 && value != 0xFFFF_FFFF
}

pub(super) fn parse_u32_param(
    params: Option<&Value>,
    key: &str,
    default: Option<u32>,
) -> Result<u32, JsonRpcError> {
    match params.and_then(|p| p.get(key)) {
        Some(v) => v
            .as_u64()
            .or_else(|| {
                v.as_str()
                    .and_then(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).ok())
                    .map(u64::from)
            })
            .and_then(|n| u32::try_from(n).ok())
            .ok_or_else(|| {
                JsonRpcError::invalid_params(format!("Invalid '{key}' (u32 or hex string)"))
            }),
        None => default.ok_or_else(|| JsonRpcError::invalid_params(format!("Missing '{key}'"))),
    }
}

pub(super) fn parse_base(params: Option<&Value>) -> Result<usize, JsonRpcError> {
    Ok(parse_u32_param(params, "base", None)? as usize)
}

/// Open read-write BAR0 as [`MappedBar`] for falcon PIO library calls.
pub(super) fn open_mapped_bar0_rw(
    bdf: &str,
) -> Result<toadstool_cylinder::vfio::device::MappedBar, JsonRpcError> {
    toadstool_cylinder::vfio::ember_gate::check_channel(bdf)
        .map_err(|e| JsonRpcError::internal_error(format!("ember gate for {bdf}: {e}")))?;
    toadstool_cylinder::vfio::device::MappedBar::from_sysfs_rw(bdf, DEFAULT_BAR0_SIZE)
        .map_err(|e| JsonRpcError::internal_error(format!("BAR0 open (rw) failed for {bdf}: {e}")))
}

pub(super) fn parse_bdf(params: Option<&Value>) -> Result<String, JsonRpcError> {
    params
        .and_then(|p| p.get("bdf"))
        .and_then(Value::as_str)
        .map(String::from)
        .ok_or_else(|| JsonRpcError::invalid_params("Missing 'bdf' string parameter"))
}

fn parse_offset(params: Option<&Value>) -> Result<u32, JsonRpcError> {
    params
        .and_then(|p| p.get("offset"))
        .and_then(|v| {
            v.as_u64().or_else(|| {
                v.as_str()
                    .and_then(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).ok())
                    .map(u64::from)
            })
        })
        .and_then(|v| u32::try_from(v).ok())
        .ok_or_else(|| {
            JsonRpcError::invalid_params("Missing or invalid 'offset' (u32 or hex string)")
        })
}

/// Open a read-only sysfs BAR0 mapping for a PCI device.
pub(super) fn open_sysfs_bar0(
    bdf: &str,
) -> Result<toadstool_cylinder::vfio::sysfs_bar0::SysfsBar0, JsonRpcError> {
    toadstool_cylinder::vfio::sysfs_bar0::SysfsBar0::open(bdf, DEFAULT_BAR0_SIZE)
        .map_err(|e| JsonRpcError::internal_error(format!("BAR0 open failed for {bdf}: {e}")))
}

/// Open a read-write sysfs BAR0 mapping for a PCI device.
pub(super) fn open_sysfs_bar0_rw(
    bdf: &str,
) -> Result<toadstool_cylinder::vfio::sysfs_bar0::SysfsBar0Rw, JsonRpcError> {
    toadstool_cylinder::vfio::sysfs_bar0::SysfsBar0Rw::open(bdf, DEFAULT_BAR0_SIZE)
        .map_err(|e| JsonRpcError::internal_error(format!("BAR0 open (rw) failed for {bdf}: {e}")))
}

/// `mmio.read32` — read a single 32-bit BAR0 register.
pub fn mmio_read32(params: Option<&Value>) -> Result<Value, JsonRpcError> {
    let bdf = parse_bdf(params)?;
    let offset = parse_offset(params)?;
    let bar = open_sysfs_bar0(&bdf)?;
    let value = bar.read_u32(offset as usize);

    debug!(bdf = %bdf, offset = format!("{offset:#010x}"), value = format!("{value:#010x}"), "mmio.read32");
    Ok(serde_json::json!({ "bdf": bdf, "offset": format!("{offset:#010x}"), "value": value }))
}

/// `mmio.write32` — write a single 32-bit BAR0 register.
pub fn mmio_write32(params: Option<&Value>) -> Result<Value, JsonRpcError> {
    let bdf = parse_bdf(params)?;
    let offset = parse_offset(params)?;
    let value = params
        .and_then(|p| p.get("value"))
        .and_then(Value::as_u64)
        .and_then(|v| u32::try_from(v).ok())
        .ok_or_else(|| JsonRpcError::invalid_params("Missing 'value' (u32)"))?;

    let bar = open_sysfs_bar0_rw(&bdf)?;
    bar.write_u32(offset as usize, value)
        .map_err(|e| JsonRpcError::internal_error(format!("MMIO write failed: {e}")))?;

    debug!(bdf = %bdf, offset = format!("{offset:#010x}"), value = format!("{value:#010x}"), "mmio.write32");
    Ok(serde_json::json!({ "bdf": bdf, "offset": format!("{offset:#010x}"), "ok": true }))
}

/// `mmio.batch` — batch read/write operations on BAR0.
///
/// Each op: `{ "offset": u32, "value"?: u32 }`.
/// If `value` is present, it's a write; otherwise a read.
pub fn mmio_batch(params: Option<&Value>) -> Result<Value, JsonRpcError> {
    let bdf = parse_bdf(params)?;
    let ops = params
        .and_then(|p| p.get("ops"))
        .and_then(Value::as_array)
        .ok_or_else(|| JsonRpcError::invalid_params("Missing 'ops' array"))?;

    let has_writes = ops.iter().any(|op| op.get("value").is_some());
    let bar_rw = if has_writes {
        Some(open_sysfs_bar0_rw(&bdf)?)
    } else {
        None
    };
    let bar_ro = if has_writes {
        None
    } else {
        Some(open_sysfs_bar0(&bdf)?)
    };
    let mut results = Vec::with_capacity(ops.len());

    for op in ops {
        let offset = op
            .get("offset")
            .and_then(Value::as_u64)
            .and_then(|v| u32::try_from(v).ok())
            .ok_or_else(|| JsonRpcError::invalid_params("Each op needs 'offset' (u32)"))?;

        if let Some(val) = op.get("value").and_then(Value::as_u64) {
            let val32 = u32::try_from(val)
                .map_err(|_| JsonRpcError::invalid_params("value exceeds u32"))?;
            if let Some(ref rw) = bar_rw {
                match rw.write_u32(offset as usize, val32) {
                    Ok(()) => results.push(serde_json::json!({ "offset": offset, "wrote": val32 })),
                    Err(e) => results
                        .push(serde_json::json!({ "offset": offset, "error": e.to_string() })),
                }
            }
        } else if let Some(ref rw) = bar_rw {
            let v = rw.read_u32(offset as usize);
            results.push(serde_json::json!({ "offset": offset, "value": v }));
        } else if let Some(ref ro) = bar_ro {
            let v = ro.read_u32(offset as usize);
            results.push(serde_json::json!({ "offset": offset, "value": v }));
        }
    }

    debug!(bdf = %bdf, count = results.len(), "mmio.batch");
    Ok(serde_json::json!({ "bdf": bdf, "results": results }))
}

/// `mmio.pramin.read32` — read a 32-bit value from PRAMIN window.
///
/// PRAMIN is the PMC window into VRAM/RAMIN at BAR0+0x700000.
pub fn mmio_pramin_read32(params: Option<&Value>) -> Result<Value, JsonRpcError> {
    let bdf = parse_bdf(params)?;
    let offset = parse_offset(params)?;

    let phys_offset = PRAMIN_BASE
        .checked_add(offset)
        .ok_or_else(|| JsonRpcError::invalid_params("PRAMIN offset overflow"))?;

    let bar = open_sysfs_bar0(&bdf)?;
    let value = bar.read_u32(phys_offset as usize);

    debug!(bdf = %bdf, offset = format!("{offset:#010x}"), value = format!("{value:#010x}"), "mmio.pramin.read32");
    Ok(serde_json::json!({ "bdf": bdf, "offset": format!("{offset:#010x}"), "value": value }))
}

/// `mmio.bar0.probe` — read chip identity and PMC state from BAR0.
pub fn mmio_bar0_probe(params: Option<&Value>) -> Result<Value, JsonRpcError> {
    let bdf = parse_bdf(params)?;
    let bar = open_sysfs_bar0(&bdf)?;

    let boot0 = bar.read_u32(PMC_BOOT_0 as usize);
    let pmc_enable = bar.read_u32(PMC_ENABLE as usize);
    let chip_id = (boot0 >> 20) & 0x1FF;
    let vendor = boot0 & 0xFFF;

    debug!(bdf = %bdf, boot0 = format!("{boot0:#010x}"), chip_id = format!("{chip_id:#05x}"), "mmio.bar0.probe");
    Ok(serde_json::json!({
        "bdf": bdf,
        "boot0": boot0,
        "chip_id": chip_id,
        "vendor": vendor,
        "pmc_enable": pmc_enable,
        "responsive": boot0 != 0 && boot0 != 0xFFFF_FFFF,
    }))
}

#[cfg(test)]
#[path = "mmio_tests.rs"]
mod tests;
