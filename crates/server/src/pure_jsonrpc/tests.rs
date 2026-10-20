// SPDX-License-Identifier: AGPL-3.0-or-later
//! Tests for the JSON-RPC handler and types

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use std::borrow::Cow;

use super::handler::JsonRpcHandler;
use super::types::{JsonRpcError, JsonRpcErrorExt, JsonRpcResponse, JsonWorkloadSubmission, ZeroCopyJsonRpcRequest};
use crate::rpc_types::{ResourceRequirements, WorkloadPriority};

fn test_handler() -> JsonRpcHandler {
    let executor = Arc::new(crate::tarpc_server::WorkloadExecutorDispatch::Standalone(
        crate::tarpc_server::StandaloneExecutor::new(),
    ));
    JsonRpcHandler::new(
        executor,
        "test-1.0.0".to_string(),
        None,
        Arc::new(AtomicBool::new(true)),
        None,
    )
}

fn mk_request(method: &str, params: Option<serde_json::Value>, id: i32) -> ZeroCopyJsonRpcRequest<'static> {
    ZeroCopyJsonRpcRequest {
        jsonrpc: "2.0".into(),
        method: Cow::Owned(method.to_string()),
        params,
        id: Some(serde_json::json!(id)),
    }
}

#[test]
fn test_parse_request() {
    let json = r#"{
        "jsonrpc": "2.0",
        "method": "toadstool.health",
        "id": 1
    }"#;

    let req: ZeroCopyJsonRpcRequest<'_> = serde_json::from_str(json).expect("Parse failed");
    assert_eq!(req.jsonrpc.as_ref(), "2.0");
    assert_eq!(req.method.as_ref(), "toadstool.health");
}

#[test]
fn test_error_response() {
    let err = JsonRpcError::method_not_found("foo.bar");
    assert_eq!(err.code, -32601);
    assert!(err.message.contains("foo.bar"));
}

#[test]
fn test_json_workload_submission() {
    use base64::{Engine as _, engine::general_purpose::STANDARD};

    let data = vec![1, 2, 3, 4];
    let encoded = STANDARD.encode(&data);

    let submission = JsonWorkloadSubmission {
        workload_id: Arc::from("work-123"),
        workload_type: Arc::from("gpu_compute"),
        data: encoded,
        metadata: HashMap::new(),
        priority: WorkloadPriority::Normal,
        requirements: ResourceRequirements {
            cpu_cores: Some(4),
            memory_bytes: Some(1024 * 1024 * 1024),
            gpu_memory_bytes: None,
            timeout_secs: Some(300),
        },
    };

    let tarpc = submission.into_tarpc().expect("Conversion failed");
    assert_eq!(tarpc.data, vec![1, 2, 3, 4]);
}

#[tokio::test]
async fn test_health_via_handle_request() {
    let handler = test_handler();
    let request = mk_request("toadstool.health", None, 1);
    let response = handler.handle_request(&request).await;

    assert!(response.error.is_none());
    let result = response.result.expect("result present");
    assert!(result["healthy"].as_bool().unwrap());
    assert_eq!(result["details"]["version"], "test-1.0.0");
}

#[tokio::test]
async fn test_handle_method_dispatch_version() {
    let handler = test_handler();
    let request = mk_request("toadstool.version", None, 2);
    let response = handler.handle_request(&request).await;

    assert!(response.error.is_none());
    let result = response.result.expect("result present");
    assert_eq!(result["version"], "test-1.0.0");
    assert_eq!(result["protocol"], "JSON-RPC 2.0");
}

#[tokio::test]
async fn test_handle_method_dispatch_unknown() {
    let handler = test_handler();
    let request = mk_request("unknown.method", None, 99);
    let response = handler.handle_request(&request).await;

    assert!(response.result.is_none());
    let err = response.error.expect("error present");
    assert_eq!(err.code, JsonRpcError::METHOD_NOT_FOUND);
}

#[tokio::test]
async fn test_invalid_jsonrpc_version() {
    let handler = test_handler();
    let request = ZeroCopyJsonRpcRequest {
        jsonrpc: "3.0".into(),
        method: Cow::Borrowed("toadstool.health"),
        params: None,
        id: Some(serde_json::json!(1)),
    };
    let response = handler.handle_request(&request).await;

    assert!(response.result.is_none());
    let err = response.error.expect("error present");
    assert_eq!(err.code, JsonRpcError::INVALID_REQUEST);
}

