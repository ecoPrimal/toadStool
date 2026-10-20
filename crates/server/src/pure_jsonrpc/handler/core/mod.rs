// SPDX-License-Identifier: AGPL-3.0-or-later
//! Core handlers for JSON-RPC: health, version, capabilities, GPU info.
//!
//! Provides health checks, version information, capability discovery,
//! and GPU device/memory queries.

mod compute;
mod health;
mod identity;
mod wire_l3;

pub(crate) use compute::{gpu_info, gpu_memory, version_info};
pub(crate) use health::{
    health, health_drain, health_liveness, health_readiness, health_simple, health_version,
};
pub(crate) use identity::{
    capabilities_list, discover_capabilities, identity_get, primal_announce,
};

use crate::pure_jsonrpc::types::{JsonRpcError};

pub(super) type JsonRpcResult = Result<serde_json::Value, JsonRpcError>;

/// Canonical JSON-RPC method names registered via direct string match in
/// [`crate::pure_jsonrpc::handler::JsonRpcHandler::handle_method`] before semantic
/// registry resolution. Semantic-only names are merged from
/// `SemanticMethodRegistry::semantic_names` at runtime.
pub const DIRECT_JSONRPC_METHODS: &[&str] = &[
    "capabilities.list",
    "identity.get",
    "health",
    "health.liveness",
    "health.readiness",
    "health.check",
    "health.version",
    "health.drain",
    "toadstool.health",
    "toadstool.version",
    "toadstool.submit_workload",
    "toadstool.query_status",
    "toadstool.cancel_workload",
    "toadstool.list_workloads",
    "toadstool.validate",
    "toadstool.query_capabilities",
    "toadstool.resources.estimate",
    "toadstool.resources.validate_availability",
    "toadstool.resources.suggest_optimizations",
    "resources.estimate",
    "resources.validate_availability",
    "resources.suggest_optimizations",
    "ai.local_inference",
    "ai.local_execute",
    "compute.health",
    "compute.version",
    "compute.capabilities",
    "compute.discover_capabilities",
    "compute.execute",
    "compute.submit",
    "compute.status",
    "compute.result",
    "compute.cancel",
    "compute.list",
    "compute.dispatch",
    "compute.dispatch.submit",
    "compute.fan_out",
    "compute.dispatch.status",
    "compute.dispatch.result",
    "compute.dispatch.forward",
    "dispatch.verify_trust",
    "dispatch.telemetry.schema",
    "compute.dispatch.capabilities",
    "compute.dispatch.pipeline.submit",
    "compute.dispatch.pipeline.status",
    "compute.dispatch.multi_gpu",
    "compute.dispatch.halo_exchange",
    "compute.hardware.observe",
    "compute.hardware.distill",
    "compute.hardware.apply",
    "compute.hardware.share_recipe",
    "compute.hardware.auto_init",
    "compute.hardware.auto_init_all",
    "compute.hardware.status",
    "compute.hardware.vfio_devices",
    "compute.performance_surface.report",
    "compute.performance_surface.query",
    "compute.performance_surface.list",
    "compute.route.multi_unit",
    "gpu.query_info",
    "gpu.query_memory",
    "gpu.query_telemetry",
    "gate.update",
    "gate.remove",
    "gate.list",
    "gate.route",
    "transport.discover",
    "transport.list",
    "transport.route",
    "transport.open",
    "transport.stream",
    "transport.status",
    "shader.dispatch",
    "ember.list",
    "ember.status",
    "ember.reacquire",
    "ember.warm_cycle",
    "ember.prepare_dma",
    "ember.cleanup_dma",
    "ember.adopt_device",
    "device.swap",
    "device.warm_catch",
    "device.get",
    "device.experiment_lifecycle",
    "device.reset",
    "device.resurrect",
    "device.health",
    "device.vfio.open",
    "device.vfio.roundtrip",
    "device.gr.init",
    "compute.context.init",
    "sovereign.init",
    "sovereign.boot",
    "sovereign.profile",
    "sovereign.warm_status",
    "sovereign.runlist_diagnostic",
    "sovereign.defense_status",
    "sovereign.watchdog_status",
    "mmio.read32",
    "mmio.write32",
    "mmio.batch",
    "mmio.pramin.read32",
    "mmio.bar0.probe",
    "mmio.falcon.status",
    "ember.falcon.upload_imem",
    "ember.falcon.upload_dmem",
    "ember.falcon.start_cpu",
    "ember.falcon.poll",
    "ember.pramin.write",
    "ember.pramin.read",
    "ember.fecs.state",
    "ember.device.health",
    "ember.device.recover",
    "primal.announce",
    "provenance.query",
];

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{Duration, Instant};

    use toadstool::semantic_methods::SemanticMethodRegistry;

    use super::*;

    #[tokio::test]
    async fn health_liveness_always_alive() {
        let v = health_liveness().await.expect("ok");
        assert_eq!(v, serde_json::json!({ "status": "alive" }));
    }

    #[tokio::test]
    async fn health_readiness_includes_ready_and_version() {
        let v = health_readiness("v-ready-1", true).await.expect("ok");
        assert_eq!(v["status"], "ready");
        assert_eq!(v["version"], "v-ready-1");
    }

    #[tokio::test]
    async fn health_readiness_starting_before_ready() {
        let v = health_readiness("v-start-1", false).await.expect("ok");
        assert_eq!(v["status"], "starting");
        assert_eq!(v["version"], "v-start-1");
    }

    #[tokio::test]
    async fn health_includes_version_uptime_error_count_and_wire_status() {
        let ver = "unit-test-9.9.9";
        let start = Instant::now()
            .checked_sub(Duration::from_secs(2))
            .expect("instant");
        let errors = AtomicU64::new(7);
        let v = health(&std::sync::Arc::from(ver), start, &errors)
            .await
            .expect("health ok");
        assert_eq!(v["healthy"], true);
        assert_eq!(
            v["status"], "alive",
            "Wire Standard L1: status must be 'alive'"
        );
        assert_eq!(v["details"]["version"], ver);
        assert!(v["uptime_secs"].as_u64().unwrap() >= 2);
        assert_eq!(v["details"]["error_count"], 7);
        errors.fetch_add(1, Ordering::Relaxed);
        let v2 = health(&std::sync::Arc::from(ver), start, &errors)
            .await
            .expect("health ok");
        assert_eq!(v2["details"]["error_count"], 8);
        assert_eq!(v2["status"], "alive");
    }

    #[tokio::test]
    async fn health_version_includes_session_and_build_hash() {
        let v = health_version("v-ver-1").await.expect("ok");
        assert_eq!(v["version"], "v-ver-1");
        assert!(v["session"].as_str().is_some(), "session field required");
        assert!(
            v["build_hash"].as_str().is_some(),
            "build_hash field required"
        );
        assert_eq!(v["service"], "toadstool");
    }

    #[tokio::test]
    async fn health_drain_sets_status() {
        let draining = std::sync::atomic::AtomicBool::new(false);
        let ready = std::sync::atomic::AtomicBool::new(true);
        let v = health_drain(&draining, &ready).await.expect("ok");
        assert_eq!(v["status"], "draining");
        assert_eq!(v["accepting_new_work"], false);
        assert!(
            draining.load(Ordering::Relaxed),
            "draining flag should be set"
        );
        assert!(
            !ready.load(Ordering::Relaxed),
            "ready flag should be cleared"
        );
    }

    #[tokio::test]
    async fn version_info_maps_expected_keys() {
        let v = version_info("v-x").await.expect("version_info");
        assert_eq!(v["version"], "v-x");
        assert_eq!(v["protocol"], "JSON-RPC 2.0");
        assert_eq!(v["service"], "ToadStool Compute");
        assert!(v["implementation"].as_str().unwrap().contains("Pure Rust"));
    }

    #[tokio::test]
    async fn discover_capabilities_merges_registry_and_sorts_methods() {
        let reg = SemanticMethodRegistry::new();
        let cap = discover_capabilities(&reg, "cap-ver-1")
            .await
            .expect("discover_capabilities");
        let methods = cap["methods"].as_array().expect("methods array");
        assert!(!methods.is_empty(), "methods should be non-empty");
        let as_strs: Vec<&str> = methods
            .iter()
            .map(|m| m.as_str().expect("string"))
            .collect();
        let mut sorted = as_strs.clone();
        sorted.sort_unstable();
        assert_eq!(as_strs, sorted, "methods must be sorted");
        assert!(
            as_strs.contains(&"compute.execute"),
            "registry-only semantic should be merged in"
        );
        assert_eq!(cap["version"], "cap-ver-1");
        let node_caps = cap["node_capabilities"].as_array().expect("node caps");
        assert!(node_caps.iter().any(|c| c.as_str() == Some("compute")));
    }

    #[tokio::test]
    async fn capabilities_list_returns_wire_standard_envelope() {
        let reg = SemanticMethodRegistry::new();
        let cap = capabilities_list(&reg, "wire-1.0")
            .await
            .expect("capabilities_list");
        assert_eq!(cap["primal"], toadstool_common::constants::PRIMAL_NAME);
        assert_eq!(cap["version"], "wire-1.0");

        let methods = cap["methods"].as_array().expect("methods array");
        assert!(!methods.is_empty());
        let strs: Vec<&str> = methods.iter().map(|m| m.as_str().unwrap()).collect();
        let mut sorted = strs.clone();
        sorted.sort_unstable();
        assert_eq!(strs, sorted, "methods must be sorted");

        assert!(
            strs.contains(&"capabilities.list"),
            "must advertise capabilities.list"
        );
        assert!(
            strs.contains(&"health.liveness"),
            "must advertise health.liveness"
        );
        assert!(
            strs.contains(&"identity.get"),
            "must advertise identity.get"
        );
        assert!(
            strs.contains(&"compute.submit"),
            "must advertise compute.submit"
        );
        assert!(
            strs.contains(&"shader.dispatch"),
            "must advertise shader.dispatch"
        );
        assert!(strs.contains(&"ember.list"), "must advertise ember.list");

        let groups = cap["provided_capabilities"]
            .as_array()
            .expect("provided_capabilities");
        assert!(groups.len() >= 5, "should have multiple capability groups");
        let group_types: Vec<&str> = groups.iter().map(|g| g["type"].as_str().unwrap()).collect();
        assert!(group_types.contains(&"compute"));
        assert!(group_types.contains(&"toadstool"));
        assert!(group_types.contains(&"gpu"));
        assert!(group_types.contains(&"transport"));
        assert!(group_types.contains(&"shader"));

        assert!(cap["consumed_capabilities"].as_array().is_some());
        assert_eq!(cap["protocol"], "jsonrpc-2.0");

        // Wire Standard L3: cost_estimates
        let costs = cap["cost_estimates"].as_object().expect("cost_estimates");
        assert!(
            costs.len() >= 40,
            "should have cost estimates for most methods"
        );
        let health_cost = &costs["health.liveness"];
        assert_eq!(health_cost["cpu"], "negligible");
        assert_eq!(health_cost["energy"], "negligible");
        assert_eq!(health_cost["gpu_eligible"], false);
        let dispatch_cost = &costs["shader.dispatch"];
        assert_eq!(dispatch_cost["cpu"], "high");
        assert_eq!(dispatch_cost["energy"], "high");
        assert_eq!(dispatch_cost["gpu_eligible"], true);
        let submit_cost = &costs["compute.submit"];
        assert_eq!(submit_cost["cpu"], "variable");
        assert_eq!(submit_cost["energy"], "variable");

        // Wire Standard L3: operation_dependencies
        let deps = cap["operation_dependencies"]
            .as_object()
            .expect("operation_dependencies");
        assert!(deps.len() >= 15, "should have dependency entries");
        let status_deps = deps["compute.status"].as_array().expect("array");
        assert!(status_deps.iter().any(|d| d == "compute.submit"));
        let stream_deps = deps["transport.stream"].as_array().expect("array");
        assert!(stream_deps.iter().any(|d| d == "transport.open"));
    }

    #[tokio::test]
    async fn identity_get_includes_domain_and_license() {
        let reg = SemanticMethodRegistry::new();
        let id = identity_get("id-ver", &reg).await.expect("identity_get");
        assert_eq!(id["primal"], toadstool_common::constants::PRIMAL_NAME);
        assert_eq!(id["version"], "id-ver");
        assert_eq!(id["domain"], "compute", "Wire Standard L2: domain field");
        assert_eq!(
            id["license"], "AGPL-3.0-or-later",
            "Wire Standard L2: license field"
        );
        assert_eq!(id["protocol"], "JSON-RPC 2.0");
        assert_eq!(id["transport"], "unix-socket");
        let sock = id["socket_name"].as_str().expect("socket_name");
        assert!(
            std::path::Path::new(sock)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("sock"))
        );
        let caps = id["capabilities"].as_array().expect("capabilities");
        assert!(caps.iter().any(|c| c.as_str() == Some("compute")));
        let methods = id["methods"].as_array().expect("methods");
        assert!(methods.len() > 3);
    }

    #[tokio::test]
    async fn gpu_info_returns_device_and_driver_shape() {
        let g = gpu_info().await.expect("gpu_info");
        assert!(g.get("devices").is_some());
        assert_eq!(g["driver"], "wgpu");
        assert!(g.get("compute_backends").is_some());
        assert!(g.get("spirv_codegen_safety").is_some());
        assert!(g.get("firmware_inventory").is_some());
    }

    #[tokio::test]
    async fn gpu_memory_returns_devices_array() {
        let m = gpu_memory().await.expect("gpu_memory");
        assert!(m.get("devices").is_some());
    }
}
