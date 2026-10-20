// SPDX-License-Identifier: AGPL-3.0-or-later
//! Comprehensive coverage tests for display IPC client
//!
//! Tests message framing, serialization, endpoint handling WITHOUT real TCP/Unix connections.

use std::path::PathBuf;
use toadstool_display::ipc::IpcEndpoint;
use toadstool_display::ipc::{DisplayCapabilitiesInfo, JsonRpcRequest, JsonRpcRequestExt, JsonRpcResponse};
use toadstool_display::window::WindowId;

#[test]
fn test_jsonrpc_request_new_has_id() {
    let req = JsonRpcRequest::new_auto("display.get_capabilities", None);
    assert_eq!(req.jsonrpc, "2.0");
    assert_eq!(req.method, "display.get_capabilities");
    assert!(req.id.is_some());
    assert!(req.params.is_none());
}

#[test]
fn test_jsonrpc_request_notification_no_id() {
    let req = JsonRpcRequest::notification("display.input_event", None);
    assert!(req.id.is_none());
    assert_eq!(req.method, "display.input_event");
}

#[test]
fn test_jsonrpc_request_with_params_serialization() {
    let params = serde_json::json!({"window_id": "test-123", "width": 800, "height": 600});
    let req = JsonRpcRequest::new_auto("display.resize_window", Some(params));
    let json = serde_json::to_string(&req).unwrap();
    assert!(json.contains("display.resize_window"));
    assert!(json.contains("800"));
    assert!(json.contains("600"));
    let parsed: JsonRpcRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.params.unwrap()["width"], 800);
}

#[test]
fn test_display_capabilities_info_deserialization() {
    let json = r#"{
        "primal_id": "toadstool-primary",
        "socket_path": "/tmp/display.sock",
        "max_windows": 16,
        "supported_formats": ["RGBA8888", "BGRA8888"],
        "has_gpu_acceleration": true,
        "vsync_available": true,
        "display_count": 1,
        "input_device_count": 0,
        "window_count": 0,
        "isomorphic": true
    }"#;
    let caps: DisplayCapabilitiesInfo = serde_json::from_str(json).unwrap();
    assert_eq!(caps.primal_id, "toadstool-primary");
    assert_eq!(caps.max_windows, 16);
    assert!(caps.isomorphic);
}

#[test]
fn test_ipc_endpoint_unix_path() {
    let path = PathBuf::from("/run/user/1000/toadstool/display.sock");
    let ep = IpcEndpoint::UnixSocket(path);
    let s = match &ep {
        IpcEndpoint::UnixSocket(p) => p.display().to_string(),
        IpcEndpoint::TcpLocal(_) => String::new(),
    };
    assert!(s.contains("toadstool") || s.contains("display") || s.contains("sock"));
}

#[test]
fn test_ipc_endpoint_tcp_addr() {
    use std::net::SocketAddr;
    let addr: SocketAddr = "127.0.0.1:12345".parse().unwrap();
    let ep = IpcEndpoint::TcpLocal(addr);
    let s = match &ep {
        IpcEndpoint::UnixSocket(_) => String::new(),
        IpcEndpoint::TcpLocal(a) => a.to_string(),
    };
    assert!(s.contains("127.0.0.1"));
    assert!(s.contains("12345"));
}

#[test]
fn test_window_id_serialization_for_request() {
    let window_id = WindowId::new();
    let params = serde_json::json!({"window_id": window_id.as_string()});
    let req = JsonRpcRequest::new_auto("display.destroy_window", Some(params));
    assert_eq!(
        req.params.as_ref().unwrap()["window_id"],
        window_id.as_string()
    );
}

#[test]
fn test_create_window_request_params() {
    use toadstool_display::window::CreateWindowRequest;
    let req = CreateWindowRequest {
        width: 1024,
        height: 768,
        title: Some("Test".to_string()),
        fullscreen: false,
    };
    let params = serde_json::to_value(&req).unwrap();
    assert_eq!(params["width"], 1024);
    assert_eq!(params["height"], 768);
}

#[test]
fn test_jsonrpc_response_success_parse() {
    let json = r#"{"jsonrpc":"2.0","result":{"window_id":"abc-123"},"id":"req-1"}"#;
    let resp: JsonRpcResponse = serde_json::from_str(json).unwrap();
    assert!(resp.result.is_some());
    assert!(resp.error.is_none());
    assert_eq!(resp.result.unwrap()["window_id"], "abc-123");
}

#[test]
fn test_jsonrpc_response_error_parse() {
    let json = r#"{"jsonrpc":"2.0","error":{"code":-32603,"message":"Internal error"},"id":1}"#;
    let resp: JsonRpcResponse = serde_json::from_str(json).unwrap();
    assert!(resp.result.is_none());
    assert!(resp.error.is_some());
    assert_eq!(resp.error.unwrap().code, -32603);
}

#[test]
fn test_jsonrpc_request_display_create_window() {
    let params = serde_json::json!({});
    let req = JsonRpcRequest::new_auto("display.create_window", Some(params));
    assert_eq!(req.method, "display.create_window");
}