#[tokio::test]
async fn test_compute_submit() {
    let handler = test_handler();
    let params = serde_json::json!({
        "inference": {
            "model": "tinyllama",
            "prompt": "Hello",
            "params": {}
        }
    });
    let request = mk_request("compute.submit", Some(params), 1);
    let response = handler.handle_request(&request).await;

    assert!(response.error.is_none());
    let result = response.result.expect("result present");
    assert!(result["job_id"].as_str().is_some());
}

#[tokio::test]
async fn test_compute_list() {
    let handler = test_handler();
    let request = mk_request("compute.list", None, 1);
    let response = handler.handle_request(&request).await;

    assert!(response.error.is_none());
    let result = response.result.expect("result present");
    assert!(result["jobs"].is_array());
    assert!(result["counts"].is_object());
}

#[tokio::test]
async fn test_compute_status_missing_job_id() {
    let handler = test_handler();
    let request = mk_request("compute.status", Some(serde_json::json!({})), 1);
    let response = handler.handle_request(&request).await;

    assert!(response.result.is_none());
    let err = response.error.expect("error present");
    assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
}

#[tokio::test]
async fn test_query_capabilities() {
    let handler = test_handler();
    let request = mk_request("toadstool.query_capabilities", None, 1);
    let response = handler.handle_request(&request).await;

    assert!(response.error.is_none());
    let result = response.result.expect("result present");
    assert!(result["service_id"].as_str().is_some());
    assert!(result["compute_units"].is_array());
}

#[test]
fn test_jsonrpc_error_constructors() {
    let err = JsonRpcError::parse_error("bad json");
    assert_eq!(err.code, JsonRpcError::PARSE_ERROR);
    assert!(err.message.contains("bad json"));

    let err = JsonRpcError::invalid_request("wrong version");
    assert_eq!(err.code, JsonRpcError::INVALID_REQUEST);

    let err = JsonRpcError::invalid_params("missing field");
    assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);

    let err = JsonRpcError::internal_error("panic");
    assert_eq!(err.code, JsonRpcError::INTERNAL_ERROR);
}

#[test]
fn test_json_workload_submission_invalid_base64() {
    let submission = JsonWorkloadSubmission {
        workload_id: Arc::from("work-1"),
        workload_type: Arc::from("gpu_compute"),
        data: "!!!not-valid-base64!!!".to_string(),
        metadata: HashMap::new(),
        priority: WorkloadPriority::Normal,
        requirements: ResourceRequirements {
            cpu_cores: Some(4),
            memory_bytes: Some(1024 * 1024 * 1024),
            gpu_memory_bytes: None,
            timeout_secs: Some(300),
        },
    };
    let result = submission.into_tarpc();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Invalid base64"));
}

#[tokio::test]
async fn test_submit_workload_missing_params() {
    let handler = test_handler();
    let request = mk_request("toadstool.submit_workload", None, 1);
    let response = handler.handle_request(&request).await;

    assert!(response.result.is_none());
    let err = response.error.expect("error present");
    assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
}

#[tokio::test]
async fn test_submit_workload_success() {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let handler = test_handler();
    let params = serde_json::json!({
        "workload_id": "work-submit-1",
        "workload_type": "cpu_compute",
        "data": STANDARD.encode([1u8, 2, 3, 4]),
        "metadata": {},
        "priority": "Normal",
        "requirements": {
            "cpu_cores": 2,
            "memory_bytes": 1024,
            "timeout_secs": 60
        }
    });
    let request = mk_request("toadstool.submit_workload", Some(params), 1);
    let response = handler.handle_request(&request).await;

    assert!(response.error.is_none());
    let result = response.result.expect("result present");
    assert_eq!(result["workload_id"], "work-submit-1");
    assert!(result["status"].as_str().is_some());
}

#[tokio::test]
async fn test_submit_workload_invalid_base64() {
    let handler = test_handler();
    let params = serde_json::json!({
        "workload_id": "work-1",
        "workload_type": "cpu_compute",
        "data": "!!!invalid!!!",
        "metadata": {},
        "priority": "Normal",
        "requirements": {}
    });
    let request = mk_request("toadstool.submit_workload", Some(params), 1);
    let response = handler.handle_request(&request).await;

    assert!(response.result.is_none());
    let err = response.error.expect("error present");
    assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
}

