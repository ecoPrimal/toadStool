// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;
use crate::rpc_types::{ResourceRequirements, WorkloadPriority};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

fn arc_test_dispatch(kind: TestWorkloadDouble) -> Arc<WorkloadExecutorDispatch> {
    Arc::new(WorkloadExecutorDispatch::TestDouble(kind))
}

#[test]
fn test_tarpc_server_new() {
    let executor = arc_test_dispatch(TestWorkloadDouble::Mock);
    let server = ToadStoolTarpcServer::new("test-v1", executor, None);
    assert!(server.error_count.load(Ordering::Relaxed) == 0);
}

#[test]
fn test_tarpc_server_new_with_error_count() {
    let executor = arc_test_dispatch(TestWorkloadDouble::Mock);
    let error_count = Arc::new(AtomicU64::new(42));
    let server = ToadStoolTarpcServer::new("v1", executor, Some(Arc::clone(&error_count)));
    assert_eq!(server.error_count.load(Ordering::Relaxed), 42);
}

#[test]
fn test_tarpc_server_new_with_version_arc_str() {
    let executor = arc_test_dispatch(TestWorkloadDouble::Mock);
    let version: Arc<str> = Arc::from("arc-version");
    let server = ToadStoolTarpcServer::new(version, executor, None);
    assert_eq!(server.version.as_ref(), "arc-version");
}

#[test]
fn test_tarpc_server_clone() {
    let executor = arc_test_dispatch(TestWorkloadDouble::Mock);
    let server = ToadStoolTarpcServer::new("v1", executor, None);
    let cloned = server.clone();
    assert_eq!(server.version.as_ref(), cloned.version.as_ref());
}

#[test]
fn test_standalone_executor_new() {
    let exec = StandaloneExecutor::new();
    assert_eq!(exec.capabilities.service_id, "toadstool-standalone");
    assert!(!exec.capabilities.compute_units.is_empty());
    assert!(
        exec.capabilities
            .supported_workload_types
            .contains(&"cpu_compute".to_string())
    );
}

#[test]
fn test_standalone_executor_default() {
    let exec = StandaloneExecutor::default();
    assert_eq!(exec.capabilities.service_id, "toadstool-standalone");
}

fn mk_submission(id: &str, workload_type: &str, data: Vec<u8>) -> WorkloadSubmission {
    WorkloadSubmission {
        workload_id: Arc::from(id),
        workload_type: Arc::from(workload_type),
        data: data.into(),
        metadata: std::collections::HashMap::new(),
        priority: WorkloadPriority::Normal,
        requirements: ResourceRequirements::default(),
    }
}

#[tokio::test]
async fn test_standalone_executor_execute() {
    let exec = StandaloneExecutor::new();
    let submission = mk_submission("test-wl-1", "cpu_compute", vec![1, 2, 3]);
    let result = exec.execute(submission).await;
    assert!(result.is_ok());
    let res = result.unwrap();
    assert_eq!(res.workload_id.as_ref(), "test-wl-1");
    assert!(matches!(res.status, WorkloadStatus::Completed));
    assert!(res.data.is_some());
}

#[tokio::test]
async fn test_standalone_executor_execute_gpu_hint() {
    let exec = StandaloneExecutor::new();
    let submission = mk_submission("gpu-wl", "gpu_compute", vec![0u8; 100]);
    let result = exec.execute(submission).await;
    assert!(result.is_ok());
    let res = result.unwrap();
    assert!(res.metrics.gpu_memory_used_bytes.is_some());
}

#[tokio::test]
async fn test_standalone_executor_query_capabilities() {
    let exec = StandaloneExecutor::new();
    let caps = exec.query_capabilities().await;
    assert!(caps.is_ok());
    let c = caps.unwrap();
    assert_eq!(c.service_id, "toadstool-standalone");
    assert!(!c.compute_units.is_empty());
}

#[tokio::test]
async fn test_standalone_executor_cancel() {
    let exec = StandaloneExecutor::new();
    let result = exec.cancel("any-id").await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_mock_executor_submit_and_query() {
    let executor = arc_test_dispatch(TestWorkloadDouble::Mock);
    let server = ToadStoolTarpcServer::new("test", executor, None);
    let submission = mk_submission("mock-1", "cpu_compute", vec![]);
    let result = server
        .clone()
        .submit_workload(tarpc::context::current(), submission)
        .await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap().workload_id.as_ref(), "mock-1");
}

