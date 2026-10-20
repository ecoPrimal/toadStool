// SPDX-License-Identifier: AGPL-3.0-or-later
//! # Pure Rust JSON-RPC 2.0 Implementation
//!
//! **Pattern**: Security service's proven manual implementation  
//! **Dependencies**: ZERO extra crates — only `serde_json` (already in workspace)  
//! **Status**: 100% Pure Rust — NO `jsonrpsee`, NO `ring`!
//!
//! ## Architecture
//!
//! ```text
//! Request
//!   → Parse JSON (types)
//!   → Validate version (handler)
//!   → Resolve semantic alias (SemanticMethodRegistry)
//!   → Dispatch to method handler
//!   → Serialize response (types)
//! ```
//!
//! ## Module layout
//!
//! | Module | Content |
//! |--------|---------|
//! | `types` | `JsonRpcRequest`, `JsonRpcResponse`, `JsonRpcError`, `JsonWorkloadSubmission` |
//! | `handler` | `JsonRpcHandler` — thin coordinator; delegates to JobHandler, WorkloadHandler, ResourceHandler, TransportHandler |

mod connection;
mod handler;
mod types;

#[cfg(unix)]
pub use connection::{
    prebind_unix_listener, serve_unix, serve_unix_g65, serve_unix_prebound,
    spawn_early_health_responder,
};
pub use connection::{process_request, serve_tcp};
#[cfg(target_os = "linux")]
pub use handler::HwLearnHandler;
pub use handler::JsonRpcHandler;
pub use types::{
    JsonRpcError, JsonRpcErrorExt, JsonRpcRequest, JsonRpcResponse, JsonWorkloadSubmission,
    ZeroCopyJsonRpcRequest,
};

#[cfg(test)]
mod tests;