#[tokio::test]
async fn test_query_status_invalid_uuid() {
    let handler = test_handler();
    let params = serde_json::json!("not-a-uuid");
    let request = mk_request("toadstool.query_status", Some(params), 1);
    let response = handler.handle_request(&request).await;

    assert!(response.result.is_none());
    let err = response.error.expect("error present");
    assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
}

#[tokio::test]
async fn test_query_status_job_not_found() {
    let handler = test_handler();
    let job_id = uuid::Uuid::new_v4();
    let params = serde_json::json!(job_id.to_string());
    let request = mk_request("toadstool.query_status", Some(params), 1);
    let response = handler.handle_request(&request).await;

    assert!(response.result.is_none());
    let err = response.error.expect("error present");
    assert_eq!(err.code, JsonRpcError::INTERNAL_ERROR);
}

#[tokio::test]
async fn test_cancel_workload_missing_params() {
    let handler = test_handler();
    let request = mk_request("toadstool.cancel_workload", None, 1);
    let response = handler.handle_request(&request).await;

    assert!(response.result.is_none());
    let err = response.error.expect("error present");
    assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
}

#[tokio::test]
async fn test_cancel_workload_success() {
    let handler = test_handler();
    let params = serde_json::json!("some-workload-id");
    let request = mk_request("toadstool.cancel_workload", Some(params), 1);
    let response = handler.handle_request(&request).await;

    assert!(response.error.is_none());
    let result = response.result.expect("result present");
    assert_eq!(result["success"], true);
}

#[tokio::test]
async fn test_compute_result_missing_job_id() {
    let handler = test_handler();
    let request = mk_request("compute.result", Some(serde_json::json!({})), 1);
    let response = handler.handle_request(&request).await;

    assert!(response.result.is_none());
    let err = response.error.expect("error present");
    assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
}

#[tokio::test]
async fn test_compute_result_job_not_found() {
    let handler = test_handler();
    let job_id = uuid::Uuid::new_v4();
    let params = serde_json::json!({ "job_id": job_id.to_string() });
    let request = mk_request("compute.result", Some(params), 1);
    let response = handler.handle_request(&request).await;

    assert!(response.result.is_none());
    let err = response.error.expect("error present");
    assert_eq!(
        err.code,
        toadstool_common::constants::jsonrpc::error_codes::WORKLOAD_NOT_FOUND
    );
}

#[tokio::test]
async fn test_compute_cancel_missing_job_id() {
    let handler = test_handler();
    let request = mk_request("compute.cancel", Some(serde_json::json!({})), 1);
    let response = handler.handle_request(&request).await;

    assert!(response.result.is_none());
    let err = response.error.expect("error present");
    assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
}

#[tokio::test]
async fn test_compute_list_with_state_filter() {
    let handler = test_handler();
    let params = serde_json::json!({ "state": "pending" });
    let request = mk_request("compute.list", Some(params), 1);
    let response = handler.handle_request(&request).await;

    assert!(response.error.is_none());
    let result = response.result.expect("result present");
    assert!(result["jobs"].is_array());
    assert!(result["counts"].is_object());
}

#[tokio::test]
async fn test_health_error_count_incremented() {
    let error_count = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let executor = Arc::new(crate::tarpc_server::WorkloadExecutorDispatch::Standalone(
        crate::tarpc_server::StandaloneExecutor::new(),
    ));
    let handler = JsonRpcHandler::new(
        executor,
        "1.0".to_string(),
        Some(error_count),
        Arc::new(AtomicBool::new(true)),
        None,
    );

    let bad_request = mk_request("unknown.method", None, 1);
    let _ = handler.handle_request(&bad_request).await;

    let health_request = mk_request("toadstool.health", None, 2);
    let response = handler.handle_request(&health_request).await;
    assert!(response.error.is_none());
    let result = response.result.expect("result present");
    assert!(result["details"]["error_count"].as_u64().unwrap_or(0) >= 1);
}

#[tokio::test]
async fn test_request_id_null_when_missing() {
    let handler = test_handler();
    let request = ZeroCopyJsonRpcRequest {
        jsonrpc: "2.0".into(),
        method: Cow::Borrowed("toadstool.health"),
        params: None,
        id: None,
    };
    let response = handler.handle_request(&request).await;
    assert_eq!(response.id, serde_json::Value::Null);
}

