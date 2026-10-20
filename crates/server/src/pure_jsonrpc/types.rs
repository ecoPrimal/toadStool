// SPDX-License-Identifier: AGPL-3.0-or-later
//! JSON-RPC 2.0 wire types
//!
//! Pure data structures for the JSON-RPC 2.0 protocol.
//! No logic — only serialization/deserialization.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;

use crate::rpc_types::{
    ResourceRequirements, WorkloadPriority, WorkloadSubmission as TarpcWorkloadSubmission,
};

// Canonical JSON-RPC types from cellmembrane-types (String-based, owning).
pub use cellmembrane_types::{JsonRpcError, JsonRpcRequest, JsonRpcResponse};

/// Zero-copy JSON-RPC 2.0 Request (toadStool hot-path wrapper).
///
/// Uses `Cow` for zero-copy deserialization via `serde_json::from_slice`.
/// Convert to canonical `JsonRpcRequest` at API boundaries.
#[derive(Debug, Clone, Deserialize)]
pub struct ZeroCopyJsonRpcRequest<'a> {
    /// Protocol version (must be "2.0")
    #[serde(borrow)]
    pub jsonrpc: Cow<'a, str>,

    /// Method name (e.g., "toadstool.submit_workload")
    #[serde(borrow)]
    pub method: Cow<'a, str>,

    /// Optional parameters (can be object or array)
    #[serde(default)]
    pub params: Option<serde_json::Value>,

    /// Request ID (for matching request/response)
    pub id: Option<serde_json::Value>,
}

impl<'a> From<ZeroCopyJsonRpcRequest<'a>> for JsonRpcRequest {
    fn from(zc: ZeroCopyJsonRpcRequest<'a>) -> Self {
        Self {
            jsonrpc: zc.jsonrpc.into_owned(),
            method: zc.method.into_owned(),
            params: zc.params,
            id: zc.id,
        }
    }
}

/// Zero-copy JSON-RPC version constant for responses
pub(crate) const JSONRPC_VERSION: &str = toadstool_common::constants::jsonrpc::VERSION;

/// toadStool-specific JSON-RPC error factory methods (ecosystem error codes).
///
/// The canonical `JsonRpcError` type and standard error constants are imported
/// from `cellmembrane_types`. These extension methods add toadStool-specific
/// ecosystem error codes (unauthorized, permission denied) not in the spec.
pub trait JsonRpcErrorExt {
    /// Creates a parse error.
    fn parse_error(msg: impl Into<String>) -> JsonRpcError;
    /// Creates an invalid request error.
    fn invalid_request(msg: impl Into<String>) -> JsonRpcError;
    /// Creates a method not found error.
    fn method_not_found(method: &str) -> JsonRpcError;
    /// Creates an invalid params error.
    fn invalid_params(msg: impl Into<String>) -> JsonRpcError;
    /// Creates an internal error.
    fn internal_error(msg: impl Into<String>) -> JsonRpcError;
    /// Creates an application-defined server error with a custom code.
    fn server_error(code: i32, msg: impl Into<String>) -> JsonRpcError;
    /// Unauthorized (-32000): caller has no identity/token (ecosystem standard).
    fn unauthorized(msg: impl Into<String>) -> JsonRpcError;
    /// Permission denied (-32001): caller authenticated but lacks access (ecosystem standard).
    fn permission_denied(method: &str) -> JsonRpcError;
}

impl JsonRpcErrorExt for JsonRpcError {
    fn parse_error(msg: impl Into<String>) -> JsonRpcError {
        JsonRpcError { code: JsonRpcError::PARSE_ERROR, message: msg.into(), data: None }
    }
    fn invalid_request(msg: impl Into<String>) -> JsonRpcError {
        JsonRpcError { code: JsonRpcError::INVALID_REQUEST, message: msg.into(), data: None }
    }
    fn method_not_found(method: &str) -> JsonRpcError {
        JsonRpcError { code: JsonRpcError::METHOD_NOT_FOUND, message: format!("Method not found: {method}"), data: None }
    }
    fn invalid_params(msg: impl Into<String>) -> JsonRpcError {
        JsonRpcError { code: JsonRpcError::INVALID_PARAMS, message: msg.into(), data: None }
    }
    fn internal_error(msg: impl Into<String>) -> JsonRpcError {
        JsonRpcError { code: JsonRpcError::INTERNAL_ERROR, message: msg.into(), data: None }
    }
    fn server_error(code: i32, msg: impl Into<String>) -> JsonRpcError {
        JsonRpcError { code, message: msg.into(), data: None }
    }
    fn unauthorized(msg: impl Into<String>) -> JsonRpcError {
        JsonRpcError {
            code: toadstool_common::constants::jsonrpc::error_codes::UNAUTHORIZED,
            message: msg.into(),
            data: None,
        }
    }
    fn permission_denied(method: &str) -> JsonRpcError {
        JsonRpcError {
            code: toadstool_common::constants::jsonrpc::error_codes::PERMISSION_DENIED,
            message: format!("Permission denied: {method}"),
            data: None,
        }
    }
}