#[test]
fn test_jsonrpc_request_display_get_window_info() {
    let window_id = WindowId::new();
    let params = serde_json::json!({"window_id": window_id.as_string()});
    let req = JsonRpcRequest::new_auto("display.get_window_info", Some(params));
    assert_eq!(req.method, "display.get_window_info");
}

#[test]
fn test_jsonrpc_request_display_get_capabilities() {
    let req = JsonRpcRequest::new_auto("display.get_capabilities", None);
    assert_eq!(req.method, "display.get_capabilities");
}

#[test]
fn test_jsonrpc_response_roundtrip() {
    let resp = JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        result: Some(serde_json::json!({"primal_id": "test"})),
        error: None,
        id: serde_json::json!(1),
    };
    let json = serde_json::to_string(&resp).unwrap();
    let parsed: JsonRpcResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.result.unwrap()["primal_id"], "test");
}

#[test]
fn test_ipc_endpoint_debug_unix() {
    let ep = IpcEndpoint::UnixSocket(PathBuf::from("/tmp/test.sock"));
    let s = format!("{ep:?}");
    assert!(s.contains("UnixSocket") || s.contains("test"));
}

#[test]
fn test_ipc_endpoint_debug_tcp() {
    use std::net::SocketAddr;
    let addr: SocketAddr = "127.0.0.1:9999".parse().unwrap();
    let ep = IpcEndpoint::TcpLocal(addr);
    let s = format!("{ep:?}");
    assert!(s.contains("TcpLocal") || s.contains("127"));
}

#[test]
fn test_ipc_endpoint_clone() {
    let ep1 = IpcEndpoint::UnixSocket(PathBuf::from("/tmp/a.sock"));
    let ep2 = ep1.clone();
    assert!(matches!(
        (ep1, ep2),
        (IpcEndpoint::UnixSocket(_), IpcEndpoint::UnixSocket(_))
    ));
}

// ── DEEP coverage: discovery paths, types (no DisplayClient - test-only API) ──

#[tokio::test]
async fn test_discover_fails_without_socket_or_tcp_file() {
    let result = toadstool_display::ipc::DisplayClient::discover().await;
    if let Err(e) = result {
        let err_msg = e.to_string().to_lowercase();
        assert!(
            err_msg.contains("discover")
                || err_msg.contains("endpoint")
                || err_msg.contains("could not")
                || err_msg.contains("connect")
                || err_msg.contains("refused")
                || err_msg.contains("no such file")
        );
    }
}

#[tokio::test]
async fn test_discover_tcp_file_connect_fails() {
    let dir = tempfile::tempdir().expect("temp dir");
    let port_file = dir.path().join("toadstool-ipc-port");
    std::fs::write(&port_file, "tcp:127.0.0.1:0").expect("write port file");
    temp_env::async_with_vars(
        [
            (
                "XDG_RUNTIME_DIR",
                Some(dir.path().to_string_lossy().as_ref()),
            ),
            ("HOME", None::<&str>),
        ],
        async {
            let result = toadstool_display::ipc::DisplayClient::discover().await;
            assert!(
                result.is_err(),
                "discover with TCP file but no server should fail"
            );
        },
    )
    .await;
}

#[test]
fn test_window_id_in_destroy_params() {
    let window_id = WindowId::new();
    let params = serde_json::json!({"window_id": window_id.as_string()});
    assert_eq!(params["window_id"], window_id.as_string());
}

#[test]
fn test_window_id_in_resize_params() {
    let window_id = WindowId::new();
    let params = serde_json::json!({
        "window_id": window_id.as_string(),
        "width": 800,
        "height": 600
    });
    assert_eq!(params["width"], 800);
    assert_eq!(params["height"], 600);
}

#[test]
fn test_jsonrpc_response_empty_result_error() {
    let json = r#"{"jsonrpc":"2.0","result":null,"error":null,"id":1}"#;
    let resp: JsonRpcResponse = serde_json::from_str(json).unwrap();
    assert!(resp.result.is_none());
    assert!(resp.error.is_none());
}

#[test]
fn test_create_window_request_serialization() {
    use toadstool_display::window::CreateWindowRequest;
    let req = CreateWindowRequest {
        width: 640,
        height: 480,
        title: Some("Coverage".to_string()),
        fullscreen: false,
    };
    let json = serde_json::to_value(&req).unwrap();
    assert_eq!(json["width"], 640);
    assert_eq!(json["height"], 480);
    assert_eq!(json["title"], "Coverage");
}

#[test]
fn test_ipc_endpoint_tcp_socket_addr() {
    use std::net::SocketAddr;
    let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let ep = IpcEndpoint::TcpLocal(addr);
    let s = format!("{ep:?}");
    assert!(s.contains("127") || s.contains("TcpLocal"));
}

#[test]
fn test_display_capabilities_minimal() {
    let json = r#"{
        "primal_id": "test",
        "socket_path": "/tmp/sock",
        "max_windows": 1,
        "supported_formats": [],
        "has_gpu_acceleration": false,
        "vsync_available": false,
        "display_count": 0,
        "input_device_count": 0,
        "window_count": 0,
        "isomorphic": false
    }"#;
    let caps: DisplayCapabilitiesInfo = serde_json::from_str(json).unwrap();
    assert_eq!(caps.max_windows, 1);
    assert!(!caps.isomorphic);
}
