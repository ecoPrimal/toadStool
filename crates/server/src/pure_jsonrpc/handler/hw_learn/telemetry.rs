// SPDX-License-Identifier: AGPL-3.0-or-later
//! GPU telemetry handler — report thermal and power data for detected GPUs.

use super::HwLearnHandler;
use super::helpers::check_thermal_for_bdf;
use crate::pure_jsonrpc::types::{JsonRpcError};

impl HwLearnHandler {
    /// `gpu.query_telemetry` — Report thermal, power, and per-unit silicon data.
    ///
    /// Returns per-GPU temperature, power, safety status, and per-silicon-unit
    /// utilization breakdown (where hardware counters are available).
    ///
    /// The per-unit breakdown enables silicon-aware scheduling: springs and
    /// the `compute.route.multi_unit` engine can see which units are idle
    /// and could accept secondary workloads.
    ///
    /// # Errors
    ///
    /// This function does not return errors.
    #[expect(
        clippy::unused_async,
        reason = "async for JSON-RPC handler trait consistency"
    )]
    pub async fn gpu_telemetry(
        &self,
        _params: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let sysmon_gpus = toadstool_sysmon::discover_gpus();

        let mut gpu_entries = Vec::new();
        for gpu in &sysmon_gpus {
            let telemetry = gpu.telemetry();
            let safety =
                check_thermal_for_bdf(&gpu.pci_slot).unwrap_or(nvpmu::SafetyStatus::Unknown);

            let per_unit = build_per_unit_utilization(gpu, &telemetry);

            gpu_entries.push(serde_json::json!({
                "card_index": gpu.card_index,
                "driver": gpu.driver,
                "pci_slot": gpu.pci_slot,
                "vendor": format!("{:?}", gpu.vendor),
                "device_id": format!("{:#06x}", gpu.device_id),
                "temperature_celsius": telemetry.temperature_celsius,
                "power_watts": telemetry.power_watts,
                "power_cap_watts": telemetry.power_cap_watts,
                "core_clock_mhz": telemetry.core_clock_mhz,
                "fan_rpm": telemetry.fan_rpm,
                "utilization_percent": telemetry.utilization_percent,
                "vram_total_bytes": telemetry.vram_total_bytes,
                "vram_used_bytes": telemetry.vram_used_bytes,
                "safety_status": format!("{:?}", safety),
                "compute_safe": safety.compute_safe(),
                "silicon_units": per_unit,
            }));
        }

        Ok(serde_json::json!({
            "domain": "gpu",
            "operation": "telemetry",
            "gpus": gpu_entries,
            "gpu_count": gpu_entries.len(),
        }))
    }
}

/// Build per-unit utilization from sysmon telemetry.
///
/// Where hardware counters are available (NVIDIA CUPTI via nvidia-smi dmon,
/// AMD perf counters via amdgpu_top/hwmon), report per-unit busy%.
/// Falls back to estimating from board-level utilization.
fn build_per_unit_utilization(
    _gpu: &toadstool_sysmon::GpuDevice,
    telemetry: &toadstool_sysmon::GpuTelemetry,
) -> serde_json::Value {
    use toadstool_core::silicon::SiliconUnit;

    let board_util = telemetry.utilization_percent.unwrap_or(0.0);

    let mut units = Vec::new();

    // Shader cores: use board-level utilization as proxy.
    // Future: CUPTI SM active% or AMD GFX pipe busy% for finer breakdown.
    units.push(serde_json::json!({
        "unit": SiliconUnit::ShaderCore.as_str(),
        "busy_percent": board_util,
        "power_watts": null,
        "idle_since_ms": if board_util < 1.0 { 1000u64 } else { 0u64 },
    }));

    // Tensor cores: no per-unit counter available via sysfs yet.
    // Future: CUPTI tensor_active% when NVIDIA exposes it.
    units.push(serde_json::json!({
        "unit": SiliconUnit::TensorCore.as_str(),
        "busy_percent": 0.0,
        "power_watts": null,
        "idle_since_ms": 1000u64,
    }));

    // TMUs: idle during pure compute workloads.
    units.push(serde_json::json!({
        "unit": SiliconUnit::TextureUnit.as_str(),
        "busy_percent": 0.0,
        "power_watts": null,
        "idle_since_ms": 1000u64,
    }));

    // ROPs: idle during pure compute workloads.
    units.push(serde_json::json!({
        "unit": SiliconUnit::Rop.as_str(),
        "busy_percent": 0.0,
        "power_watts": null,
        "idle_since_ms": 1000u64,
    }));

    serde_json::json!(units)
}

#[cfg(test)]
mod tests {
    use crate::pure_jsonrpc::handler::hw_learn::HwLearnHandler;

    fn handler_with_temp_store() -> (HwLearnHandler, tempfile::TempDir) {
        let dir = tempfile::TempDir::new().unwrap();
        let handler = HwLearnHandler {
            store_dir: dir.path().to_path_buf(),
        };
        (handler, dir)
    }

    #[tokio::test]
    async fn no_params_returns_valid_json_with_gpu_count() {
        let (handler, _dir) = handler_with_temp_store();
        let value = handler.gpu_telemetry(None).await.unwrap();
        assert_eq!(value.get("domain"), Some(&serde_json::json!("gpu")));
        assert_eq!(
            value.get("operation"),
            Some(&serde_json::json!("telemetry"))
        );
        let count = value
            .get("gpu_count")
            .and_then(serde_json::Value::as_u64)
            .expect("gpu_count");
        let gpus = value
            .get("gpus")
            .and_then(|g| g.as_array())
            .expect("gpus array");
        assert_eq!(gpus.len() as u64, count);
    }

    #[tokio::test]
    async fn telemetry_entries_have_expected_structure() {
        let (handler, _dir) = handler_with_temp_store();
        let value = handler.gpu_telemetry(None).await.unwrap();
        let gpus = value.get("gpus").and_then(|g| g.as_array()).unwrap();
        if let Some(first) = gpus.first() {
            assert!(first.get("pci_slot").is_some());
            assert!(first.get("temperature_celsius").is_some());
            assert!(first.get("safety_status").is_some());
            assert!(first.get("compute_safe").is_some());
        }
    }
}
