// SPDX-License-Identifier: AGPL-3.0-or-later
//! # BYOB JSON-RPC dispatch for container runtime
//!
//! Pure Rust JSON-RPC handlers for BYOB deployment operations.
//! Replaces the former axum/HTTP layer — toadStool uses JSON-RPC over
//! Unix sockets per wateringHole standard; HTTP belongs to songBird.

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{error, info};
use uuid::Uuid;

use toadstool::{
    ToadStoolError,
    byob::{ByobDeploymentRequest, ByobExecutor},
};
use toadstool_common::constants::jsonrpc::error_codes;

// ── Canonical types from cellmembrane-types ──────────────────────────
pub use cellmembrane_types::{JsonRpcError, JsonRpcRequest, JsonRpcResponse};

// ── Extension traits for toadStool container convenience methods ─────

/// Convenience constructors for `JsonRpcError` used by BYOB dispatch.
pub trait JsonRpcErrorExt {
    /// Parse error (`-32700`).
    fn parse_error() -> Self;
    /// Method not found (`-32601`).
    fn method_not_found(method: &str) -> Self;
    /// Invalid params (`-32602`).
    fn invalid_params(msg: &str) -> Self;
    /// Internal error (`-32603`).
    fn internal_error(msg: &str) -> Self;
}

impl JsonRpcErrorExt for JsonRpcError {
    fn parse_error() -> Self {
        Self { code: error_codes::PARSE_ERROR, message: "Parse error".to_string(), data: None }
    }
    fn method_not_found(method: &str) -> Self {
        Self { code: error_codes::METHOD_NOT_FOUND, message: format!("Method not found: {method}"), data: None }
    }
    fn invalid_params(msg: &str) -> Self {
        Self { code: error_codes::INVALID_PARAMS, message: format!("Invalid params: {msg}"), data: None }
    }
    fn internal_error(msg: &str) -> Self {
        Self { code: error_codes::INTERNAL_ERROR, message: format!("Internal error: {msg}"), data: None }
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

/// Stop deployment response
#[derive(Debug, Serialize, Deserialize)]
pub struct StopDeploymentResponse {
    pub deployment_id: Uuid,
    pub message: String,
}

/// Health check response
#[derive(Debug, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub message: String,
}

/// BYOB JSON-RPC API dispatcher
pub struct ByobApi<E: ByobExecutor + Send + Sync + 'static> {
    executor: Arc<E>,
}

