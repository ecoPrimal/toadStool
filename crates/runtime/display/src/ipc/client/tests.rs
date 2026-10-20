// SPDX-License-Identifier: AGPL-3.0-or-later
//! Tests for the display IPC client.

use super::{DisplayClient, IpcEndpoint, default_display_ipc_tcp_addr};
use crate::ipc::types::{JsonRpcRequest, JsonRpcRequestExt, JsonRpcResponse};
use crate::window::WindowId;
use std::path::PathBuf;

#[test]
fn test_jsonrpc_request_creation() {
    let req = JsonRpcRequest::new_auto("display.create_window", None);
    assert_eq!(req.jsonrpc, "2.0");
    assert_eq!(req.method, "display.create_window");
    assert!(req.id.is_some());
}

#[test]
fn test_ipc_endpoint_unix_variant() {
    use std::path::PathBuf;
    let ep = IpcEndpoint::UnixSocket(PathBuf::from("/tmp/display.sock"));
    let s = format!("{ep:?}");
    assert!(s.contains("UnixSocket") || s.contains("display"));
}

#[test]
fn test_ipc_endpoint_tcp_variant() {
    use std::net::SocketAddr;
    let addr: SocketAddr = default_display_ipc_tcp_addr().parse().unwrap();
    let ep = IpcEndpoint::TcpLocal(addr);
    let s = format!("{ep:?}");
    assert!(s.contains("TcpLocal") || s.contains("127"));
}

#[test]
fn test_jsonrpc_request_with_params() {
    let params = serde_json::json!({"width": 800, "height": 600});
    let req = JsonRpcRequest::new_auto("display.create_window", Some(params));
    assert_eq!(req.method, "display.create_window");
    assert!(req.params.is_some());
}

#[test]
fn test_jsonrpc_request_destroy_window_params() {
    let window_id = WindowId::new();
    let params = serde_json::json!({"window_id": window_id.as_string()});
    let req = JsonRpcRequest::new_auto("display.destroy_window", Some(params));
    assert_eq!(req.method, "display.destroy_window");
    assert_eq!(
        req.params.as_ref().unwrap()["window_id"],
        window_id.as_string()
    );
}

#[test]
fn test_jsonrpc_request_resize_window_params() {
    let window_id = WindowId::new();
    let params = serde_json::json!({
        "window_id": window_id.as_string(),
        "width": 1024,
        "height": 768
    });
    let req = JsonRpcRequest::new_auto("display.resize_window", Some(params));
    assert_eq!(req.method, "display.resize_window");
    let p = req.params.unwrap();
    assert_eq!(p["width"], 1024);
    assert_eq!(p["height"], 768);
}

#[test]
fn test_jsonrpc_request_serialization() {
    let req = JsonRpcRequest::new_auto("display.get_capabilities", None);
    let json = serde_json::to_string(&req).unwrap();
    assert!(json.contains("display.get_capabilities"));
    assert!(json.contains("2.0"));
    let parsed: JsonRpcRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.method, req.method);
}

#[test]
fn test_jsonrpc_response_parse_success() {
    let json = r#"{"jsonrpc":"2.0","result":{"window_id":"test-123"},"id":1}"#;
    let resp: JsonRpcResponse = serde_json::from_str(json).unwrap();
    assert!(resp.result.is_some());
    assert!(resp.error.is_none());
    assert_eq!(resp.result.unwrap()["window_id"], "test-123");
}

#[test]
fn test_jsonrpc_response_parse_error() {
    let json = r#"{"jsonrpc":"2.0","error":{"code":-32603,"message":"Internal error"},"id":1}"#;
    let resp: JsonRpcResponse = serde_json::from_str(json).unwrap();
    assert!(resp.result.is_none());
    assert!(resp.error.is_some());
    assert_eq!(resp.error.unwrap().code, -32603);
}

#[tokio::test]
async fn test_connect_nonexistent_path_fails() {
    let result = DisplayClient::connect("/nonexistent/socket/path/display.sock").await;
    assert!(result.is_err());
    if let Err(e) = result {
        assert!(
            e.to_string().to_lowercase().contains("connection")
                || e.to_string().to_lowercase().contains("failed")
                || e.to_string().to_lowercase().contains("error")
        );
    }
}

#[test]
fn test_endpoint_string_unix() {
    let (client_half, _server_half) = tokio::io::duplex(1024);
    let ep = IpcEndpoint::UnixSocket(PathBuf::from("/tmp/toadstool/display.sock"));
    let client = DisplayClient::new_for_test(client_half, ep);
    let s = client.endpoint_string();
    assert!(s.contains("toadstool") || s.contains("display") || s.contains("sock"));
}

#[test]
fn test_endpoint_string_tcp() {
    use std::net::SocketAddr;
    let (client_half, _server_half) = tokio::io::duplex(1024);
    let addr: SocketAddr = default_display_ipc_tcp_addr().parse().unwrap();
    let ep = IpcEndpoint::TcpLocal(addr);
    let client = DisplayClient::new_for_test(client_half, ep);
    let s = client.endpoint_string();
    assert!(s.contains("127.0.0.1"));
    assert!(s.contains("8091"));
}

#[test]
fn test_transport_name_unix() {
    let (client_half, _server_half) = tokio::io::duplex(1024);
    let ep = IpcEndpoint::UnixSocket(PathBuf::from("/tmp/test.sock"));
    let client = DisplayClient::new_for_test(client_half, ep);
    assert_eq!(client.transport_name(), "unix");
}

