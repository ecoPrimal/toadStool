// SPDX-License-Identifier: AGPL-3.0-or-later
#![allow(
    clippy::cast_precision_loss,
    clippy::expect_used,
    clippy::float_cmp,
    clippy::no_effect_underscore_binding,
    clippy::unreadable_literal
)]
//! Supplemental coverage for `tarpc_server` and tarpc RPC wire types:
//! serde round-trips, semantic method helpers, workload map edge cases, and health
//! metrics with `Running` workloads.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::AtomicU8;

use tarpc::context::Context;

use crate::rpc_types::{
    AvailableResources, ComputeUnit, ExecutionMetrics, HealthStatus, ResourceRequirements,
    TarpcWorkloadSubmission, ToadStoolComputeRpc, WorkloadPriority, WorkloadResult, WorkloadStatus,
    WorkloadSubmission, semantic_methods,
};
use crate::tarpc_server::{
    StandaloneExecutor, TestWorkloadDouble, ToadStoolTarpcServer, WorkloadExecutor,
    WorkloadExecutorDispatch,
};

fn round_trip_json<T>(value: &T) -> T
where
    T: serde::Serialize + serde::de::DeserializeOwned,
{
    let json = serde_json::to_string(value).expect("serialize");
    serde_json::from_str(&json).expect("deserialize")
}

#[tokio::test]
async fn semantic_methods_round_trip_and_registry() {
    let pairs = [
        "submit_workload",
        "query_status",
        "cancel_workload",
        "list_workloads",
        "query_capabilities",
        "health_check",
    ];
    for rust in pairs {
        let sem = semantic_methods::get_semantic_name(rust).expect("semantic name");
        assert!(semantic_methods::is_semantic_method(sem));
        assert_eq!(semantic_methods::get_rust_method(sem), Some(rust));
    }
    let all = semantic_methods::all_semantic_methods();
    assert_eq!(all.len(), pairs.len());
    assert!(all.iter().all(|s| semantic_methods::is_semantic_method(s)));
}

#[tokio::test]
async fn execution_metrics_serde_round_trip() {
    let m = ExecutionMetrics {
        queued_duration_secs: 1.25,
        execution_duration_secs: 3.5,
        cpu_cores_used: 8,
        memory_used_bytes: 16_777_216,
        gpu_memory_used_bytes: Some(512_000_000),
    };
    let restored = round_trip_json(&m);
    assert_eq!(restored.queued_duration_secs, m.queued_duration_secs);
    assert_eq!(restored.execution_duration_secs, m.execution_duration_secs);
    assert_eq!(restored.cpu_cores_used, m.cpu_cores_used);
    assert_eq!(restored.memory_used_bytes, m.memory_used_bytes);
    assert_eq!(restored.gpu_memory_used_bytes, m.gpu_memory_used_bytes);
    let cloned = m.clone();
    assert_eq!(cloned.cpu_cores_used, m.cpu_cores_used);
    let _ = format!("{m:?}");
}

#[tokio::test]
async fn compute_unit_serde_round_trip() {
    let u = ComputeUnit {
        id: "gpu-7".to_string(),
        unit_type: "gpu".to_string(),
        name: "Test GPU".to_string(),
        cores: 8192,
        memory_bytes: 24_000_000_000,
        tflops: Some(12.5),
        utilization: 0.33,
    };
    let restored = round_trip_json(&u);
    assert_eq!(restored.id, u.id);
    assert_eq!(restored.tflops, u.tflops);
    let cloned = u.clone();
    assert_eq!(cloned.name, u.name);
    let _ = format!("{u:?}");
}

#[tokio::test]
async fn available_resources_serde_round_trip() {
    let a = AvailableResources {
        total_cpu_cores: 32,
        available_cpu_cores: 12,
        total_memory_bytes: 64_000_000_000,
        available_memory_bytes: 8_000_000_000,
        total_gpu_memory_bytes: Some(48_000_000_000),
        available_gpu_memory_bytes: Some(24_000_000_000),
        cpu_utilization: 0.4,
        memory_utilization: 0.55,
        gpu_utilization: Some(0.2),
    };
    let restored = round_trip_json(&a);
    assert_eq!(restored.total_cpu_cores, a.total_cpu_cores);
    assert_eq!(restored.gpu_utilization, a.gpu_utilization);
    let cloned = a.clone();
    assert_eq!(cloned.available_cpu_cores, a.available_cpu_cores);
    let _ = format!("{a:?}");
}

#[tokio::test]
async fn workload_priority_serde_all_variants() {
    for p in [
        WorkloadPriority::Low,
        WorkloadPriority::Normal,
        WorkloadPriority::High,
        WorkloadPriority::Critical,
    ] {
        let json = serde_json::to_string(&p).expect("serialize priority");
        let restored: WorkloadPriority = serde_json::from_str(&json).expect("deserialize");
        let json2 = serde_json::to_string(&restored).expect("serialize again");
        assert_eq!(json, json2);
        let cloned = p;
        assert_eq!(
            serde_json::to_string(&cloned).expect("c"),
            serde_json::to_string(&p).expect("p")
        );
        let _ = format!("{p:?}");
    }
}

#[tokio::test]
async fn resource_requirements_serde_default_and_clone() {
    let r = ResourceRequirements {
        cpu_cores: Some(4),
        memory_bytes: Some(4096),
        gpu_memory_bytes: Some(2048),
        timeout_secs: Some(120),
    };
    let restored = round_trip_json(&r);
    assert_eq!(restored.cpu_cores, r.cpu_cores);
    assert_eq!(restored.timeout_secs, r.timeout_secs);
    let def = ResourceRequirements::default();
    let def2 = round_trip_json(&def);
    assert_eq!(def2.cpu_cores, None);
    assert_eq!(def.clone().memory_bytes, def.memory_bytes);
    let _ = format!("{r:?}");
}