#[tokio::test]
async fn test_mock_executor_query_status() {
    let executor = arc_test_dispatch(TestWorkloadDouble::Mock);
    let server = ToadStoolTarpcServer::new("test", executor, None);
    let submission = mk_submission("status-test", "cpu_compute", vec![]);
    server
        .clone()
        .submit_workload(tarpc::context::current(), submission)
        .await
        .unwrap();
    let status = server
        .clone()
        .query_status(tarpc::context::current(), "status-test".to_string())
        .await;
    assert!(status.is_ok());
}

#[tokio::test]
async fn test_mock_executor_query_status_not_found() {
    let executor = arc_test_dispatch(TestWorkloadDouble::Mock);
    let server = ToadStoolTarpcServer::new("test", executor, None);
    let status = server
        .clone()
        .query_status(
            tarpc::context::current(),
            "nonexistent-workload".to_string(),
        )
        .await;
    assert!(status.is_err());
    assert!(status.unwrap_err().to_string().contains("not found"));
}

#[tokio::test]
async fn test_mock_executor_list_workloads() {
    let executor = arc_test_dispatch(TestWorkloadDouble::Mock);
    let server = ToadStoolTarpcServer::new("test", executor, None);
    let list = server
        .clone()
        .list_workloads(tarpc::context::current(), None)
        .await;
    assert!(list.is_ok());
    assert!(list.unwrap().is_empty());
}

#[tokio::test]
async fn test_mock_executor_health_check() {
    let executor = arc_test_dispatch(TestWorkloadDouble::Mock);
    let server = ToadStoolTarpcServer::new("test-v1", executor, None);
    let health = server.clone().health_check(tarpc::context::current()).await;
    assert!(health.is_ok());
    let h = health.unwrap();
    assert!(h.healthy);
    assert_eq!(h.details["version"].as_str().unwrap(), "test-v1");
    let ru = h.details["resource_utilization"].as_f64().unwrap();
    assert!(ru >= 0.0 && ru <= 1.0);
}

#[tokio::test]
async fn test_workload_result_serialization() {
    let result = WorkloadResult {
        workload_id: Arc::from("ser-1"),
        status: WorkloadStatus::Completed,
        data: Some(vec![1, 2, 3].into()),
        error: None,
        metrics: ExecutionMetrics {
            queued_duration_secs: 0.0,
            execution_duration_secs: 1.5,
            cpu_cores_used: 4,
            memory_used_bytes: 1024,
            gpu_memory_used_bytes: Some(4096),
        },
    };
    let json = serde_json::to_string(&result).unwrap();
    let restored: WorkloadResult = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.workload_id.as_ref(), result.workload_id.as_ref());
    assert!(matches!(restored.status, WorkloadStatus::Completed));
}

#[tokio::test]
async fn test_workload_submission_serialization() {
    let sub = mk_submission("sub-1", "gpu_compute", vec![0xff, 0xfe]);
    let json = serde_json::to_string(&sub).unwrap();
    let restored: WorkloadSubmission = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.workload_id.as_ref(), sub.workload_id.as_ref());
    assert_eq!(restored.workload_type.as_ref(), sub.workload_type.as_ref());
}

#[tokio::test]
async fn test_workload_status_variants() {
    let running = WorkloadStatus::Running;
    let completed = WorkloadStatus::Completed;
    let cancelled = WorkloadStatus::Cancelled;
    let _ = (running, completed, cancelled);
}