fn serialize_arc_str<S>(v: &Arc<str>, s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    s.serialize_str(v)
}

fn deserialize_arc_str<'de, D>(d: D) -> Result<Arc<str>, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(d)?;
    Ok(Arc::from(s))
}

/// JSON-friendly workload submission (base64 encoding for binary data)
///
/// `workload_id` and `workload_type` use `Arc<str>` per wateringHole zero-copy guidelines.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonWorkloadSubmission {
    /// Workload identifier.
    #[serde(
        serialize_with = "serialize_arc_str",
        deserialize_with = "deserialize_arc_str"
    )]
    pub workload_id: Arc<str>,
    /// Workload type.
    #[serde(
        serialize_with = "serialize_arc_str",
        deserialize_with = "deserialize_arc_str"
    )]
    pub workload_type: Arc<str>,
    /// Base64-encoded binary data
    pub data: String,
    /// Optional metadata.
    pub metadata: HashMap<String, String>,
    /// Workload priority.
    pub priority: WorkloadPriority,
    /// Resource requirements.
    pub requirements: ResourceRequirements,
}

impl JsonWorkloadSubmission {
    /// Convert into tarpc submission (decode base64, consumes self)
    ///
    /// # Errors
    ///
    /// Returns error if base64 data is invalid.
    pub fn into_tarpc(self) -> Result<TarpcWorkloadSubmission, String> {
        use base64::{Engine as _, engine::general_purpose::STANDARD};

        let data = STANDARD
            .decode(&self.data)
            .map_err(|e| format!("Invalid base64 data: {e}"))?;

        Ok(TarpcWorkloadSubmission {
            workload_id: self.workload_id,
            workload_type: self.workload_type,
            data: data.into(),
            metadata: self.metadata,
            priority: self.priority,
            requirements: self.requirements,
        })
    }
}

#[cfg(test)]
mod proptest_tests {
    use super::ZeroCopyJsonRpcRequest;
    use proptest::prelude::*;
    use serde::Serialize;

    #[derive(Serialize)]
    struct JsonRpcRequestBuilder {
        jsonrpc: String,
        method: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        params: Option<serde_json::Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<serde_json::Value>,
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn prop_jsonrpc_request_parse_method_params_id(
            method in "[a-zA-Z0-9_.]{1,60}",
            id_num in prop::option::of(0i64..10000i64),
        ) {
            let builder = JsonRpcRequestBuilder {
                jsonrpc: "2.0".to_string(),
                method: method.clone(),
                params: Some(serde_json::json!({"key": "value", "n": 42})),
                id: id_num.map(|n| serde_json::json!(n)),
            };
            let json = serde_json::to_string(&builder).unwrap();
            let parsed: ZeroCopyJsonRpcRequest<'_> = serde_json::from_str(&json).unwrap();
            prop_assert_eq!(parsed.jsonrpc.as_ref(), "2.0");
            prop_assert_eq!(parsed.method.as_ref(), method);
            prop_assert!(parsed.params.is_some());
        }

        #[test]
        fn prop_jsonrpc_request_parse_string_id(
            method in "[a-z_]+",
            id_str in "[a-zA-Z0-9_-]{1,30}",
        ) {
            let builder = JsonRpcRequestBuilder {
                jsonrpc: "2.0".to_string(),
                method: method.clone(),
                params: None,
                id: Some(serde_json::Value::String(id_str)),
            };
            let json = serde_json::to_string(&builder).unwrap();
            let parsed: ZeroCopyJsonRpcRequest<'_> = serde_json::from_str(&json).unwrap();
            prop_assert_eq!(parsed.method.as_ref(), method);
            prop_assert!(parsed.id.is_some());
        }

        #[test]
        fn prop_jsonrpc_request_parse_array_params(
            method in "[a-z.]+",
        ) {
            let builder = JsonRpcRequestBuilder {
                jsonrpc: "2.0".to_string(),
                method: method.clone(),
                params: Some(serde_json::json!([1, 2, "three"])),
                id: Some(serde_json::json!(1)),
            };
            let json = serde_json::to_string(&builder).unwrap();
            let parsed: ZeroCopyJsonRpcRequest<'_> = serde_json::from_str(&json).unwrap();
            prop_assert_eq!(parsed.method.as_ref(), method);
            prop_assert!(parsed.params.is_some());
        }
    }
}