#[tokio::test]
async fn workload_status_serde_all_variants() {
    for s in [
        WorkloadStatus::Pending,
        WorkloadStatus::Queued,
        WorkloadStatus::Running,
        WorkloadStatus::Completed,
        WorkloadStatus::Failed,
        WorkloadStatus::Cancelled,
    ] {
        let restored: WorkloadStatus = round_trip_json(&s);
        assert_eq!(format!("{restored:?}"), format!("{s:?}"));
    }
}

#[tokio::test]
async fn health_status_clone_debug_serde() {
    let mut h = HealthStatus::ok().with_uptime(42)
        .with_detail("version", serde_json::Value::String("9.9.9".into()))
        .with_detail("resource_utilization", serde_json::json!(0.88))
        .with_detail("active_workloads", serde_json::json!(3))
        .with_detail("queued_workloads", serde_json::json!(1))
        .with_detail("error_count", serde_json::json!(7));
    h.healthy = false;
    let restored = round_trip_json(&h);
    assert_eq!(restored.details["error_count"], h.details["error_count"]);
    let c = h.clone();
    assert_eq!(c.details["version"], h.details["version"]);
    let dbg = format!("{h:?}");
    assert!(dbg.contains("9.9.9"));
}

#[tokio::test]
async fn workload_result_with_error_field_serde_round_trip() {
    let r = WorkloadResult {
        workload_id: Arc::from("wl-err"),
        status: WorkloadStatus::Failed,
        data: None,
        error: Some("boom".to_string()),
        metrics: ExecutionMetrics {
            queued_duration_secs: 0.0,
            execution_duration_secs: 0.0,
            cpu_cores_used: 0,
            memory_used_bytes: 0,
            gpu_memory_used_bytes: None,
        },
    };
    let restored = round_trip_json(&r);
    assert_eq!(restored.error, r.error);
    assert_eq!(restored.workload_id.as_ref(), "wl-err");
}

#[tokio::test]
async fn tarpc_workload_submission_wire_type_serde() {
    let t = TarpcWorkloadSubmission {
        workload_id: "ext-1".to_string(),
        runtime_type: "native".to_string(),
        payload: bytes::Bytes::from_static(b"hello"),
        resources: ResourceRequirements {
            cpu_cores: Some(2),
            memory_bytes: None,
            gpu_memory_bytes: None,
            timeout_secs: Some(30),
        },
        metadata: HashMap::from([("k".to_string(), "v".to_string())]),
    };
    let restored = round_trip_json(&t);
    assert_eq!(restored.workload_id, t.workload_id);
    assert_eq!(restored.payload.as_ref(), t.payload.as_ref());
}

#[tokio::test]
async fn standalone_executor_new_and_default_equivalent_service_id() {
    let a = StandaloneExecutor::new();
    let b = StandaloneExecutor::default();
    let ca = a.query_capabilities().await.expect("caps a");
    let cb = b.query_capabilities().await.expect("caps b");
    assert_eq!(ca.service_id, cb.service_id);
}

#[tokio::test]
async fn duplicate_workload_id_keeps_first_map_entry() {
    let executor = Arc::new(WorkloadExecutorDispatch::TestDouble(
        TestWorkloadDouble::SeqTag(Arc::new(AtomicU8::new(0))),
    ));
    let server = ToadStoolTarpcServer::new("v", executor, None);
    let sub = |id: &str| WorkloadSubmission {
        workload_id: Arc::from(id),
        workload_type: Arc::from("cpu_compute"),
        data: vec![].into(),
        metadata: HashMap::new(),
        priority: WorkloadPriority::Normal,
        requirements: ResourceRequirements::default(),
    };

    let out1 = server
        .clone()
        .submit_workload(Context::current(), sub("same-id"))
        .await
        .expect("first submit");
    let out2 = server
        .clone()
        .submit_workload(Context::current(), sub("same-id"))
        .await
        .expect("second submit");

    assert_eq!(
        out1.data.as_ref().map(std::convert::AsRef::as_ref),
        Some([0u8].as_slice())
    );
    assert_eq!(
        out2.data.as_ref().map(std::convert::AsRef::as_ref),
        Some([1u8].as_slice())
    );

    let stored = server
        .query_status(Context::current(), "same-id".to_string())
        .await
        .expect("query");
    assert_eq!(
        stored.data.as_ref().map(std::convert::AsRef::as_ref),
        Some([0u8].as_slice())
    );
}

#[tokio::test]
async fn health_check_counts_running_without_queued() {
    let server = ToadStoolTarpcServer::new(
        "v",
        Arc::new(WorkloadExecutorDispatch::TestDouble(
            TestWorkloadDouble::Running,
        )),
        None,
    );
    server
        .clone()
        .submit_workload(
            Context::current(),
            WorkloadSubmission {
                workload_id: Arc::from("run-1"),
                workload_type: Arc::from("cpu_compute"),
                data: vec![].into(),
                metadata: HashMap::new(),
                priority: WorkloadPriority::Normal,
                requirements: ResourceRequirements::default(),
            },
        )
        .await
        .expect("submit");

    let h = server
        .health_check(Context::current())
        .await
        .expect("health");
    assert_eq!(h.details["active_workloads"].as_u64().unwrap(), 1);
    assert_eq!(h.details["queued_workloads"].as_u64().unwrap(), 0);
}
