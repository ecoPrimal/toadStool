// SPDX-License-Identifier: AGPL-3.0-or-later
//! Resource estimation, validation, and optimization for JSON-RPC handler.

use crate::resource_estimator::ResourceEstimator;
use crate::resource_optimizer::ResourceOptimizer;
use crate::resource_validator::ResourceValidator;

use crate::pure_jsonrpc::types::{JsonRpcError, JsonRpcErrorExt};

/// Handles resource workload operations (estimate, validate, optimize).
pub(super) struct ResourceHandler {
    pub(super) estimator: ResourceEstimator,
    pub(super) validator: ResourceValidator,
    pub(super) optimizer: ResourceOptimizer,
}

impl ResourceHandler {
    pub(super) fn new() -> Self {
        Self {
            estimator: ResourceEstimator::new(),
            validator: ResourceValidator::new(),
            optimizer: ResourceOptimizer::new(),
        }
    }

    pub(super) fn extract_graph(
        params: Option<&serde_json::Value>,
    ) -> Result<crate::graph_types::ExecutionGraph, JsonRpcError> {
        let params = params.ok_or_else(|| JsonRpcError::invalid_params("Missing params"))?;
        let graph_value = params
            .get("graph")
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        serde_json::from_value(graph_value)
            .map_err(|e| JsonRpcError::invalid_params(format!("Invalid graph parameter: {e}")))
    }

    #[expect(
        clippy::unused_async,
        reason = "async signature required by trait/interface"
    )] // JSON-RPC method dispatch; sync estimator.estimate()
    pub(super) async fn resources_estimate(
        &self,
        params: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let graph = Self::extract_graph(params)?;
        self.estimator
            .estimate(&graph)
            .map_err(|e| JsonRpcError::internal_error(format!("Estimation failed: {e}")))
            .and_then(|estimate| {
                serde_json::to_value(estimate)
                    .map_err(|e| JsonRpcError::internal_error(format!("Serialization: {e}")))
            })
    }

    pub(super) async fn resources_validate_availability(
        &self,
        params: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let graph = Self::extract_graph(params)?;
        self.validator
            .validate_availability(&graph)
            .await
            .map_err(|e| JsonRpcError::internal_error(format!("Validation failed: {e}")))
            .and_then(|result| {
                serde_json::to_value(result)
                    .map_err(|e| JsonRpcError::internal_error(format!("Serialization: {e}")))
            })
    }

    pub(super) async fn resources_suggest_optimizations(
        &self,
        params: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let graph = Self::extract_graph(params)?;
        self.optimizer
            .suggest_optimizations(&graph)
            .await
            .map_err(|e| JsonRpcError::internal_error(format!("Optimization failed: {e}")))
            .and_then(|suggestions| {
                serde_json::to_value(suggestions)
                    .map_err(|e| JsonRpcError::internal_error(format!("Serialization: {e}")))
            })
    }
}

#[cfg(test)]
mod tests {
    use crate::graph_types::{ExecutionGraph, GraphNode};
    use crate::pure_jsonrpc::types::{JsonRpcError, JsonRpcErrorExt};

    use super::ResourceHandler;

    fn minimal_graph_params() -> serde_json::Value {
        let graph = ExecutionGraph::builder("res-test")
            .nodes([GraphNode::simple("n1", "cpu_compute")])
            .build();
        serde_json::json!({ "graph": graph })
    }

    #[test]
    fn extract_graph_missing_params_returns_invalid_params() {
        let err = ResourceHandler::extract_graph(None).expect_err("missing params");
        assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
        assert!(err.message.contains("Missing params"));
    }

    #[test]
    fn extract_graph_malformed_graph_returns_invalid_params() {
        let params = serde_json::json!({ "graph": { "id": 123 } });
        let err = ResourceHandler::extract_graph(Some(&params)).expect_err("bad graph");
        assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
        assert!(err.message.contains("Invalid graph parameter"));
    }

    #[test]
    fn extract_graph_valid_params_deserializes_graph() {
        let graph = ResourceHandler::extract_graph(Some(&minimal_graph_params())).expect("graph");
        assert_eq!(graph.id, "res-test");
        assert_eq!(graph.nodes.len(), 1);
    }

    #[tokio::test]
    async fn resources_estimate_missing_params_returns_invalid_params() {
        let handler = ResourceHandler::new();
        let err = handler
            .resources_estimate(None)
            .await
            .expect_err("missing params");
        assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
    }

    #[tokio::test]
    async fn resources_estimate_valid_graph_returns_estimate() {
        let handler = ResourceHandler::new();
        let result = handler
            .resources_estimate(Some(&minimal_graph_params()))
            .await
            .expect("estimate");
        assert!(result.get("cpu_cores").is_some());
    }

    #[tokio::test]
    async fn resources_validate_availability_missing_params_returns_invalid_params() {
        let handler = ResourceHandler::new();
        let err = handler
            .resources_validate_availability(None)
            .await
            .expect_err("missing params");
        assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
    }

    #[tokio::test]
    async fn resources_suggest_optimizations_malformed_graph_returns_invalid_params() {
        let handler = ResourceHandler::new();
        let params = serde_json::json!({ "graph": "not-an-object" });
        let err = handler
            .resources_suggest_optimizations(Some(&params))
            .await
            .expect_err("bad graph");
        assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
    }
}
