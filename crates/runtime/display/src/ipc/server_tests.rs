// SPDX-License-Identifier: AGPL-3.0-or-later

use super::*;
use crate::ipc::{JsonRpcError, JsonRpcErrorExt, JsonRpcRequest, JsonRpcResponse, JsonRpcResponseExt};
use std::net::{Ipv4Addr, SocketAddr};

#[tokio::test]
async fn test_jsonrpc_parsing() {
    let request_str = r#"{"jsonrpc":"2.0","method":"display.get_capabilities","id":1}"#;
    let request: JsonRpcRequest = serde_json::from_str(request_str).unwrap();
    assert_eq!(request.method, "display.get_capabilities");
}

/// Create an owned manager for `DisplayServer::new` tests.
async fn test_manager_owned() -> Option<WindowManager> {
    WindowManager::new().await.ok()
}

#[test]
fn test_ipc_transport_debug() {
    let t = IpcTransport::UnixSocket;
    let s = format!("{t:?}");
    assert!(s.contains("Unix"));
}

#[test]
fn test_ipc_transport_tcp_fallback() {
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, 0));
    let t = IpcTransport::TcpFallback(addr);
    let s = format!("{t:?}");
    assert!(s.contains("TcpFallback") || s.contains("127"));
}

#[tokio::test]
async fn test_display_server_new_socket_path() {
    let Some(manager) = test_manager_owned().await else {
        return;
    };
    let server = DisplayServer::new(manager);
    let path = server.socket_path();
    assert!(path.to_string_lossy().contains("toadstool"));
    assert!(path.to_string_lossy().ends_with("display.sock"));
}

#[test]
fn test_jsonrpc_error_constructors() {
    let parse = JsonRpcError::parse_error();
    assert_eq!(parse.code, -32700);
    assert!(parse.message.to_lowercase().contains("parse"));

    let invalid = JsonRpcError::invalid_request();
    assert_eq!(invalid.code, -32600);

    let not_found = JsonRpcError::method_not_found("foo.bar");
    assert_eq!(not_found.code, -32601);
    assert!(not_found.message.contains("foo.bar"));

    let invalid_params = JsonRpcError::invalid_params("bad");
    assert_eq!(invalid_params.code, -32602);
    assert!(invalid_params.message.contains("bad"));

    let internal = JsonRpcError::internal_error("oops");
    assert_eq!(internal.code, -32603);
    assert!(internal.message.contains("oops"));
}

#[test]
fn test_jsonrpc_response_success_roundtrip() {
    let resp = JsonRpcResponse::success(
        serde_json::json!(1),
        serde_json::json!({"window_id": "test-123"}),
    );
    let json = serde_json::to_string(&resp).unwrap();
    let parsed: JsonRpcResponse = serde_json::from_str(&json).unwrap();
    assert!(parsed.result.is_some());
    assert_eq!(parsed.result.unwrap()["window_id"], "test-123");
}

#[tokio::test]
async fn test_transport_initially_none() {
    let Some(manager) = test_manager_owned().await else {
        return;
    };
    let server = DisplayServer::new(manager);
    let transport = server.transport().await;
    assert!(transport.is_none());
}

#[test]
fn test_ipc_transport_clone() {
    let t = IpcTransport::UnixSocket;
    let t2 = t.clone();
    assert!(matches!(
        (t, t2),
        (IpcTransport::UnixSocket, IpcTransport::UnixSocket)
    ));

    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, 0));
    let t3 = IpcTransport::TcpFallback(addr);
    let t4 = t3.clone();
    assert!(matches!(
        (t3, t4),
        (IpcTransport::TcpFallback(_), IpcTransport::TcpFallback(_))
    ));
}

#[test]
fn test_ipc_transport_tcp_fallback_addr() {
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, 12_345));
    let t = IpcTransport::TcpFallback(addr);
    let s = format!("{t:?}");
    assert!(s.contains("12345") || s.contains("127"));
}

#[test]
fn test_jsonrpc_request_parse_valid() {
    let req = r#"{"jsonrpc":"2.0","method":"display.get_capabilities","id":1}"#;
    let parsed: JsonRpcRequest = serde_json::from_str(req).unwrap();
    assert_eq!(parsed.method, "display.get_capabilities");
    assert_eq!(parsed.id, Some(serde_json::json!(1)));
}

#[test]
fn test_jsonrpc_response_error_roundtrip() {
    let err = JsonRpcError::internal_error("test");
    let resp = JsonRpcResponse::from_error(serde_json::json!(1), err);
    let json = serde_json::to_string(&resp).unwrap();
    let parsed: JsonRpcResponse = serde_json::from_str(&json).unwrap();
    assert!(parsed.error.is_some());
    assert!(parsed.result.is_none());
}

#[test]
fn test_jsonrpc_error_method_not_found() {
    let err = JsonRpcError::method_not_found("display.unknown");
    assert_eq!(err.code, -32601);
    assert!(err.message.contains("display.unknown"));
}

#[test]
fn test_jsonrpc_error_internal_error() {
    let err = JsonRpcError::internal_error("oops");
    assert_eq!(err.code, -32603);
    assert!(err.message.contains("oops"));
}

#[tokio::test]
async fn test_display_server_socket_path_contains_toadstool() {
    let Some(manager) = test_manager_owned().await else {
        return;
    };
    let server = DisplayServer::new(manager);
    let path_str = server.socket_path().to_string_lossy();
    assert!(path_str.contains("toadstool") || path_str.contains("display"));
}

#[tokio::test]
async fn test_handle_request_with_empty_line() {
    use crate::input::InputManager;
    use crate::ipc::dispatch;
    use std::sync::RwLock;

    let Some(manager) = test_manager_owned().await else {
        return;
    };
    let manager = Arc::new(RwLock::new(manager));
    let input = Arc::new(RwLock::new(InputManager::empty()));
    let response = dispatch::handle_request("", &manager, &input).await;
    assert!(response.error.is_some() || response.result.is_none());
}

#[tokio::test]
async fn test_handle_request_invalid_json() {
    use crate::input::InputManager;
    use crate::ipc::dispatch;
    use std::sync::RwLock;

    let Some(manager) = test_manager_owned().await else {
        return;
    };
    let manager = Arc::new(RwLock::new(manager));
    let input = Arc::new(RwLock::new(InputManager::empty()));
    let response = dispatch::handle_request("not valid json", &manager, &input).await;
    assert!(response.error.is_some());
}
