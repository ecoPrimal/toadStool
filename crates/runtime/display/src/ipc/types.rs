// SPDX-License-Identifier: AGPL-3.0-or-later
//! JSON-RPC protocol types for display operations
//!
//! Defines the protocol between petalTongue (client) and Toadstool (server).

use crate::input::InputEvent;
use crate::window::{CreateWindowRequest, WindowInfo};
use serde::{Deserialize, Serialize};

// ── Canonical types from cellmembrane-types ──────────────────────────
pub use cellmembrane_types::{JsonRpcError, JsonRpcRequest, JsonRpcResponse};

// ── Extension traits for toadStool display convenience methods ───────

/// Convenience constructors for `JsonRpcRequest` (auto-generates UUID id).
pub trait JsonRpcRequestExt {
    /// Create a new request with an auto-generated UUID id.
    fn new_auto(method: impl Into<String>, params: Option<serde_json::Value>) -> Self;
}

impl JsonRpcRequestExt for JsonRpcRequest {
    fn new_auto(method: impl Into<String>, params: Option<serde_json::Value>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            method: method.into(),
            params,
            id: Some(serde_json::json!(uuid::Uuid::new_v4().to_string())),
        }
    }
}

/// Convenience constructors for `JsonRpcError` used by display dispatch.
pub trait JsonRpcErrorExt {
    /// Parse error (`-32700`).
    fn parse_error() -> Self;
    /// Invalid request (`-32600`).
    fn invalid_request() -> Self;
    /// Method not found (`-32601`).
    fn method_not_found(method: &str) -> Self;
    /// Invalid params (`-32602`).
    fn invalid_params(msg: &str) -> Self;
    /// Internal error (`-32603`).
    fn internal_error(msg: &str) -> Self;
}

impl JsonRpcErrorExt for JsonRpcError {
    fn parse_error() -> Self {
        Self { code: JsonRpcError::PARSE_ERROR, message: "Parse error".to_string(), data: None }
    }
    fn invalid_request() -> Self {
        Self { code: JsonRpcError::INVALID_REQUEST, message: "Invalid request".to_string(), data: None }
    }
    fn method_not_found(method: &str) -> Self {
        Self { code: JsonRpcError::METHOD_NOT_FOUND, message: format!("Method not found: {method}"), data: None }
    }
    fn invalid_params(msg: &str) -> Self {
        Self { code: JsonRpcError::INVALID_PARAMS, message: format!("Invalid params: {msg}"), data: None }
    }
    fn internal_error(msg: &str) -> Self {
        Self { code: JsonRpcError::INTERNAL_ERROR, message: format!("Internal error: {msg}"), data: None }
    }
}

/// Extension for creating error responses from pre-built `JsonRpcError` objects.
pub trait JsonRpcResponseExt {
    /// Create an error response from a pre-built error object.
    fn from_error(id: serde_json::Value, error: JsonRpcError) -> Self;
}

impl JsonRpcResponseExt for JsonRpcResponse {
    fn from_error(id: serde_json::Value, error: JsonRpcError) -> Self {
        Self { jsonrpc: "2.0".into(), result: None, error: Some(error), id }
    }
}

/// Display method requests
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "method", content = "params")]
pub enum DisplayMethod {
    /// Create a new window
    #[serde(rename = "display.create_window")]
    CreateWindow(CreateWindowRequest),

    /// Destroy a window
    #[serde(rename = "display.destroy_window")]
    DestroyWindow {
        /// Window ID to destroy
        window_id: String,
    },

    /// Resize a window
    #[serde(rename = "display.resize_window")]
    ResizeWindow {
        /// Window ID to resize
        window_id: String,
        /// New width
        width: u32,
        /// New height
        height: u32,
    },

    /// Get window information
    #[serde(rename = "display.get_window_info")]
    GetWindowInfo {
        /// Window ID to query
        window_id: String,
    },

    /// Subscribe to input events for a window.
    #[serde(rename = "display.subscribe_input")]
    SubscribeInput {
        /// Window ID for events
        window_id: String,
    },

    /// Poll for pending input events.
    #[serde(rename = "display.poll_events")]
    PollEvents {
        /// Optional window filter (reserved for future use).
        #[serde(skip_serializing_if = "Option::is_none")]
        window_id: Option<String>,
    },

    /// Get display capabilities
    #[serde(rename = "display.get_capabilities")]
    GetCapabilities,

    /// Present pixel data to a window's framebuffer.
    ///
    /// Exactly one of `data` or `shm_path` must be provided.
    #[serde(rename = "display.present")]
    Present {
        /// Window ID to present to.
        window_id: String,
        /// Base64-encoded raw pixel data (inline mode).
        #[serde(skip_serializing_if = "Option::is_none")]
        data: Option<String>,
        /// Path to a shared memory file containing raw pixel data (zero-copy mode).
        #[serde(skip_serializing_if = "Option::is_none")]
        shm_path: Option<String>,
    },
}

/// Display method responses
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DisplayResult {
    /// Window created
    WindowCreated {
        /// Created window ID
        window_id: String,
    },

    /// Window destroyed
    WindowDestroyed,

    /// Window resized
    WindowResized,

    /// Window information
    WindowInfo(WindowInfo),

    /// Input subscription
    InputSubscription {
        /// Subscription status
        subscribed: bool,
    },

    /// Polled events
    Events {
        /// List of input events
        events: Vec<InputEvent>,
    },

    /// Display capabilities
    Capabilities {
        /// Capability information
        capabilities: DisplayCapabilitiesInfo,
    },

    /// Present acknowledgment
    Presented,
}

/// Display capabilities information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayCapabilitiesInfo {
    /// Primal ID
    pub primal_id: String,
    /// Socket path
    pub socket_path: String,
    /// Maximum windows supported
    pub max_windows: usize,
    /// Supported pixel formats
    pub supported_formats: Vec<String>,
    /// GPU acceleration available
    pub has_gpu_acceleration: bool,
    /// `VSync` available
    pub vsync_available: bool,
    /// Number of displays
    pub display_count: usize,
    /// Number of input devices
    pub input_device_count: usize,
    /// Current window count (Phase 3)
    pub window_count: usize,
    /// Isomorphic IPC support (Phase 3)
    pub isomorphic: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jsonrpc_request_serialization() {
        let req = JsonRpcRequest::new_auto("display.create_window", None);
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"jsonrpc\":\"2.0\""));
        assert!(json.contains("\"method\":\"display.create_window\""));
    }

    #[test]
    fn test_jsonrpc_notification() {
        let notif = JsonRpcRequest::notification("display.input_event", None);
        assert!(notif.id.is_none());
    }

    #[test]
    fn test_jsonrpc_response_success() {
        let resp =
            JsonRpcResponse::success(serde_json::json!(1), serde_json::json!({"status": "ok"}));
        assert!(resp.result.is_some());
        assert!(resp.error.is_none());
    }

    #[test]
    fn test_jsonrpc_error() {
        let err = JsonRpcError::method_not_found("unknown.method");
        assert_eq!(err.code, -32601);
        assert!(err.message.contains("unknown.method"));
    }
}