impl<E: ByobExecutor + Send + Sync + 'static> ByobApi<E> {
    /// Create a new BYOB API dispatcher.
    pub fn new(executor: Arc<E>) -> Self {
        Self { executor }
    }

    /// Dispatch a JSON-RPC request to the appropriate handler.
    pub async fn dispatch(&self, request_str: &str) -> JsonRpcResponse {
        let request: JsonRpcRequest = match serde_json::from_str(request_str.trim()) {
            Ok(req) => req,
            Err(_) => {
                return JsonRpcResponse::from_error(
                    serde_json::json!(null),
                    JsonRpcError::parse_error(),
                );
            }
        };

        let id = request.id.clone().unwrap_or(serde_json::json!(null));

        match request.method.as_str() {
            "byob.deploy" => self.deploy_biome(id, request.params).await,
            "byob.list_deployments" => self.list_deployments(id).await,
            "byob.get_deployment" => self.get_deployment(id, request.params).await,
            "byob.stop_deployment" => self.stop_deployment(id, request.params).await,
            "byob.get_resource_usage" => self.get_resource_usage(id, request.params).await,
            "byob.health" => self.health_check(id),
            "byob.info" => self.info(id),
            _ => JsonRpcResponse::from_error(id, JsonRpcError::method_not_found(&request.method)),
        }
    }

    async fn deploy_biome(
        &self,
        id: serde_json::Value,
        params: Option<serde_json::Value>,
    ) -> JsonRpcResponse {
        let request: ByobDeploymentRequest = match params
            .as_ref()
            .and_then(|p| serde_json::from_value(p.clone()).ok())
        {
            Some(req) => req,
            None => {
                return JsonRpcResponse::from_error(
                    id,
                    JsonRpcError::invalid_params("missing or invalid deployment request"),
                );
            }
        };

        info!(
            "Received BYOB deployment request for team {}",
            request.team_id
        );

        match self.executor.deploy_biome(request).await {
            Ok(response) => {
                info!("BYOB deployment {} completed", response.deployment_id);
                JsonRpcResponse::success(id, serde_json::to_value(response).unwrap_or_default())
            }
            Err(e) => {
                error!("BYOB deployment failed: {:?}", e);
                JsonRpcResponse::from_error(id, toadstool_error_to_jsonrpc(&e))
            }
        }
    }

    async fn list_deployments(&self, id: serde_json::Value) -> JsonRpcResponse {
        match self.executor.list_deployments().await {
            Ok(deployments) => {
                JsonRpcResponse::success(id, serde_json::to_value(deployments).unwrap_or_default())
            }
            Err(e) => {
                error!("Failed to list deployments: {:?}", e);
                JsonRpcResponse::from_error(id, toadstool_error_to_jsonrpc(&e))
            }
        }
    }

    async fn get_deployment(
        &self,
        id: serde_json::Value,
        params: Option<serde_json::Value>,
    ) -> JsonRpcResponse {
        let deployment_id = match extract_deployment_id(&params) {
            Some(did) => did,
            None => {
                return JsonRpcResponse::from_error(
                    id,
                    JsonRpcError::invalid_params("missing or invalid deployment_id"),
                );
            }
        };

        match self.executor.get_deployment_status(deployment_id).await {
            Ok(response) => {
                JsonRpcResponse::success(id, serde_json::to_value(response).unwrap_or_default())
            }
            Err(e) => {
                error!("Failed to get deployment status: {:?}", e);
                JsonRpcResponse::from_error(id, toadstool_error_to_jsonrpc(&e))
            }
        }
    }

    async fn stop_deployment(
        &self,
        id: serde_json::Value,
        params: Option<serde_json::Value>,
    ) -> JsonRpcResponse {
        let deployment_id = match extract_deployment_id(&params) {
            Some(did) => did,
            None => {
                return JsonRpcResponse::from_error(
                    id,
                    JsonRpcError::invalid_params("missing or invalid deployment_id"),
                );
            }
        };

        match self.executor.stop_deployment(deployment_id).await {
            Ok(()) => {
                let response = StopDeploymentResponse {
                    deployment_id,
                    message: "Deployment stopped successfully".to_string(),
                };
                JsonRpcResponse::success(id, serde_json::to_value(response).unwrap_or_default())
            }
            Err(e) => {
                error!("Failed to stop deployment: {:?}", e);
                JsonRpcResponse::from_error(id, toadstool_error_to_jsonrpc(&e))
            }
        }
    }

    async fn get_resource_usage(
        &self,
        id: serde_json::Value,
        params: Option<serde_json::Value>,
    ) -> JsonRpcResponse {
        let deployment_id = match extract_deployment_id(&params) {
            Some(did) => did,
            None => {
                return JsonRpcResponse::from_error(
                    id,
                    JsonRpcError::invalid_params("missing or invalid deployment_id"),
                );
            }
        };

        match self.executor.get_resource_usage(deployment_id).await {
            Ok(usage) => {
                JsonRpcResponse::success(id, serde_json::to_value(usage).unwrap_or_default())
            }
            Err(e) => {
                error!("Failed to get resource usage: {:?}", e);
                JsonRpcResponse::from_error(id, toadstool_error_to_jsonrpc(&e))
            }
        }
    }

    fn health_check(&self, id: serde_json::Value) -> JsonRpcResponse {
        let response = HealthResponse {
            status: "healthy".to_string(),
            message: "Toadstool BYOB API is operational".to_string(),
        };
        JsonRpcResponse::success(id, serde_json::to_value(response).unwrap_or_default())
    }

    fn info(&self, id: serde_json::Value) -> JsonRpcResponse {
        JsonRpcResponse::success(
            id,
            serde_json::json!({
                "service": "toadstool-byob-server",
                "version": env!("CARGO_PKG_VERSION"),
                "transport": "json-rpc-2.0",
            }),
        )
    }
}

fn extract_deployment_id(params: &Option<serde_json::Value>) -> Option<Uuid> {
    params
        .as_ref()?
        .get("deployment_id")?
        .as_str()
        .and_then(|s| s.parse().ok())
}

fn toadstool_error_to_jsonrpc(err: &ToadStoolError) -> JsonRpcError {
    use toadstool::error::{ConfigError, ResourceError, SystemError};

    match err {
        ToadStoolError::Resource(ResourceError::NotFound { .. }) | ToadStoolError::NotFound(_) => {
            JsonRpcError {
                code: error_codes::WORKLOAD_NOT_FOUND,
                message: err.to_string(),
                data: None,
            }
        }
        ToadStoolError::Configuration(ConfigError::ValidationError { .. }) => JsonRpcError {
            code: error_codes::INVALID_PARAMS,
            message: err.to_string(),
            data: None,
        },
        ToadStoolError::Resource(
            ResourceError::AllocationFailure { .. }
            | ResourceError::LimitExceeded { .. }
            | ResourceError::Insufficient { .. },
        ) => JsonRpcError {
            code: error_codes::RESOURCE_EXHAUSTED,
            message: err.to_string(),
            data: None,
        },
        ToadStoolError::System(SystemError::NotSupported { .. }) => JsonRpcError {
            code: error_codes::CAPABILITY_NOT_AVAILABLE,
            message: err.to_string(),
            data: None,
        },
        _ => JsonRpcError::internal_error(&err.to_string()),
    }
}
