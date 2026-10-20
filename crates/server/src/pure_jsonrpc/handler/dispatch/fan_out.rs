// SPDX-License-Identifier: AGPL-3.0-or-later
//! Fan-out dispatch — parallel clone processing for large batch workloads.
//!
//! Wire contract per S263 handoff + Wave 38 scale target (Tenaillon 590 GB):
//! - Accepts `work_units[]` with optional `substrate_filter` and `dag_session_id`
//! - Assigns units to available substrate (local_cylinder for GPU, cpu for CPU)
//! - Queues units when `gpu_required` and no GPU substrate available
//! - Auto-generates `unit_id` when not provided
//! - Degradation: sequential `compute.dispatch.submit` produces identical results

use serde::Deserialize;

use super::DispatchHandler;
use super::types::{FanOutAssignment, FanOutUnitStatus, FanOutWorkUnit, SubstrateFilter};
use crate::pure_jsonrpc::handler::method_gate::CallerContext;
use crate::pure_jsonrpc::types::{JsonRpcError, JsonRpcErrorExt};
use std::sync::atomic::Ordering;

impl DispatchHandler {
    /// Handle `compute.fan_out` — parallel dispatch of multiple work units.
    pub(crate) async fn fan_out(
        &self,
        params: Option<&serde_json::Value>,
        ctx: &CallerContext,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let params = params
            .ok_or_else(|| JsonRpcError::invalid_params("compute.fan_out requires params"))?;

        let work_units: Vec<FanOutWorkUnit> = params
            .get("work_units")
            .and_then(|v| Vec::<FanOutWorkUnit>::deserialize(v).ok())
            .ok_or_else(|| {
                JsonRpcError::invalid_params("compute.fan_out requires 'work_units' array")
            })?;

        if work_units.is_empty() {
            return Err(JsonRpcError::invalid_params(
                "compute.fan_out requires at least one work unit",
            ));
        }

        let filter: SubstrateFilter = params
            .get("substrate_filter")
            .and_then(|v| SubstrateFilter::deserialize(v).ok())
            .unwrap_or_default();

        let dag_session_id = params
            .get("dag_session_id")
            .and_then(|v| v.as_str())
            .map(String::from);

        if filter.gpu_required {
            self.pre_dispatch_resource_check("auto", Some(ctx), Some(params))
                .await?;
        }

        if let Some(ref env) = ctx.envelope
            && let Some(cpu_cores) = env.cpu_cores
        {
            let max_concurrent = u64::from(cpu_cores) * 4;
            if work_units.len() as u64 > max_concurrent {
                return Err(JsonRpcError::server_error(
                    toadstool_common::constants::jsonrpc::error_codes::RESOURCE_EXHAUSTED,
                    format!(
                        "Fan-out unit count ({}) exceeds envelope cpu_cores ({}) × 4 = {} max concurrent",
                        work_units.len(),
                        cpu_cores,
                        max_concurrent,
                    ),
                ));
            }
        }

        let dispatch_id = format!("fan-{}", uuid::Uuid::new_v4().as_hyphenated());

        #[cfg(target_os = "linux")]
        let has_gpu = self.local_device_factory.is_some();
        #[cfg(not(target_os = "linux"))]
        let has_gpu = false;

        let mut assigned = Vec::new();
        let mut queued = Vec::new();

        for (i, unit) in work_units.iter().enumerate() {
            let unit_id = unit
                .unit_id
                .clone()
                .unwrap_or_else(|| format!("{dispatch_id}-{i}"));

            if filter.gpu_required && !has_gpu {
                queued.push(FanOutAssignment {
                    unit_id,
                    status: FanOutUnitStatus::Queued,
                    substrate: "queued",
                });
            } else {
                assigned.push(FanOutAssignment {
                    unit_id,
                    status: FanOutUnitStatus::Assigned,
                    substrate: if has_gpu { "local_cylinder" } else { "cpu" },
                });
            }
        }

        self.dispatch_count.fetch_add(1, Ordering::Relaxed);

        let total_units = assigned.len() + queued.len();

        let mut result = serde_json::json!({
            "dispatch_id": dispatch_id,
            "assigned": assigned,
            "queued": queued,
            "total_units": total_units,
            "assigned_count": assigned.len(),
            "queued_count": queued.len(),
            "timing": {
                "dispatch_ms": 0,
            },
        });

        if let Some(session_id) = dag_session_id {
            result["dag_session_id"] = serde_json::Value::String(session_id);
        }

        result["caller"] = serde_json::json!({
            "gate_id": ctx.gate_id,
            "trust_level": ctx.trust_level,
        });

        super::telemetry::emit_dispatch_completion_telemetry(
            &super::telemetry::DispatchTelemetryEmit {
                ctx,
                method: "compute.fan_out",
                dispatch_ms: 0,
                readback_ms: 0,
                dispatch_mode: if has_gpu { "local_cylinder" } else { "cpu" },
                bdf: "auto",
                binary_size: 0,
                workgroup_size: [total_units as u32, 1, 1],
                timeout_ms: 0,
                success: true,
            },
        );

        Ok(result)
    }
}
