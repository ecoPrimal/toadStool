// SPDX-License-Identifier: AGPL-3.0-or-later
//! Display IPC operations (windows, capabilities, presentation, input, endpoint metadata).

use base64::Engine;

use super::DisplayClient;
use crate::DisplayError;
use crate::input::InputEvent;
use crate::ipc::types::{DisplayCapabilitiesInfo, JsonRpcRequestExt};
use crate::window::{CreateWindowRequest, WindowId, WindowInfo};

impl DisplayClient {
    /// Create a window
    ///
    /// # Errors
    ///
    /// Returns an error if the IPC request fails or the server returns an error.
    pub async fn create_window(&mut self, request: CreateWindowRequest) -> crate::Result<WindowId> {
        let params = serde_json::to_value(request)
            .map_err(|e| DisplayError::IpcError(format!("Failed to serialize request: {e}")))?;
        let req = crate::ipc::types::JsonRpcRequest::new_auto("display.create_window", Some(params));

        let response = self.send_request(req).await?;

        if let Some(result) = response.result {
            let window_id_str = result["window_id"]
                .as_str()
                .ok_or_else(|| DisplayError::IpcError("Invalid response".to_string()))?;
            WindowId::from_string(window_id_str)
        } else if let Some(error) = response.error {
            Err(DisplayError::IpcError(format!(
                "Server error: {}",
                error.message
            )))
        } else {
            Err(DisplayError::IpcError("Invalid response".to_string()))
        }
    }

    /// Destroy a window
    ///
    /// # Errors
    ///
    /// Returns an error if the IPC request fails or the server returns an error.
    pub async fn destroy_window(&mut self, window_id: WindowId) -> crate::Result<()> {
        let req = crate::ipc::types::JsonRpcRequest::new_auto(
            "display.destroy_window",
            Some(serde_json::json!({"window_id": window_id.as_string()})),
        );

        let response = self.send_request(req).await?;

        if response.error.is_some() {
            Err(DisplayError::IpcError(
                "Failed to destroy window".to_string(),
            ))
        } else {
            Ok(())
        }
    }

    /// Resize a window
    ///
    /// # Errors
    ///
    /// Returns an error if the IPC request fails or the server returns an error.
    pub async fn resize_window(
        &mut self,
        window_id: WindowId,
        width: u32,
        height: u32,
    ) -> crate::Result<()> {
        let req = crate::ipc::types::JsonRpcRequest::new_auto(
            "display.resize_window",
            Some(serde_json::json!({
                "window_id": window_id.as_string(),
                "width": width,
                "height": height
            })),
        );

        let response = self.send_request(req).await?;

        if response.error.is_some() {
            Err(DisplayError::IpcError(
                "Failed to resize window".to_string(),
            ))
        } else {
            Ok(())
        }
    }

    /// Get window information
    ///
    /// # Errors
    ///
    /// Returns an error if the IPC request fails or the server returns an error.
    pub async fn get_window_info(&mut self, window_id: WindowId) -> crate::Result<WindowInfo> {
        let req = crate::ipc::types::JsonRpcRequest::new_auto(
            "display.get_window_info",
            Some(serde_json::json!({"window_id": window_id.as_string()})),
        );

        let response = self.send_request(req).await?;

        if let Some(result) = response.result {
            serde_json::from_value(result)
                .map_err(|e| DisplayError::IpcError(format!("Parse error: {e}")))
        } else if let Some(error) = response.error {
            Err(DisplayError::IpcError(format!(
                "Server error: {}",
                error.message
            )))
        } else {
            Err(DisplayError::IpcError("Invalid response".to_string()))
        }
    }

    /// Get display capabilities
    ///
    /// # Errors
    ///
    /// Returns an error if the IPC request fails or the server returns an error.
    pub async fn get_capabilities(&mut self) -> crate::Result<DisplayCapabilitiesInfo> {
        let req = crate::ipc::types::JsonRpcRequest::new_auto("display.get_capabilities", None);

        let response = self.send_request(req).await?;

        if let Some(result) = response.result {
            serde_json::from_value(result)
                .map_err(|e| DisplayError::IpcError(format!("Parse error: {e}")))
        } else if let Some(error) = response.error {
            Err(DisplayError::IpcError(format!(
                "Server error: {}",
                error.message
            )))
        } else {
            Err(DisplayError::IpcError("Invalid response".to_string()))
        }
    }