#[tokio::test]
async fn test_compute_submit_invalid_params() {
    let handler = test_handler();
    let params = serde_json::json!({ "invalid": "job_type" });
    let request = mk_request("compute.submit", Some(params), 1);
    let response = handler.handle_request(&request).await;

    assert!(response.result.is_none());
    let err = response.error.expect("error present");
    assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
}

#[tokio::test]
async fn test_submit_workload_invalid_params_structure() {
    let handler = test_handler();
    let params = serde_json::json!({
        "workload_type": "cpu_compute",
        "data": ""
    });
    let request = mk_request("toadstool.submit_workload", Some(params), 1);
    let response = handler.handle_request(&request).await;

    assert!(response.result.is_none());
    let err = response.error.expect("error present");
    assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
}

#[tokio::test]
async fn test_compute_cancel_success() {
    let handler = test_handler();
    let params = serde_json::json!({
        "inference": { "model": "test", "prompt": "x", "params": {} }
    });
    let submit_req = mk_request("compute.submit", Some(params.clone()), 1);
    let submit_resp = handler.handle_request(&submit_req).await;
    let job_id = submit_resp
        .result
        .as_ref()
        .and_then(|r| r.get("job_id"))
        .and_then(|v| v.as_str())
        .expect("submit should return job_id");

    let cancel_params = serde_json::json!({ "job_id": job_id });
    let cancel_req = mk_request("compute.cancel", Some(cancel_params), 2);
    let cancel_resp = handler.handle_request(&cancel_req).await;

    assert!(cancel_resp.error.is_none());
    let result = cancel_resp.result.expect("result present");
    assert_eq!(result["cancelled"], true);
}

#[test]
fn test_jsonrpc_response_serialization() {
    let success = JsonRpcResponse {
        jsonrpc: "2.0".into(),
        result: Some(serde_json::json!({"ok": true})),
        error: None,
        id: serde_json::json!(1),
    };
    let json = serde_json::to_string(&success).expect("Serialize failed");
    assert!(json.contains("\"result\""));
    assert!(json.contains("\"ok\""));
    assert!(!json.contains("\"error\""));

    let failure = JsonRpcResponse {
        jsonrpc: "2.0".into(),
        result: None,
        error: Some(JsonRpcError::method_not_found("foo")),
        id: serde_json::json!(2),
    };
    let json_err = serde_json::to_string(&failure).expect("Serialize failed");
    assert!(json_err.contains("\"error\""));
}

#[test]
fn test_jsonrpc_request_with_params_array() {
    let json = r#"{"jsonrpc":"2.0","method":"foo","params":[1,2],"id":1}"#;
    let req: ZeroCopyJsonRpcRequest<'_> = serde_json::from_str(json).expect("Parse failed");
    assert!(req.params.is_some());
}

#[test]
fn test_jsonrpc_request_with_params_object() {
    let json = r#"{"jsonrpc":"2.0","method":"foo","params":{"x":1},"id":"req-1"}"#;
    let req: ZeroCopyJsonRpcRequest<'_> = serde_json::from_str(json).expect("Parse failed");
    assert!(req.params.is_some());
    assert_eq!(req.method.as_ref(), "foo");
}

#[test]
fn test_jsonrpc_request_default_params() {
    let json = r#"{"jsonrpc":"2.0","method":"bar","id":null}"#;
    let req: ZeroCopyJsonRpcRequest<'_> = serde_json::from_str(json).expect("Parse failed");
    assert!(req.params.is_none());
}

#[test]
fn test_jsonrpc_error_serialization_roundtrip() {
    let err = JsonRpcError::invalid_params("missing field");
    let json = serde_json::to_string(&err).unwrap();
    let restored: JsonRpcError = serde_json::from_str(&json).unwrap();
    assert_eq!(err.code, restored.code);
    assert_eq!(err.message, restored.message);
}

