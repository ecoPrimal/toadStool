// SPDX-License-Identifier: AGPL-3.0-or-later

//! Performance surface and multi-unit routing JSON-RPC handlers.
//!
//! Springs report measured throughput for `(operation, silicon_unit, precision)`
//! triples. toadStool stores these and uses them for tolerance-based routing
//! across all silicon units on the GPU die.

use std::sync::RwLock;

use toadstool_core::silicon::{
    MultiUnitRoutingPlan, PerformanceMeasurement, PerformanceSurfaceEntry, RoutedOperation,
    SiliconEnergyLedger, SiliconUnit,
};

use crate::pure_jsonrpc::types::{JsonRpcError, JsonRpcErrorExt};

/// Handler for `compute.performance_surface.*` and `compute.silicon_ledger.*` JSON-RPC methods.
///
/// Uses `std::sync::RwLock` for synchronous lock acquisition and
/// cannot block the runtime under contention.
pub struct SiliconHandler {
    measurements: RwLock<Vec<PerformanceMeasurement>>,
    energy_ledger: RwLock<Option<SiliconEnergyLedger>>,
}

impl SiliconHandler {
    /// Create a new silicon handler with an empty performance surface.
    pub fn new() -> Self {
        Self {
            measurements: RwLock::new(Vec::new()),
            energy_ledger: RwLock::new(None),
        }
    }

    /// `compute.performance_surface.report` — record a spring experiment measurement.
    pub async fn report(
        &self,
        params: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let params = params.ok_or_else(|| JsonRpcError::invalid_params("missing params"))?;

        let measurement: PerformanceMeasurement = serde::Deserialize::deserialize(params)
            .map_err(|e| JsonRpcError::invalid_params(format!("invalid measurement: {e}")))?;

        let unit_name = measurement.silicon_unit.as_str().to_string();
        let op_name = measurement.operation.clone();

        let mut store = self
            .measurements
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        store.push(measurement);
        let total = store.len();

        Ok(serde_json::json!({
            "status": "recorded",
            "operation": op_name,
            "silicon_unit": unit_name,
            "total_measurements": total
        }))
    }

