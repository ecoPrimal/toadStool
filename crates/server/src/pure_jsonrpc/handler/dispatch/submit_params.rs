// SPDX-License-Identifier: AGPL-3.0-or-later
//! Parameter resolution helpers for `dispatch.submit`.
//!
//! Extracted from `submit.rs` to keep handler orchestration separate from
//! binary decoding, workgroup resolution, buffer transform, and shader info mapping.

use crate::pure_jsonrpc::handler::method_gate::CallerContext;
use crate::pure_jsonrpc::types::{JsonRpcError, JsonRpcErrorExt};
use base64::Engine;

/// Enforce resource envelope limits on a dispatch request (JH-2).
///
/// When the caller has a token with a [`ResourceEnvelope`], this checks:
/// - `binary_size` against `mem_mb` (binary rounded up to MB <= envelope limit)
/// - `workgroup_total` against `cpu_cores` (total threads <= core limit * 1024)
/// - `timeout_ms` against `max_timeout_ms`
///
/// Returns `Ok(())` if no envelope is present or all checks pass.
pub(in crate::pure_jsonrpc::handler::dispatch) fn enforce_envelope(
    ctx: &CallerContext,
    binary_size: usize,
    workgroup_total: u64,
    timeout_ms: u64,
) -> Result<(), JsonRpcError> {
    let Some(ref env) = ctx.envelope else {
        return Ok(());
    };

    if let Some(mem_mb) = env.mem_mb {
        let binary_mb = (binary_size as u64).saturating_add(1024 * 1024 - 1) / (1024 * 1024);
        if binary_mb > mem_mb {
            return Err(resource_exhausted(format!(
                "Binary size ({binary_mb} MB) exceeds token envelope mem_mb ({mem_mb} MB)"
            )));
        }
    }

    if let Some(cpu_cores) = env.cpu_cores {
        let thread_cap = u64::from(cpu_cores) * 1024;
        if workgroup_total > thread_cap {
            return Err(resource_exhausted(format!(
                "Workgroup total ({workgroup_total} threads) exceeds token envelope \
                 cpu_cores ({cpu_cores}) \u{00d7} 1024 = {thread_cap} thread cap"
            )));
        }
    }

    if let Some(max_timeout) = env.max_timeout_ms
        && timeout_ms > max_timeout
    {
        return Err(resource_exhausted(format!(
            "Requested timeout ({timeout_ms} ms) exceeds token envelope \
             max_timeout_ms ({max_timeout} ms)"
        )));
    }

    Ok(())
}

/// Resolve the binary payload from either `binary_b64` (base64, preferred) or
/// `binary` (JSON u8 array, legacy).
pub(in crate::pure_jsonrpc::handler::dispatch) fn resolve_binary_param(
    p: &serde_json::Value,
) -> Result<Vec<u8>, JsonRpcError> {
    if let Some(b64) = p.get("binary_b64").and_then(|v| v.as_str()) {
        return base64::engine::general_purpose::STANDARD
            .decode(b64)
            .map_err(|e| {
                JsonRpcError::invalid_params(format!("binary_b64 base64 decode error: {e}"))
            });
    }

    if let Some(arr) = p.get("binary").and_then(|v| v.as_array()) {
        return Ok(arr.iter().map(|v| v.as_u64().unwrap_or(0) as u8).collect());
    }

    Err(JsonRpcError::invalid_params(
        "Missing 'binary' (u8 array) or 'binary_b64' (base64 string)",
    ))
}

/// Resolve workgroup/dispatch dimensions from `dispatch_dims` (trio standard)
/// or `workgroup_size` (legacy), defaulting to [256, 1, 1].
pub(in crate::pure_jsonrpc::handler::dispatch) fn resolve_workgroup_size(
    p: &serde_json::Value,
) -> [u32; 3] {
    let dims = p
        .get("dispatch_dims")
        .or_else(|| p.get("workgroup_size"))
        .and_then(|v| v.as_array());

    dims.map_or([256, 1, 1], |arr| {
        let x = arr
            .first()
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(256) as u32;
        let y = arr.get(1).and_then(serde_json::Value::as_u64).unwrap_or(1) as u32;
        let z = arr.get(2).and_then(serde_json::Value::as_u64).unwrap_or(1) as u32;
        [x, y, z]
    })
}

/// Resolve buffer descriptors. Accepts trio-standard `buffers[]` with `data_b64`
/// fields — decodes them to `data` (u8 arrays) for downstream consumption.
///
/// Field-level clones are necessary since the input is borrowed (`&Value`);
/// only buffers with `data_b64` are rebuilt (others pass through unchanged).
pub(in crate::pure_jsonrpc::handler::dispatch) fn resolve_buffers(
    p: &serde_json::Value,
) -> serde_json::Value {
    let Some(buffers) = p.get("buffers").and_then(|v| v.as_array()) else {
        return serde_json::json!([]);
    };

    let resolved: Vec<serde_json::Value> = buffers
        .iter()
        .map(|buf| {
            let Some(obj) = buf.as_object() else {
                return buf.clone();
            };
            if let Some(b64) = obj.get("data_b64").and_then(|v| v.as_str()) {
                let mut out = serde_json::Map::with_capacity(obj.len());
                for (k, v) in obj {
                    if k != "data_b64" {
                        out.insert(k.clone(), v.clone());
                    }
                }
                if let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(b64) {
                    out.insert("data".into(), serde_json::json!(decoded));
                }
                serde_json::Value::Object(out)
            } else {
                buf.clone()
            }
        })
        .collect();

    serde_json::json!(resolved)
}

/// Resolve shader metadata from JSON, accepting both toadStool-native and
/// compilation provider `CompilationInfoResponse` field names.
///
/// toadStool names: `gpr_count`, `shared_mem_bytes`, `barrier_count`, `wave_size`, `local_mem_bytes`
/// compilation provider names: `gprs`, `shared_memory`, `barriers`, `wave_size`, `local_memory`
#[cfg(target_os = "linux")]
pub(in crate::pure_jsonrpc::handler::dispatch) fn resolve_shader_info(
    si: &serde_json::Value,
    workgroup: [u32; 3],
) -> toadstool_cylinder::ShaderInfo {
    let u32_field = |primary: &str, alias: &str| -> u32 {
        si.get(primary)
            .or_else(|| si.get(alias))
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0) as u32
    };

    toadstool_cylinder::ShaderInfo {
        gpr_count: u32_field("gpr_count", "gprs"),
        shared_mem_bytes: u32_field("shared_mem_bytes", "shared_memory"),
        barrier_count: u32_field("barrier_count", "barriers"),
        workgroup,
        wave_size: si
            .get("wave_size")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(32) as u32,
        local_mem_bytes: si
            .get("local_mem_bytes")
            .or_else(|| si.get("local_memory"))
            .and_then(serde_json::Value::as_u64)
            .map(|v| v as u32),
    }
}

pub(in crate::pure_jsonrpc::handler::dispatch) fn resource_exhausted(
    msg: impl Into<String>,
) -> JsonRpcError {
    JsonRpcError {
        code: toadstool_common::constants::jsonrpc::error_codes::RESOURCE_EXHAUSTED,
        message: msg.into(),
        data: None,
    }
}