    /// Present raw RGBA pixel data to a window (inline base64 mode).
    ///
    /// The pixel data is base64-encoded and sent in the JSON-RPC request.
    /// For large framebuffers, prefer [`present_shm`](Self::present_shm).
    ///
    /// # Errors
    ///
    /// Returns an error if the IPC request fails or the server returns an error.
    pub async fn present(&mut self, window_id: WindowId, pixels: &[u8]) -> crate::Result<()> {
        let data_b64 = base64::engine::general_purpose::STANDARD.encode(pixels);
        let req = crate::ipc::types::JsonRpcRequest::new_auto(
            "display.present",
            Some(serde_json::json!({
                "window_id": window_id.as_string(),
                "data": data_b64,
            })),
        );

        let response = self.send_request(req).await?;

        if let Some(error) = response.error {
            Err(DisplayError::IpcError(format!(
                "Server error: {}",
                error.message
            )))
        } else {
            Ok(())
        }
    }

    /// Present framebuffer via shared memory path (zero-copy mode).
    ///
    /// The client writes raw RGBA pixel data to the given path (e.g.
    /// `/dev/shm/toadstool-fb-{window_id}`), then calls this method.
    /// The server reads the file and copies it to the DRM framebuffer.
    ///
    /// # Errors
    ///
    /// Returns an error if the IPC request fails or the server returns an error.
    pub async fn present_shm(&mut self, window_id: WindowId, shm_path: &str) -> crate::Result<()> {
        let req = crate::ipc::types::JsonRpcRequest::new_auto(
            "display.present",
            Some(serde_json::json!({
                "window_id": window_id.as_string(),
                "shm_path": shm_path,
            })),
        );

        let response = self.send_request(req).await?;

        if let Some(error) = response.error {
            Err(DisplayError::IpcError(format!(
                "Server error: {}",
                error.message
            )))
        } else {
            Ok(())
        }
    }

    /// Subscribe to input events for a window.
    ///
    /// Sets the server-side input focus to the given window so subsequent
    /// [`poll_events`](Self::poll_events) calls return events for it.
    ///
    /// # Errors
    ///
    /// Returns an error if the IPC request fails or the server returns an error.
    pub async fn subscribe_input(&mut self, window_id: WindowId) -> crate::Result<()> {
        let req = crate::ipc::types::JsonRpcRequest::new_auto(
            "display.subscribe_input",
            Some(serde_json::json!({"window_id": window_id.as_string()})),
        );

        let response = self.send_request(req).await?;

        if let Some(error) = response.error {
            Err(DisplayError::IpcError(format!(
                "Server error: {}",
                error.message
            )))
        } else {
            Ok(())
        }
    }

    /// Poll for pending input events (non-blocking).
    ///
    /// Returns all events buffered since the last poll. Returns an empty
    /// `Vec` when no events are queued.
    ///
    /// # Errors
    ///
    /// Returns an error if the IPC request fails or the server returns an error.
    pub async fn poll_events(&mut self) -> crate::Result<Vec<InputEvent>> {
        let req = crate::ipc::types::JsonRpcRequest::new_auto("display.poll_events", None);

        let response = self.send_request(req).await?;

        if let Some(result) = response.result {
            let events: Vec<InputEvent> = serde_json::from_value(
                result
                    .get("events")
                    .cloned()
                    .unwrap_or(serde_json::json!([])),
            )
            .map_err(|e| DisplayError::IpcError(format!("Parse error: {e}")))?;
            Ok(events)
        } else if let Some(error) = response.error {
            Err(DisplayError::IpcError(format!(
                "Server error: {}",
                error.message
            )))
        } else {
            Err(DisplayError::IpcError("Invalid response".to_string()))
        }
    }

    /// Get endpoint string for display purposes
    ///
    /// **Helper for health checks and monitoring**
    #[must_use]
    pub fn endpoint_string(&self) -> String {
        match &self.endpoint {
            super::IpcEndpoint::UnixSocket(path) => path.display().to_string(),
            super::IpcEndpoint::TcpLocal(addr) => addr.to_string(),
        }
    }

    /// Get transport name for display purposes
    ///
    /// **Helper for health checks and monitoring**
    #[must_use]
    pub const fn transport_name(&self) -> &str {
        match &self.endpoint {
            super::IpcEndpoint::UnixSocket(_) => "unix",
            super::IpcEndpoint::TcpLocal(_) => "tcp",
        }
    }
}
