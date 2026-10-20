// SPDX-License-Identifier: AGPL-3.0-or-later
//! JSON-RPC request dispatch for display operations
//!
//! Handles parsing, routing, and execution of display.* JSON-RPC methods.

use base64::Engine;
use serde::Deserialize;

use super::platform;
use super::types::{JsonRpcError, JsonRpcErrorExt, JsonRpcRequest, JsonRpcResponse, JsonRpcResponseExt};
use crate::input::InputManager;
use crate::window::{CreateWindowRequest, Size, WindowId, WindowManager};
use crate::{DisplayError, Result};
use std::sync::Arc;
use std::sync::RwLock;
use toadstool_common::constants::PRIMAL_NAME;

/// Handle a single JSON-RPC request
///
/// Parses the request, dispatches to the appropriate handler, and returns
/// a JSON-RPC response (success or error).
pub async fn handle_request(
    request_str: &str,
    manager: &Arc<RwLock<WindowManager>>,
    input: &Arc<RwLock<InputManager>>,
) -> JsonRpcResponse {
    let request: JsonRpcRequest = match serde_json::from_slice(request_str.as_bytes()) {
        Ok(req) => req,
        Err(_) => {
            return JsonRpcResponse::from_error(serde_json::json!(null), JsonRpcError::parse_error());
        }
    };

    let id = request.id.clone().unwrap_or(serde_json::json!(null));

    let result = dispatch_method(&request, manager, input).await;

    match result {
        Ok(value) => JsonRpcResponse::success(id, value),
        Err(e) => JsonRpcResponse::from_error(id, JsonRpcError::internal_error(&e.to_string())),
    }
}

/// Dispatch method to handler
async fn dispatch_method(
    request: &JsonRpcRequest,
    manager: &Arc<RwLock<WindowManager>>,
    input: &Arc<RwLock<InputManager>>,
) -> Result<serde_json::Value> {
    match request.method.as_str() {
        "display.create_window" => {
            let params: CreateWindowRequest = request
                .params
                .as_ref()
                .and_then(|p| CreateWindowRequest::deserialize(p).ok())
                .unwrap_or_default();

            let window_id = manager
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .create_window(params)?;

            Ok(serde_json::json!({
                "window_id": window_id.as_string()
            }))
        }
        "display.destroy_window" => {
            let window_id_str = request
                .params
                .as_ref()
                .and_then(|p| p.get("window_id"))
                .and_then(|v| v.as_str())
                .ok_or_else(|| DisplayError::IpcError("Missing window_id".to_string()))?;
            let window_id = WindowId::from_string(window_id_str)?;

            manager
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .destroy_window(window_id)?;

            Ok(serde_json::json!({"destroyed": true}))
        }
        "display.resize_window" => {
            let params = request
                .params
                .as_ref()
                .ok_or_else(|| DisplayError::IpcError("Missing params".to_string()))?;
            let window_id_str = params["window_id"]
                .as_str()
                .ok_or_else(|| DisplayError::IpcError("Missing window_id".to_string()))?;
            let window_id = WindowId::from_string(window_id_str)?;
            #[expect(
                clippy::cast_possible_truncation,
                reason = "display dimensions fit in u32"
            )]
            let width = params["width"]
                .as_u64()
                .ok_or_else(|| DisplayError::IpcError("Missing width".to_string()))?
                as u32;
            #[expect(
                clippy::cast_possible_truncation,
                reason = "display dimensions fit in u32"
            )]
            let height = params["height"]
                .as_u64()
                .ok_or_else(|| DisplayError::IpcError("Missing height".to_string()))?
                as u32;

            manager
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .resize_window(window_id, Size { width, height })?;

            Ok(serde_json::json!({"resized": true}))
        }
        "display.get_window_info" => {
            let window_id_str = request
                .params
                .as_ref()
                .and_then(|p| p.get("window_id"))
                .and_then(|v| v.as_str())
                .ok_or_else(|| DisplayError::IpcError("Missing window_id".to_string()))?;
            let window_id = WindowId::from_string(window_id_str)?;

            let info = manager
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .get_window_info(window_id)?;

            Ok(serde_json::to_value(info)
                .map_err(|e| DisplayError::IpcError(format!("Serialization error: {e}")))?)
        }
        "display.present" => {
            let params = request
                .params
                .as_ref()
                .ok_or_else(|| DisplayError::IpcError("Missing params".to_string()))?;
            let window_id_str = params["window_id"]
                .as_str()
                .ok_or_else(|| DisplayError::IpcError("Missing window_id".to_string()))?;
            let window_id = WindowId::from_string(window_id_str)?;

            let pixels = if let Some(shm_path) = params.get("shm_path").and_then(|v| v.as_str()) {
                std::fs::read(shm_path)
                    .map_err(|e| DisplayError::IpcError(format!("Failed to read shm_path: {e}")))?
            } else if let Some(data) = params.get("data").and_then(|v| v.as_str()) {
                base64::engine::general_purpose::STANDARD
                    .decode(data)
                    .map_err(|e| DisplayError::IpcError(format!("Invalid base64 data: {e}")))?
            } else {
                return Err(DisplayError::IpcError(
                    "display.present requires 'data' (base64) or 'shm_path'".to_string(),
                ));
            };

            manager
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .present_window(window_id, &pixels)?;

            Ok(serde_json::json!({"presented": true}))
        }
        "display.subscribe_input" => {
            let window_id_str = request
                .params
                .as_ref()
                .and_then(|p| p.get("window_id"))
                .and_then(|v| v.as_str())
                .ok_or_else(|| DisplayError::IpcError("Missing window_id".to_string()))?;
            let window_id = WindowId::from_string(window_id_str)?;

            input
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .set_focus(Some(window_id));

            Ok(serde_json::json!({
                "subscribed": true,
                "window_id": window_id.as_string(),
            }))
        }
        "display.poll_events" => {
            let events = input
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .poll_events()?;

            Ok(serde_json::json!({
                "events": events,
            }))
        }
        "display.get_capabilities" => Ok(serde_json::json!({
            "primal_id": PRIMAL_NAME,
            "socket_path": platform::discover_socket_path().display().to_string(),
            "transport": "isomorphic",
            "max_windows": 16,
            "supported_formats": ["RGBA8888", "BGRA8888"],
            "has_gpu_acceleration": true,
            "vsync_available": true,
            "display_count": 1,
            "input_device_count": input.read().unwrap_or_else(std::sync::PoisonError::into_inner).device_count(),
            "window_count": manager.read().unwrap_or_else(std::sync::PoisonError::into_inner).window_count(),
            "isomorphic": true,
        })),
        _ => Err(DisplayError::IpcError(format!(
            "Unknown method: {}",
            request.method
        ))),
    }
}
