// SPDX-License-Identifier: AGPL-3.0-or-later
//! Helper functions for hw-learn handlers: thermal checks, BDF resolution, store paths.

use crate::pure_jsonrpc::types::{JsonRpcError, JsonRpcErrorExt};
use std::path::PathBuf;

/// Resolve a PCI BDF address — either from params or auto-detect the first NVIDIA GPU.
pub(super) fn resolve_bdf(params: &serde_json::Value) -> Result<String, JsonRpcError> {
    if let Some(bdf) = params.get("bdf").and_then(serde_json::Value::as_str) {
        return Ok(bdf.to_string());
    }

    let gpus = nvpmu::pci::discover_gpus()
        .map_err(|e| JsonRpcError::internal_error(format!("GPU discovery failed: {e}")))?;

    gpus.first()
        .map(|g| g.bdf.clone())
        .ok_or_else(|| JsonRpcError::internal_error("No NVIDIA GPUs found for BAR0 access"))
}

/// Parse mmiotrace text into structured events.
pub(super) fn observe_from_text(
    text: &str,
    label: &str,
) -> Result<hw_learn::observer::ObserveResult, JsonRpcError> {
    let tmp_dir = std::env::temp_dir();
    let tmp_path = tmp_dir.join(format!("hw-learn-{label}-{}.txt", std::process::id()));
    std::fs::write(&tmp_path, text)
        .map_err(|e| JsonRpcError::internal_error(format!("write temp: {e}")))?;

    let config = hw_learn::observer::ObserveConfig {
        mode: hw_learn::observer::TraceMode::MmioTrace,
        trace_path: Some(tmp_path.clone()),
        gpu_selector: hw_learn::observer::GpuSelector::Auto,
        trigger_compute: false,
    };

    let result = hw_learn::TraceObserver::observe(&config)
        .map_err(|e| JsonRpcError::invalid_params(format!("Failed to parse {label} trace: {e}")));

    let _ = std::fs::remove_file(&tmp_path);
    result
}

/// Public thermal check for use by dispatch handler.
pub fn check_thermal_for_bdf_pub(bdf: &str) -> Option<nvpmu::SafetyStatus> {
    check_thermal_for_bdf(bdf)
}

/// Check thermal safety for a GPU before dispatch.
///
/// Attempts to read hwmon sensors for the given BDF. Returns `None` if
/// sensors are unavailable (e.g. no hwmon, proprietary driver).
pub(super) fn check_thermal_for_bdf(bdf: &str) -> Option<nvpmu::SafetyStatus> {
    let device_path =
        std::path::PathBuf::from(toadstool_cylinder::linux_paths::sysfs_pci_device_path(bdf));
    let config = nvpmu::MonitorConfig::default();
    if let Ok(sample) = nvpmu::monitor::sample(&device_path, &config) {
        if sample.status != nvpmu::SafetyStatus::Normal {
            tracing::warn!(
                bdf,
                status = ?sample.status,
                "GPU thermal status is not Normal"
            );
        }
        Some(sample.status)
    } else {
        tracing::debug!(bdf, "hwmon sensors unavailable — skipping thermal check");
        None
    }
}

pub(super) fn vendor_name(id: u16) -> &'static str {
    match id {
        0x10de => "NVIDIA",
        0x1002 => "AMD",
        0x8086 => "Intel",
        0x1E7C => "BrainChip",
        _ => "Unknown",
    }
}

pub(super) fn dirs_for_store() -> PathBuf {
    use toadstool_common::interned_strings::socket_env;

    if let Ok(dir) = std::env::var(socket_env::TOADSTOOL_HW_LEARN_STORE) {
        return PathBuf::from(dir);
    }

    if let Ok(xdg) = std::env::var(socket_env::XDG_DATA_HOME) {
        return PathBuf::from(xdg)
            .join(toadstool_common::constants::primal_identity::PRIMAL_NAME)
            .join("hw-learn-recipes");
    }

    if let Ok(home) = std::env::var(socket_env::HOME) {
        return PathBuf::from(home)
            .join(".local")
            .join("share")
            .join(toadstool_common::constants::primal_identity::PRIMAL_NAME)
            .join("hw-learn-recipes");
    }

    PathBuf::from("hw-learn-recipes")
}