#[test]
fn test_json_workload_submission_serialization_roundtrip() {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let sub = JsonWorkloadSubmission {
        workload_id: Arc::from("w-1"),
        workload_type: Arc::from("cpu_compute"),
        data: STANDARD.encode([1u8, 2, 3]),
        metadata: [("k".to_string(), "v".to_string())].into(),
        priority: WorkloadPriority::High,
        requirements: ResourceRequirements {
            cpu_cores: Some(4),
            memory_bytes: Some(1024),
            gpu_memory_bytes: None,
            timeout_secs: Some(60),
        },
    };
    let json = serde_json::to_string(&sub).unwrap();
    let restored: JsonWorkloadSubmission = serde_json::from_str(&json).unwrap();
    assert_eq!(sub.workload_id.as_ref(), restored.workload_id.as_ref());
    assert_eq!(sub.data, restored.data);
}

/// Semantic method names registered in SemanticMethodRegistry must route correctly.
#[tokio::test]
async fn test_semantic_method_dispatch_runtime_workload_submit() {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let handler = test_handler();
    let params = serde_json::json!({
        "workload_id": "sem-1",
        "workload_type": "cpu_compute",
        "data": STANDARD.encode([1u8]),
        "metadata": {},
        "priority": "Normal",
        "requirements": { "cpu_cores": 1, "memory_bytes": 512 }
    });
    // "runtime.workload.submit" is a semantic alias for submit_workload
    let request = mk_request("runtime.workload.submit", Some(params), 1);
    let response = handler.handle_request(&request).await;

    assert!(
        response.error.is_none(),
        "semantic alias should route correctly"
    );
    let result = response.result.expect("result present");
    assert_eq!(result["workload_id"], "sem-1");
}

/// Semantic method names for cancel should also route correctly.
#[tokio::test]
async fn test_semantic_method_dispatch_cancel() {
    let handler = test_handler();
    let params = serde_json::json!("any-workload-id");
    let request = mk_request("compute.cancel.workload", Some(params), 1);
    let response = handler.handle_request(&request).await;
    // "compute.cancel.workload" is not yet registered; should get METHOD_NOT_FOUND
    if let Some(err) = response.error {
        assert_eq!(err.code, JsonRpcError::METHOD_NOT_FOUND);
    }
}

#[tokio::test]
async fn test_gpu_info_handler() {
    let handler = test_handler();
    let request = mk_request("gpu.query_info", None, 1);
    let response = handler.handle_request(&request).await;
    assert!(response.error.is_none());
    let result = response.result.expect("result present");
    assert!(result.get("devices").is_some());
    assert!(result.get("driver").is_some());
}

#[tokio::test]
async fn test_gpu_memory_handler() {
    let handler = test_handler();
    let request = mk_request("gpu.query_memory", None, 1);
    let response = handler.handle_request(&request).await;
    assert!(response.error.is_none());
    let result = response.result.expect("result present");
    assert!(result.get("devices").is_some());
}

#[tokio::test]
async fn test_gate_list() {
    let handler = test_handler();
    let request = mk_request("gate.list", None, 1);
    let response = handler.handle_request(&request).await;
    assert!(response.error.is_none());
    let result = response.result.expect("result present");
    assert!(result.get("gates").is_some());
    assert!(result["gates"].is_array());
}

#[tokio::test]
async fn test_resources_estimate() {
    let handler = test_handler();
    let request = mk_request("toadstool.resources.estimate", None, 1);
    let response = handler.handle_request(&request).await;
    assert!(response.result.is_none());
    let err = response.error.expect("error present");
    assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
}

#[tokio::test]
async fn test_compute_health() {
    let handler = test_handler();
    let request = mk_request("compute.health", None, 1);
    let response = handler.handle_request(&request).await;
    assert!(response.error.is_none());
    let result = response.result.expect("result present");
    assert!(result["healthy"].as_bool().unwrap());
    assert!(result["uptime_secs"].as_u64().is_some());
}

#[tokio::test]
async fn test_compute_version() {
    let handler = test_handler();
    let request = mk_request("compute.version", None, 1);
    let response = handler.handle_request(&request).await;
    assert!(response.error.is_none());
    let result = response.result.expect("result present");
    assert!(result["version"].as_str().is_some());
    assert_eq!(result["protocol"], "JSON-RPC 2.0");
}

#[tokio::test]
async fn test_compute_capabilities() {
    let handler = test_handler();
    let request = mk_request("compute.capabilities", None, 1);
    let response = handler.handle_request(&request).await;
    assert!(response.error.is_none());
    let result = response.result.expect("result present");
    assert!(result["service_id"].as_str().is_some());
    assert!(result["compute_units"].is_array());
}
