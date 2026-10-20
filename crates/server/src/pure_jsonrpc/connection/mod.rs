// SPDX-License-Identifier: AGPL-3.0-or-later
//! Connection handling for Pure JSON-RPC server (Unix socket + TCP)
//!
//! Generic over JsonRpcHandler. Parses requests from owned bytes so that
//! JsonRpcRequest's Cow<'a, str> can borrow from the slice during deserialization.
//!
//! Supports riboCipher transport signal detection per
//! `ecoPrimals/infra/wateringHole/RIBOCIPHER_TRANSPORT_SIGNAL_STANDARD.md`
//! and G65 protocol negotiation per
//! `wateringHole/specs/PROTOCOL_NEGOTIATION_SPEC.md`.

#[cfg(unix)]
mod btsp_unix;
mod dispatch;
#[cfg(unix)]
pub(crate) mod ipc_protocol;
#[cfg(unix)]
pub(crate) mod protocol_negotiation;
mod tcp;
#[cfg(test)]
mod tests;
#[cfg(unix)]
mod unix;
#[cfg(test)]
mod unix_tests;

pub use tcp::serve_tcp;
#[cfg(unix)]
pub use unix::{
    prebind_unix_listener, serve_unix, serve_unix_g65, serve_unix_prebound,
    spawn_early_health_responder,
};

use crate::errors::{ServerError, ServerResult};
use crate::pure_jsonrpc::handler::ConnectionTrustHints;
use crate::pure_jsonrpc::types::{JsonRpcError, JsonRpcErrorExt};
use crate::pure_jsonrpc::{JsonRpcHandler, JsonRpcResponse, ZeroCopyJsonRpcRequest};

/// riboCipher transport signal constants.
///
/// Per `ecoPrimals/infra/wateringHole/RIBOCIPHER_TRANSPORT_SIGNAL_STANDARD.md`:
/// Every connection declares its intended protocol via a signal prefix instead
/// of fragile peek-and-guess detection.
pub(crate) mod ribocipher {
    /// MitoBeacon CLEAR — local/trusted wire (2 bytes: prefix + protocol type).
    pub const CLEAR: u8 = 0xEC;
    /// MitoBeacon MITO — cross-gate relay (6 bytes: prefix + 4-byte HMAC tag + protocol type).
    /// HMAC validation deferred to Wave 115; currently accepted and logged.
    pub const MITO: u8 = 0xED;
    /// Nuclear Lineage — per-user privileged channel (7+ bytes: prefix + 6-byte ciphertext).
    /// Not yet implemented; connections rejected.
    pub const NUCLEAR: u8 = 0xEE;

    /// Protocol type byte (after CLEAR prefix, or after MITO HMAC tag).
    pub mod protocol_type {
        pub const PROBE: u8 = 0x00;
        pub const NDJSON_JSONRPC: u8 = 0x01;
        #[expect(dead_code, reason = "reserved for BTSP-over-riboCipher routing")]
        pub const BTSP_BINARY: u8 = 0x02;
        #[expect(dead_code, reason = "reserved for BTSP-over-riboCipher routing")]
        pub const BTSP_JSONLINE: u8 = 0x03;
        pub const HTTP: u8 = 0x04;
    }
}

/// Parse request from body bytes, dispatch to handler, return serialized response.
///
/// Uses owned body so JsonRpcRequest can borrow from it via `serde_json::from_slice`.
#[cfg_attr(test, allow(dead_code))]
pub async fn process_request(
    handler: &JsonRpcHandler,
    body: &[u8],
    conn: ConnectionTrustHints,
) -> ServerResult<Vec<u8>> {
    let request: ZeroCopyJsonRpcRequest<'_> = match serde_json::from_slice(body) {
        Ok(r) => r,
        Err(e) => {
            let response = JsonRpcResponse {
                jsonrpc: "2.0".into(),
                result: None,
                error: Some(JsonRpcError::parse_error(format!("Parse error: {e}"))),
                id: serde_json::Value::Null,
            };
            return serde_json::to_vec(&response).map_err(|e| ServerError::Internal(e.to_string()));
        }
    };

    let response = handler.handle_request_with_connection(&request, conn).await;

    serde_json::to_vec(&response).map_err(|e| ServerError::Internal(e.to_string()))
}