#[tokio::test]
async fn test_submit_workload_executor_error_increments_error_count() {
    let executor = arc_test_dispatch(TestWorkloadDouble::FailingUnit);
    let error_count = Arc::new(AtomicU64::new(0));
    let server = ToadStoolTarpcServer::new("v1", executor, Some(Arc::clone(&error_count)));

    let submission = mk_submission("fail-1", "cpu_compute", vec![]);
    let result = server
        .clone()
        .submit_workload(tarpc::context::current(), submission)
        .await;

    assert!(result.is_err());
    assert_eq!(error_count.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn test_query_capabilities_executor_error() {
    let executor = arc_test_dispatch(TestWorkloadDouble::FailingUnit);
    let server = ToadStoolTarpcServer::new("v1", executor, None);

    let result = server
        .clone()
        .query_capabilities(tarpc::context::current())
        .await;

    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("capabilities failed")
    );
}

#[tokio::test]
async fn test_query_capabilities_executor_error_increments_error_count() {
    let executor = arc_test_dispatch(TestWorkloadDouble::FailingUnit);
    let error_count = Arc::new(AtomicU64::new(0));
    let server = ToadStoolTarpcServer::new("v1", executor, Some(Arc::clone(&error_count)));

    let _ = server
        .clone()
        .query_capabilities(tarpc::context::current())
        .await;

    assert_eq!(error_count.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn test_query_status_not_found_increments_error_count() {
    let executor = arc_test_dispatch(TestWorkloadDouble::Mock);
    let error_count = Arc::new(AtomicU64::new(0));
    let server = ToadStoolTarpcServer::new("v1", executor, Some(Arc::clone(&error_count)));

    let result = server
        .clone()
        .query_status(tarpc::context::current(), "nonexistent-id".to_string())
        .await;

    assert!(result.is_err());
    assert_eq!(error_count.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn test_cancel_workload_success() {
    let executor = arc_test_dispatch(TestWorkloadDouble::Mock);
    let server = ToadStoolTarpcServer::new("v1", executor, None);

    let submission = mk_submission("cancel-me", "cpu_compute", vec![]);
    server
        .clone()
        .submit_workload(tarpc::context::current(), submission)
        .await
        .unwrap();

    let result = server
        .clone()
        .cancel_workload(tarpc::context::current(), "cancel-me".to_string())
        .await;
    assert!(result.is_ok());

    let status = server
        .clone()
        .query_status(tarpc::context::current(), "cancel-me".to_string())
        .await;
    assert!(status.is_ok());
    assert!(matches!(status.unwrap().status, WorkloadStatus::Cancelled));
}

#[tokio::test]
async fn test_list_workloads_with_filter() {
    let executor = arc_test_dispatch(TestWorkloadDouble::Mock);
    let server = ToadStoolTarpcServer::new("v1", executor, None);

    let filter = Some(std::collections::HashMap::from([(
        "status".to_string(),
        "running".to_string(),
    )]));
    let list = server
        .clone()
        .list_workloads(tarpc::context::current(), filter)
        .await;
    assert!(list.is_ok());
    assert!(list.unwrap().is_empty());
}

#[tokio::test]
async fn test_list_workloads_after_submit() {
    let executor = arc_test_dispatch(TestWorkloadDouble::Mock);
    let server = ToadStoolTarpcServer::new("v1", executor, None);

    let submission = mk_submission("list-test", "cpu_compute", vec![]);
    server
        .clone()
        .submit_workload(tarpc::context::current(), submission)
        .await
        .unwrap();

    let list = server
        .clone()
        .list_workloads(tarpc::context::current(), None)
        .await;
    assert!(list.is_ok());
    let workloads = list.unwrap();
    assert_eq!(workloads.len(), 1);
    assert_eq!(workloads[0].workload_id.as_ref(), "list-test");
}

#[tokio::test]
async fn test_health_check_with_active_workloads() {
    let executor = arc_test_dispatch(TestWorkloadDouble::QueuedUnit);
    let server = ToadStoolTarpcServer::new("v1", executor, None);

    let submission = mk_submission("queued-wl", "cpu_compute", vec![]);
    server
        .clone()
        .submit_workload(tarpc::context::current(), submission)
        .await
        .expect("submit ok");

    let health = server.clone().health_check(tarpc::context::current()).await;
    assert!(health.is_ok());
    let h = health.expect("health check succeeded");
    assert!(h.healthy);
    assert_eq!(h.details["active_workloads"].as_u64().unwrap(), 1);
    assert_eq!(h.details["queued_workloads"].as_u64().unwrap(), 1);
}

#[tokio::test]
async fn test_health_check_includes_error_count() {
    let executor = arc_test_dispatch(TestWorkloadDouble::FailingUnit);
    let error_count = Arc::new(AtomicU64::new(5));
    let server = ToadStoolTarpcServer::new("v1", executor, Some(Arc::clone(&error_count)));

    let _ = server
        .clone()
        .submit_workload(tarpc::context::current(), mk_submission("x", "cpu", vec![]))
        .await;

    let health = server.clone().health_check(tarpc::context::current()).await;
    assert!(health.is_ok());
    let h = health.unwrap();
    assert!(h.details["error_count"].as_u64().unwrap() >= 5);
}

#[tokio::test]
async fn test_standalone_executor_compute_units_has_tflops() {
    let exec = StandaloneExecutor::new();
    let caps = exec.query_capabilities().await.unwrap();
    assert!(!caps.compute_units.is_empty());
    let unit = &caps.compute_units[0];
    assert!(unit.tflops.is_some());
}

#[tokio::test]
async fn test_standalone_executor_execute_empty_data() {
    let exec = StandaloneExecutor::new();
    let submission = mk_submission("empty", "cpu_compute", vec![]);
    let result = exec.execute(submission).await;
    assert!(result.is_ok());
    let res = result.unwrap();
    assert!(res.data.is_some());
    assert!(res.data.unwrap().is_empty());
}

#[tokio::test]
async fn test_standalone_executor_execute_large_data_truncates() {
    let exec = StandaloneExecutor::new();
    let data = vec![0u8; 2048];
    let submission = mk_submission("large", "cpu_compute", data);
    let result = exec.execute(submission).await;
    assert!(result.is_ok());
    let res = result.unwrap();
    assert!(res.data.is_some());
    assert!(res.data.unwrap().len() <= 1024);
}

#[tokio::test]
async fn test_mock_executor_cancel_workload_not_tracked() {
    let executor = arc_test_dispatch(TestWorkloadDouble::Mock);
    let server = ToadStoolTarpcServer::new("v1", executor, None);

    let result = server
        .clone()
        .cancel_workload(tarpc::context::current(), "never-submitted".to_string())
        .await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_tarpc_server_version_reflected_in_health() {
    let executor = arc_test_dispatch(TestWorkloadDouble::Mock);
    let server = ToadStoolTarpcServer::new("2.3.4", executor, None);
    let health = server.health_check(tarpc::context::current()).await;
    assert!(health.is_ok());
    assert_eq!(health.unwrap().details["version"].as_str().unwrap(), "2.3.4");
}

#[tokio::test]
async fn test_health_status_serialization() {
    use crate::rpc_types::HealthStatus;
    let status = HealthStatus::ok()
        .with_uptime(3600)
        .with_detail("version", serde_json::Value::String("1.2.3".into()))
        .with_detail("resource_utilization", serde_json::json!(0.5))
        .with_detail("active_workloads", serde_json::json!(2))
        .with_detail("queued_workloads", serde_json::json!(1))
        .with_detail("error_count", serde_json::json!(0));
    let json = serde_json::to_string(&status).expect("serialize");
    let restored: HealthStatus = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(restored.healthy, status.healthy);
    assert_eq!(restored.details["version"], status.details["version"]);
    assert_eq!(restored.uptime_secs, status.uptime_secs);
    let restored_ru = restored.details["resource_utilization"].as_f64().unwrap();
    let status_ru = status.details["resource_utilization"].as_f64().unwrap();
    assert!((restored_ru - status_ru).abs() < 0.001);
}

#[tokio::test]
async fn test_compute_capabilities_serialization() {
    let caps = ComputeCapabilities {
        service_id: "test-svc".to_string(),
        compute_units: vec![ComputeUnit {
            id: "unit-1".to_string(),
            unit_type: "cpu".to_string(),
            name: "Test CPU".to_string(),
            cores: 8,
            memory_bytes: 16_000_000_000,
            tflops: Some(0.8),
            utilization: 0.25,
        }],
        supported_workload_types: vec!["cpu_compute".to_string(), "gpu_compute".to_string()],
        available_resources: AvailableResources {
            total_cpu_cores: 8,
            available_cpu_cores: 6,
            total_memory_bytes: 16_000_000_000,
            available_memory_bytes: 8_000_000_000,
            total_gpu_memory_bytes: Some(24_000_000_000),
            available_gpu_memory_bytes: Some(12_000_000_000),
            cpu_utilization: 25.0,
            memory_utilization: 50.0,
            gpu_utilization: Some(50.0),
        },
        metadata: std::collections::HashMap::new(),
    };
    let json = serde_json::to_string(&caps).expect("serialize");
    let restored: ComputeCapabilities = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(restored.service_id, caps.service_id);
    assert_eq!(restored.compute_units.len(), caps.compute_units.len());
    assert_eq!(restored.compute_units[0].tflops, Some(0.8));
}

#[tokio::test]
async fn test_cancel_workload_executor_error() {
    let executor = arc_test_dispatch(TestWorkloadDouble::CancelFailing);
    let server = ToadStoolTarpcServer::new("v1", executor, None);

    let submission = mk_submission("cancel-fail-test", "cpu_compute", vec![]);
    server
        .clone()
        .submit_workload(tarpc::context::current(), submission)
        .await
        .expect("submit ok");

    let result = server
        .clone()
        .cancel_workload(tarpc::context::current(), "cancel-fail-test".to_string())
        .await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("cancel failed"));
}

#[tokio::test]
async fn test_cancel_workload_executor_error_increments_error_count() {
    let executor = arc_test_dispatch(TestWorkloadDouble::CancelFailing);
    let error_count = Arc::new(AtomicU64::new(0));
    let server = ToadStoolTarpcServer::new("v1", executor, Some(Arc::clone(&error_count)));

    let submission = mk_submission("cancel-err-count", "cpu_compute", vec![]);
    server
        .clone()
        .submit_workload(tarpc::context::current(), submission)
        .await
        .expect("submit ok");

    let _ = server
        .clone()
        .cancel_workload(tarpc::context::current(), "cancel-err-count".to_string())
        .await;

    assert_eq!(error_count.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn test_standalone_executor_estimate_cpu_tflops_via_capabilities() {
    let exec = StandaloneExecutor::new();
    let caps = exec.query_capabilities().await.expect("capabilities");
    assert!(!caps.compute_units.is_empty());
    let unit = &caps.compute_units[0];
    let expected_tflops = f64::from(unit.cores) * 0.1;
    assert_eq!(unit.tflops, Some(expected_tflops));
}

// ───── serve_unix and serve_tcp ────────────────────────────────────────────

#[cfg(unix)]
#[tokio::test]
async fn test_serve_unix_binds_to_temp_socket() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let socket_path = temp_dir.path().join("tarpc-test.sock");
    let socket_path_for_spawn = socket_path.clone();

    let executor = arc_test_dispatch(TestWorkloadDouble::Mock);
    let server = ToadStoolTarpcServer::new("test-v1", executor, None);

    let server_handle = tokio::spawn(async move {
        let _ = server.serve_unix(&socket_path_for_spawn).await;
    });

    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(100);
    while !socket_path.exists() {
        if std::time::Instant::now() >= deadline {
            break;
        }
        tokio::task::yield_now().await;
    }
    assert!(socket_path.exists(), "socket file should exist after bind");
    server_handle.abort();
}

#[cfg(unix)]
#[tokio::test]
async fn test_serve_unix_fails_when_parent_is_file() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let file_path = temp_dir.path().join("not_a_dir");
    std::fs::File::create(&file_path).expect("create file");
    let socket_path = file_path.join("tarpc.sock");

    let executor = arc_test_dispatch(TestWorkloadDouble::Mock);
    let server = ToadStoolTarpcServer::new("test-v1", executor, None);

    let result = server.serve_unix(&socket_path).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_serve_tcp_binds_and_accepts() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let executor = arc_test_dispatch(TestWorkloadDouble::Mock);
    let server = ToadStoolTarpcServer::new("test-v1", executor, None);

    let server_handle = tokio::spawn(async move {
        let _ = server.serve_tcp(listener).await;
    });

    for _ in 0..20 {
        tokio::task::yield_now().await;
    }
    server_handle.abort();
}