    /// `compute.performance_surface.query` — find the best unit for an operation+tolerance.
    pub async fn query(
        &self,
        params: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let params = params.ok_or_else(|| JsonRpcError::invalid_params("missing params"))?;

        let operation = params
            .get("operation")
            .and_then(|v| v.as_str())
            .ok_or_else(|| JsonRpcError::invalid_params("missing 'operation'"))?;

        let tolerance = params
            .get("tolerance_required")
            .and_then(serde_json::Value::as_f64)
            .ok_or_else(|| JsonRpcError::invalid_params("missing 'tolerance_required'"))?;

        let store = self
            .measurements
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let matching: Vec<&PerformanceMeasurement> = store
            .iter()
            .filter(|m| m.operation == operation && m.tolerance_achieved <= tolerance)
            .collect();

        if matching.is_empty() {
            return Ok(serde_json::json!({
                "operation": operation,
                "tolerance_required": tolerance,
                "recommendation": null,
                "message": "no measurements found for this operation and tolerance"
            }));
        }

        let Some(best) = matching.iter().max_by(|a, b| {
            a.throughput_gflops
                .partial_cmp(&b.throughput_gflops)
                .unwrap_or(std::cmp::Ordering::Equal)
        }) else {
            return Err(JsonRpcError::internal_error("no matching measurement"));
        };

        let fallback = matching
            .iter()
            .filter(|m| m.silicon_unit == SiliconUnit::ShaderCore)
            .max_by(|a, b| {
                a.throughput_gflops
                    .partial_cmp(&b.throughput_gflops)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

        let entry = PerformanceSurfaceEntry {
            operation: operation.to_string(),
            tolerance_required: tolerance,
            recommended_unit: best.silicon_unit,
            recommended_precision: best.precision_mode.clone(),
            estimated_throughput_gflops: best.throughput_gflops,
            fallback_unit: fallback.map_or(SiliconUnit::ShaderCore, |f| f.silicon_unit),
            fallback_throughput_gflops: fallback.map_or(0.0, |f| f.throughput_gflops),
        };

        serde_json::to_value(&entry)
            .map_err(|e| JsonRpcError::internal_error(format!("serialize: {e}")))
    }

    /// `compute.performance_surface.list` — list all measurements and available operations.
    pub async fn list(&self) -> Result<serde_json::Value, JsonRpcError> {
        let store = self
            .measurements
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let operations: Vec<&str> = {
            let mut ops: Vec<&str> = store.iter().map(|m| m.operation.as_str()).collect();
            ops.sort_unstable();
            ops.dedup();
            ops
        };

        let gpu_models: Vec<&str> = {
            let mut models: Vec<&str> = store.iter().map(|m| m.gpu_model.as_str()).collect();
            models.sort_unstable();
            models.dedup();
            models
        };

        let silicon_units: Vec<&str> = {
            let mut units: Vec<&str> = store.iter().map(|m| m.silicon_unit.as_str()).collect();
            units.sort_unstable();
            units.dedup();
            units
        };

        Ok(serde_json::json!({
            "total_measurements": store.len(),
            "operations": operations,
            "gpu_models": gpu_models,
            "silicon_units": silicon_units,
            "all_known_units": SiliconUnit::ALL.iter().map(toadstool_core::SiliconUnit::as_str).collect::<Vec<_>>()
        }))
    }

    /// `compute.route.multi_unit` — build a routing plan for a compound workload.
    ///
    /// Each operation in the workload specifies a tolerance. The routing engine
    /// consults both the performance surface and the silicon energy ledger:
    /// 1. Find highest-throughput unit meeting tolerance (performance surface)
    /// 2. If idle units are powered but unused (energy ledger), schedule
    ///    secondary workloads on them even when less efficient — the silicon
    ///    is already consuming power, so "free" throughput is available.
    ///
    /// This is where "use a less efficient secondary system because the silicon
    /// is idle and we have other workload" lives.
    pub async fn route_multi_unit(
        &self,
        params: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let params = params.ok_or_else(|| JsonRpcError::invalid_params("missing params"))?;

        let workload = params
            .get("workload")
            .and_then(|v| v.as_array())
            .ok_or_else(|| JsonRpcError::invalid_params("missing 'workload' array"))?;

        if workload.is_empty() {
            return Err(JsonRpcError::invalid_params("'workload' array is empty"));
        }

        let gpu_target = params
            .get("gpu")
            .and_then(|v| v.as_str())
            .unwrap_or("default");

        let energy_aware = params
            .get("energy_aware")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true);

        let store = self
            .measurements
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let ledger = self
            .energy_ledger
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let idle_units: Vec<SiliconUnit> = if energy_aware {
            ledger
                .as_ref()
                .map(|l| l.idle_units.clone())
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        let mut routed_ops = Vec::with_capacity(workload.len());
        let mut total_throughput = 0.0_f64;
        let mut units_in_use: Vec<SiliconUnit> = Vec::new();

        for item in workload {
            let op = item
                .get("op")
                .and_then(|v| v.as_str())
                .ok_or_else(|| JsonRpcError::invalid_params("workload item missing 'op'"))?;

            let tolerance = item
                .get("tolerance")
                .and_then(serde_json::Value::as_f64)
                .ok_or_else(|| JsonRpcError::invalid_params("workload item missing 'tolerance'"))?;

            let prefer_idle = item
                .get("prefer_idle_unit")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);

            let mut routed = route_single_op(&store, op, tolerance);

            if prefer_idle
                && !idle_units.is_empty()
                && let Some(idle_candidate) = idle_units
                    .iter()
                    .find(|u| !units_in_use.contains(u) && **u != routed.silicon_unit)
            {
                let idle_measurement = store.iter().find(|m| {
                    m.silicon_unit == *idle_candidate
                        && m.operation == op
                        && m.tolerance_achieved <= tolerance
                });
                if let Some(m) = idle_measurement {
                    routed = RoutedOperation {
                        operation: op.to_string(),
                        silicon_unit: *idle_candidate,
                        precision_mode: m.precision_mode.clone(),
                        estimated_throughput_gflops: m.throughput_gflops,
                        reason: format!(
                            "idle-silicon opportunistic: {} is powered but unused, \
                                 scheduling at {:.0} GFLOPS (vs {:.0} on primary)",
                            idle_candidate, m.throughput_gflops, routed.estimated_throughput_gflops
                        ),
                        fallback: Some(Box::new(routed)),
                    };
                }
            }

            units_in_use.push(routed.silicon_unit);
            total_throughput += routed.estimated_throughput_gflops;
            routed_ops.push(routed);
        }

        let plan = MultiUnitRoutingPlan {
            operations: routed_ops,
            total_estimated_throughput_gflops: total_throughput,
            gpu_target: gpu_target.to_string(),
        };

        serde_json::to_value(&plan)
            .map_err(|e| JsonRpcError::internal_error(format!("serialize: {e}")))
    }

    /// `compute.silicon_ledger.report` — record a silicon energy ledger snapshot.
    ///
    /// Springs or telemetry agents push per-trajectory or per-window ledger data.
    /// toadStool stores the latest snapshot for routing decisions.
    pub async fn silicon_ledger_report(
        &self,
        params: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let params = params.ok_or_else(|| JsonRpcError::invalid_params("missing params"))?;

        let ledger: SiliconEnergyLedger = serde::Deserialize::deserialize(params)
            .map_err(|e| JsonRpcError::invalid_params(format!("invalid ledger: {e}")))?;

        let gpu_id = ledger.gpu_id.clone();
        let idle_count = ledger.idle_units.len();
        let efficiency = ledger.utilization_efficiency();

        let mut store = self
            .energy_ledger
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *store = Some(ledger);

        Ok(serde_json::json!({
            "status": "recorded",
            "gpu_id": gpu_id,
            "idle_units": idle_count,
            "utilization_efficiency": efficiency
        }))
    }

    /// `compute.silicon_ledger.query` — retrieve the current energy ledger.
    ///
    /// Returns the latest per-unit time/energy/idle breakdown, enabling
    /// callers to make energy-aware scheduling decisions.
    pub async fn silicon_ledger_query(
        &self,
        _params: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let store = self
            .energy_ledger
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        match store.as_ref() {
            Some(ledger) => serde_json::to_value(ledger)
                .map_err(|e| JsonRpcError::internal_error(format!("serialize: {e}"))),
            None => Ok(serde_json::json!({
                "status": "no_data",
                "message": "no silicon energy ledger has been reported yet"
            })),
        }
    }
}

/// Route a single operation by consulting the performance surface.
///
/// Strategy:
/// 1. Find all measurements for this op where tolerance is met
/// 2. Pick the highest-throughput unit as primary
/// 3. Pick shader-core measurement as fallback (always available)
/// 4. If no measurements exist, fall back to heuristic defaults
fn route_single_op(store: &[PerformanceMeasurement], op: &str, tolerance: f64) -> RoutedOperation {
    let matching: Vec<&PerformanceMeasurement> = store
        .iter()
        .filter(|m| m.operation == op && m.tolerance_achieved <= tolerance)
        .collect();

    if matching.is_empty() {
        return route_heuristic(op, tolerance);
    }

    let Some(best) = matching.iter().max_by(|a, b| {
        a.throughput_gflops
            .partial_cmp(&b.throughput_gflops)
            .unwrap_or(std::cmp::Ordering::Equal)
    }) else {
        return route_heuristic(op, tolerance);
    };

    let shader_fallback = matching
        .iter()
        .filter(|m| m.silicon_unit == SiliconUnit::ShaderCore)
        .max_by(|a, b| {
            a.throughput_gflops
                .partial_cmp(&b.throughput_gflops)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

    let fallback = if best.silicon_unit == SiliconUnit::ShaderCore {
        None
    } else {
        Some(Box::new(RoutedOperation {
            operation: op.to_string(),
            silicon_unit: shader_fallback.map_or(SiliconUnit::ShaderCore, |f| f.silicon_unit),
            precision_mode: shader_fallback.map_or_else(
                || precision_for_tolerance(tolerance),
                |f| f.precision_mode.clone(),
            ),
            estimated_throughput_gflops: shader_fallback.map_or(0.0, |f| f.throughput_gflops),
            reason: String::from("shader core fallback"),
            fallback: None,
        }))
    };

    RoutedOperation {
        operation: op.to_string(),
        silicon_unit: best.silicon_unit,
        precision_mode: best.precision_mode.clone(),
        estimated_throughput_gflops: best.throughput_gflops,
        reason: format!(
            "{} at {} achieves {:.0} GFLOPS within tolerance {:.0e}",
            best.silicon_unit, best.precision_mode, best.throughput_gflops, tolerance
        ),
        fallback,
    }
}

/// Heuristic routing when no performance surface data exists for an operation.
///
/// Uses tolerance thresholds to pick reasonable defaults based on the
/// silicon budget table from specs/README.md.
fn route_heuristic(op: &str, tolerance: f64) -> RoutedOperation {
    let op_lower = op.to_lowercase();

    let (unit, precision, throughput_est, reason) = if op_lower.contains("neighbor")
        || op_lower.contains("spatial")
        || op_lower.contains("bvh")
    {
        (
            SiliconUnit::RtCore,
            "fp32",
            5400.0,
            "spatial query heuristic — RT cores for BVH traversal",
        )
    } else if op_lower.contains("histogram")
        || op_lower.contains("scatter")
        || op_lower.contains("deposit")
    {
        (
            SiliconUnit::Rop,
            "fp32",
            2700.0,
            "scatter/additive heuristic — ROPs for per-pixel atomic ops",
        )
    } else if op_lower.contains("lookup") || op_lower.contains("table") || op_lower.contains("eos")
    {
        (
            SiliconUnit::TextureUnit,
            "fp32",
            4000.0,
            "table lookup heuristic — TMUs for interpolated reads",
        )
    } else if op_lower.contains("matmul")
        || op_lower.contains("mma")
        || op_lower.contains("cg_solve")
    {
        if tolerance >= 1e-4 {
            (
                SiliconUnit::TensorCore,
                "fp16",
                142_000.0,
                "matrix heuristic — tensor cores at FP16 for loose tolerance",
            )
        } else {
            (
                SiliconUnit::ShaderCore,
                &*precision_for_tolerance(tolerance),
                throughput_for_tolerance(tolerance),
                "matrix heuristic — shader cores for tight tolerance",
            )
        }
    } else {
        (
            SiliconUnit::ShaderCore,
            &*precision_for_tolerance(tolerance),
            throughput_for_tolerance(tolerance),
            "default heuristic — shader cores",
        )
    };

    let fallback = if unit == SiliconUnit::ShaderCore {
        None
    } else {
        Some(Box::new(RoutedOperation {
            operation: op.to_string(),
            silicon_unit: SiliconUnit::ShaderCore,
            precision_mode: precision_for_tolerance(tolerance),
            estimated_throughput_gflops: throughput_for_tolerance(tolerance),
            reason: String::from("shader core fallback (heuristic)"),
            fallback: None,
        }))
    };

    RoutedOperation {
        operation: op.to_string(),
        silicon_unit: unit,
        precision_mode: precision.to_string(),
        estimated_throughput_gflops: throughput_est,
        reason: reason.to_string(),
        fallback,
    }
}

/// Select precision mode based on tolerance threshold.
fn precision_for_tolerance(tolerance: f64) -> String {
    if tolerance <= 1e-14 {
        String::from("df64")
    } else if tolerance <= 1e-7 {
        String::from("fp32")
    } else {
        String::from("fp16")
    }
}

/// Estimate shader-core throughput (GFLOPS) based on tolerance/precision.
///
/// Reference numbers from RTX 3090 silicon budget (specs/README.md).
fn throughput_for_tolerance(tolerance: f64) -> f64 {
    if tolerance <= 1e-14 {
        3240.0 // DF64 on FP32 ALUs
    } else if tolerance <= 1e-7 {
        35_580.0 // FP32 native
    } else {
        71_160.0 // FP16 on shader cores
    }
}

#[cfg(test)]
#[path = "silicon_tests.rs"]
mod tests;
