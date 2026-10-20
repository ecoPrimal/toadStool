// SPDX-License-Identifier: AGPL-3.0-or-later
//! Falcon microcontroller JSON-RPC handlers.
//!
//! `mmio.falcon.status`, `ember.falcon.upload_imem`, `ember.falcon.upload_dmem`,
//! `ember.falcon.start_cpu`, `ember.falcon.poll`.

use serde_json::Value;
use tracing::debug;

use super::mmio::{
    base64_decode, falcon_bases, falcon_offsets, open_mapped_bar0_rw, open_sysfs_bar0,
    open_sysfs_bar0_rw, parse_base, parse_bdf, parse_u32_param,
};
use crate::pure_jsonrpc::types::{JsonRpcError, JsonRpcErrorExt};

/// `mmio.falcon.status` — read falcon microcontroller registers.
pub fn mmio_falcon_status(params: Option<&Value>) -> Result<Value, JsonRpcError> {
    let bdf = parse_bdf(params)?;
    let engine = params
        .and_then(|p| p.get("engine"))
        .and_then(Value::as_str)
        .unwrap_or("pmu");

    let base = match engine.to_ascii_lowercase().as_str() {
        "pmu" => falcon_bases::PMU,
        "fecs" => falcon_bases::FECS,
        "gpccs" => falcon_bases::GPCCS,
        "sec2" => falcon_bases::SEC2,
        other => {
            return Err(JsonRpcError::invalid_params(format!(
                "Unknown falcon engine '{other}'. Valid: pmu, fecs, gpccs, sec2"
            )));
        }
    };

    let bar = open_sysfs_bar0(&bdf)?;
    let read = |off: u32| bar.read_u32((base + off) as usize);

    let cpuctl = read(falcon_offsets::CPUCTL);
    let mailbox0 = read(falcon_offsets::MAILBOX0);
    let mailbox1 = read(falcon_offsets::MAILBOX1);
    let os = read(falcon_offsets::OS);
    let bootvec = read(falcon_offsets::BOOTVEC);
    let hwcfg = read(falcon_offsets::HWCFG);

    let halted = (cpuctl & 0x20) != 0;

    debug!(bdf = %bdf, engine, cpuctl = format!("{cpuctl:#010x}"), halted, "mmio.falcon.status");
    Ok(serde_json::json!({
        "bdf": bdf,
        "engine": engine,
        "cpuctl": cpuctl,
        "mailbox0": mailbox0,
        "mailbox1": mailbox1,
        "os": os,
        "bootvec": bootvec,
        "hwcfg": hwcfg,
        "halted": halted,
    }))
}

/// `ember.falcon.upload_imem` — upload firmware to falcon IMEM via PIO ports.
pub fn falcon_upload_imem(params: Option<&Value>) -> Result<Value, JsonRpcError> {
    let bdf = parse_bdf(params)?;
    let base = parse_base(params)?;
    let addr = parse_u32_param(params, "offset", Some(0))?;
    let data_b64 = params
        .and_then(|p| p.get("data_b64"))
        .and_then(Value::as_str)
        .ok_or_else(|| JsonRpcError::invalid_params("Missing 'data_b64' (base64 firmware)"))?;
    let secure = params
        .and_then(|p| p.get("secure"))
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let data = base64_decode(data_b64)?;
    let bar = open_mapped_bar0_rw(&bdf)?;

    toadstool_cylinder::nv::falcon_pio::falcon_upload_imem(&bar, base, addr, &data, secure);

    debug!(bdf = %bdf, base, addr, bytes = data.len(), secure, "ember.falcon.upload_imem");
    Ok(serde_json::json!({
        "bdf": bdf,
        "base": base,
        "offset": addr,
        "bytes_written": data.len(),
        "secure": secure,
    }))
}

/// `ember.falcon.upload_dmem` — upload data to falcon DMEM via PIO ports.
pub fn falcon_upload_dmem(params: Option<&Value>) -> Result<Value, JsonRpcError> {
    let bdf = parse_bdf(params)?;
    let base = parse_base(params)?;
    let addr = parse_u32_param(params, "offset", Some(0))?;
    let data_b64 = params
        .and_then(|p| p.get("data_b64"))
        .and_then(Value::as_str)
        .ok_or_else(|| JsonRpcError::invalid_params("Missing 'data_b64' (base64 payload)"))?;

    let data = base64_decode(data_b64)?;
    let bar = open_mapped_bar0_rw(&bdf)?;

    toadstool_cylinder::nv::falcon_pio::falcon_upload_dmem(&bar, base, addr, &data);

    debug!(bdf = %bdf, base, addr, bytes = data.len(), "ember.falcon.upload_dmem");
    Ok(serde_json::json!({
        "bdf": bdf,
        "base": base,
        "offset": addr,
        "bytes_written": data.len(),
    }))
}

/// `ember.falcon.start_cpu` — set boot vector and release falcon from HRESET.
pub fn falcon_start_cpu(params: Option<&Value>) -> Result<Value, JsonRpcError> {
    let bdf = parse_bdf(params)?;
    let base = parse_base(params)?;
    let bootvec = parse_u32_param(params, "bootvec", None)?;
    let bar = open_sysfs_bar0_rw(&bdf)?;

    bar.write_u32(base + falcon_offsets::BOOTVEC as usize, bootvec)
        .map_err(|e| JsonRpcError::internal_error(format!("BOOTVEC write failed: {e}")))?;
    bar.write_u32(
        base + falcon_offsets::CPUCTL as usize,
        falcon_offsets::CPUCTL_STARTCPU,
    )
    .map_err(|e| JsonRpcError::internal_error(format!("CPUCTL write failed: {e}")))?;

    debug!(bdf = %bdf, base, bootvec, "ember.falcon.start_cpu");
    Ok(serde_json::json!({
        "bdf": bdf,
        "bootvec": bootvec,
        "started": true,
    }))
}

/// `ember.falcon.poll` — poll falcon CPUCTL until halted or timeout.
pub fn falcon_poll(params: Option<&Value>) -> Result<Value, JsonRpcError> {
    let bdf = parse_bdf(params)?;
    let base = parse_base(params)?;
    let timeout_ms = params
        .and_then(|p| p.get("timeout_ms"))
        .and_then(Value::as_u64)
        .unwrap_or(5000);
    let bar = open_sysfs_bar0(&bdf)?;

    let start = std::time::Instant::now();
    let mut cpuctl;
    loop {
        cpuctl = bar.read_u32(base + falcon_offsets::CPUCTL as usize);
        let halted = cpuctl & falcon_offsets::CPUCTL_HALTED != 0;
        if halted || start.elapsed().as_millis() >= u128::from(timeout_ms) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }

    let mailbox0 = bar.read_u32(base + falcon_offsets::MAILBOX0 as usize);
    let mailbox1 = bar.read_u32(base + falcon_offsets::MAILBOX1 as usize);
    let halted = cpuctl & falcon_offsets::CPUCTL_HALTED != 0;

    debug!(
        bdf = %bdf,
        base,
        halted,
        cpuctl = format!("{cpuctl:#010x}"),
        elapsed_ms = start.elapsed().as_millis(),
        "ember.falcon.poll"
    );
    Ok(serde_json::json!({
        "bdf": bdf,
        "halted": halted,
        "cpuctl": format!("{cpuctl:#010x}"),
        "mailbox0": mailbox0,
        "mailbox1": mailbox1,
        "elapsed_ms": start.elapsed().as_millis() as u64,
    }))
}