#[test]
fn test_transport_name_tcp() {
    use std::net::SocketAddr;
    let (client_half, _server_half) = tokio::io::duplex(1024);
    let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let ep = IpcEndpoint::TcpLocal(addr);
    let client = DisplayClient::new_for_test(client_half, ep);
    assert_eq!(client.transport_name(), "tcp");
}

#[test]
fn test_endpoint_accessor() {
    let (client_half, _server_half) = tokio::io::duplex(1024);
    let ep = IpcEndpoint::UnixSocket(PathBuf::from("/tmp/x.sock"));
    let client = DisplayClient::new_for_test(client_half, ep);
    assert!(matches!(client.endpoint(), IpcEndpoint::UnixSocket(_)));
}

/// Mock server: reads JSON-RPC request, writes success response
async fn mock_server_respond(
    mut server_half: tokio::io::DuplexStream,
    response: serde_json::Value,
) {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    let mut reader = BufReader::new(&mut server_half);
    let mut line = String::new();
    if reader.read_line(&mut line).await.is_ok() && !line.is_empty() {
        let req: std::result::Result<serde_json::Value, _> = serde_json::from_str(&line);
        let id = req
            .ok()
            .and_then(|r| r.get("id").cloned())
            .unwrap_or(serde_json::json!(null));
        let resp = serde_json::json!({
            "jsonrpc": "2.0",
            "result": response,
            "id": id
        });
        let resp_str = serde_json::to_string(&resp).unwrap();
        let _ = server_half.write_all(resp_str.as_bytes()).await;
        let _ = server_half.write_all(b"\n").await;
    }
}

#[tokio::test]
async fn test_create_window_via_mock_server() {
    let (client_half, server_half) = tokio::io::duplex(1024);
    let window_id = WindowId::new();
    let response = serde_json::json!({"window_id": window_id.as_string()});
    tokio::spawn(mock_server_respond(server_half, response));

    let ep = IpcEndpoint::UnixSocket(PathBuf::from("/tmp/test.sock"));
    let mut client = DisplayClient::new_for_test(client_half, ep);
    let result = client
        .create_window(crate::window::CreateWindowRequest::default())
        .await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), window_id);
}

#[tokio::test]
async fn test_get_capabilities_via_mock_server() {
    let (client_half, server_half) = tokio::io::duplex(1024);
    let response = serde_json::json!({
        "primal_id": "test-primal",
        "socket_path": "/tmp/display.sock",
        "max_windows": 8,
        "supported_formats": ["RGBA8888"],
        "has_gpu_acceleration": false,
        "vsync_available": true,
        "display_count": 1,
        "input_device_count": 0,
        "window_count": 0,
        "isomorphic": true
    });
    tokio::spawn(mock_server_respond(server_half, response));

    let ep = IpcEndpoint::UnixSocket(PathBuf::from("/tmp/test.sock"));
    let mut client = DisplayClient::new_for_test(client_half, ep);
    let result = client.get_capabilities().await;
    assert!(result.is_ok());
    let caps = result.unwrap();
    assert_eq!(caps.primal_id, "test-primal");
    assert_eq!(caps.max_windows, 8);
    assert!(caps.isomorphic);
}

#[tokio::test]
async fn test_destroy_window_via_mock_server() {
    let (client_half, server_half) = tokio::io::duplex(1024);
    tokio::spawn(mock_server_respond(
        server_half,
        serde_json::json!({"destroyed": true}),
    ));

    let ep = IpcEndpoint::UnixSocket(PathBuf::from("/tmp/test.sock"));
    let mut client = DisplayClient::new_for_test(client_half, ep);
    let window_id = WindowId::new();
    let result = client.destroy_window(window_id).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_resize_window_via_mock_server() {
    let (client_half, server_half) = tokio::io::duplex(1024);
    tokio::spawn(mock_server_respond(
        server_half,
        serde_json::json!({"resized": true}),
    ));

    let ep = IpcEndpoint::UnixSocket(PathBuf::from("/tmp/test.sock"));
    let mut client = DisplayClient::new_for_test(client_half, ep);
    let window_id = WindowId::new();
    let result = client.resize_window(window_id, 800, 600).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_create_window_server_error() {
    let (client_half, mut server_half) = tokio::io::duplex(1024);
    tokio::spawn(async move {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
        let mut reader = BufReader::new(&mut server_half);
        let mut line = String::new();
        if reader.read_line(&mut line).await.is_ok() {
            let resp = serde_json::json!({
                "jsonrpc": "2.0",
                "error": {"code": -32603, "message": "Internal error"},
                "id": serde_json::json!(null)
            });
            let resp_str = serde_json::to_string(&resp).unwrap();
            let _ = server_half.write_all(resp_str.as_bytes()).await;
            let _ = server_half.write_all(b"\n").await;
        }
    });

    let ep = IpcEndpoint::UnixSocket(PathBuf::from("/tmp/test.sock"));
    let mut client = DisplayClient::new_for_test(client_half, ep);
    let result = client
        .create_window(crate::window::CreateWindowRequest::default())
        .await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Server error"));
}

#[tokio::test]
async fn test_get_capabilities_invalid_response() {
    let (client_half, server_half) = tokio::io::duplex(1024);
    tokio::spawn(mock_server_respond(server_half, serde_json::json!(null)));

    let ep = IpcEndpoint::UnixSocket(PathBuf::from("/tmp/test.sock"));
    let mut client = DisplayClient::new_for_test(client_half, ep);
    let result = client.get_capabilities().await;
    assert!(result.is_err());
}
