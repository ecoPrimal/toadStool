// SPDX-License-Identifier: AGPL-3.0-or-later
//! IPC protocol (JSON-RPC 2.0 over Unix sockets)
//!
//! Provides client-server communication for display operations.
//!
//! ## Architecture
//!
//! ```text
//! petalTongue (Client)
//!    ↓ JSON-RPC over Unix socket
//! DisplayServer
//!    ↓ Calls WindowManager
//! WindowManager
//!    ↓ DRM/KMS operations
//! Hardware
//! ```
//!
//! ## Protocol Methods (wateringHole Semantic Naming Standard)
//!
//! - `display.create_window` - Create a new window
//! - `display.destroy_window` - Destroy a window
//! - `display.resize_window` - Resize a window
//! - `display.get_window_info` - Get window information
//! - `display.subscribe_input` - Subscribe to input events
//! - `display.poll_events` - Poll for pending events
//! - `display.get_capabilities` - Get display capabilities
//! - `display.present` - Present framebuffer (future: zero-copy)
//!
//! ## Example (Server)
//!
//! ```rust,ignore
//! use toadstool_display::{DisplayServer, WindowManager};
//!
//! # async fn example() -> Result<()> {
//! let manager = WindowManager::new().await?;
//! let server = DisplayServer::new(manager)
//!     .bind("/run/user/1000/toadstool/display.sock")
//!     .await?;
//!
//! server.serve().await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Example (Client)
//!
//! ```rust,ignore
//! use toadstool_display::ipc::DisplayClient;
//! use toadstool_display::window::CreateWindowRequest;
//!
//! # async fn example() -> Result<()> {
//! let mut client = DisplayClient::connect("/run/user/1000/toadstool/display.sock").await?;
//!
//! let window_id = client.create_window(CreateWindowRequest::default()).await?;
//! let info = client.get_window_info(window_id).await?;
//!
//! println!("Window: {}x{}", info.width, info.height);
//! # Ok(())
//! # }
//! ```

pub mod client;
mod dispatch;
#[cfg(test)]
mod dispatch_tests;
pub mod health;
mod platform;
pub mod server;
pub mod types;

// Re-exports
pub use client::{DisplayClient, IpcEndpoint};
pub use health::{
    HealthCheckResult, HealthStatus, check_display_health, check_display_health_with_timeout,
    monitor_display_health,
};
pub use server::{DisplayServer, IpcTransport};
pub use types::{
    DisplayCapabilitiesInfo, DisplayMethod, DisplayResult, JsonRpcError, JsonRpcErrorExt,
    JsonRpcRequest, JsonRpcRequestExt, JsonRpcResponse, JsonRpcResponseExt,
};
