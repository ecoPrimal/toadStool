// SPDX-License-Identifier: AGPL-3.0-or-later
//! ToadStool Primal Provider implementation

use std::collections::HashMap;
use std::sync::Arc;

use std::sync::RwLock;
use tracing::{debug, info};

use toadstool_common::constants::{INSTANCE_ID, PRIMAL_NAME};
use toadstool_config::env_config::EnvironmentConfig;

use crate::ToadStoolResult;

use super::requests::{PrimalEndpoints, PrimalRequest, PrimalResponse, ResponseStatus};
use super::traits::UniversalPrimalProvider;
use super::types::{PrimalCapability, PrimalContext, PrimalHealth, PrimalType};

/// `ToadStool` primal provider implementation
#[derive(Clone)]
pub struct ToadStoolPrimalProvider {
    /// Context
    context: PrimalContext,
    /// Health status
    health_status: Arc<RwLock<PrimalHealth>>,
}

impl ToadStoolPrimalProvider {
    /// Create new `ToadStool` primal provider
    #[must_use]
    pub fn new(context: PrimalContext) -> Self {
        Self {
            context,
            health_status: Arc::new(RwLock::new(PrimalHealth::Healthy)),
        }
    }
}

impl UniversalPrimalProvider for ToadStoolPrimalProvider {
    fn primal_id(&self) -> &'static str {
        PRIMAL_NAME
    }

    fn instance_id(&self) -> &'static str {
        INSTANCE_ID
    }

    fn context(&self) -> &PrimalContext {
        &self.context
    }

    fn primal_type(&self) -> PrimalType {
        PrimalType::Compute
    }

    fn capabilities(&self) -> Vec<PrimalCapability> {
        vec![
            PrimalCapability::NativeExecution {
                architectures: vec!["x86_64".to_string(), "aarch64".to_string()],
            },
            PrimalCapability::ContainerRuntime {
                orchestrators: vec!["docker".to_string(), "podman".to_string()],
            },
            PrimalCapability::WasmExecution { wasi_support: true },
            PrimalCapability::ServerlessExecution {
                languages: vec![
                    "rust".to_string(),
                    "python".to_string(),
                    "javascript".to_string(),
                ],
            },
            PrimalCapability::LoadBalancing {
                algorithms: vec!["round_robin".to_string(), "least_connections".to_string()],
            },
            PrimalCapability::AutoScaling {
                metrics: vec![
                    "cpu".to_string(),
                    "memory".to_string(),
                    "requests".to_string(),
                ],
            },
        ]
    }

    fn health_check(&self) -> impl std::future::Future<Output = PrimalHealth> + Send + '_ {
        let health_status = self.health_status.clone();
        async move {
            health_status
                .read()
                .unwrap_or_else(|e| e.into_inner())
                .clone()
        }
    }

    fn endpoints(&self) -> PrimalEndpoints {
        let config = EnvironmentConfig::from_env();
        let host = &config.network.bind_address;
        let port = config.network.toadstool_port;

        PrimalEndpoints {
            primary: format!("http://{host}:{port}"),
            health: format!("http://{host}:{port}/health"),
            metrics: Some(format!("http://{host}:{port}/metrics")),
            admin: Some(format!("http://{host}:{port}/admin")),
            events_endpoint: Some(format!("http://{host}:{port}/events")),
            custom: HashMap::new(),
        }
    }

    fn handle_primal_request(
        &self,
        request: PrimalRequest,
    ) -> impl std::future::Future<Output = ToadStoolResult<PrimalResponse>> + Send + '_ {
        async move {
            debug!("Handling primal request: {:?}", request.request_type);

            Ok(PrimalResponse {
                request_id: request.id,
                status: ResponseStatus::Success,
                payload: serde_json::json!({
                    "message": "Request processed successfully",
                    "request_type": request.request_type
                }),
                metadata: HashMap::new(),
                timestamp: std::time::SystemTime::now(),
            })
        }
    }

    fn initialize(
        &mut self,
        _config: serde_json::Value,
    ) -> impl std::future::Future<Output = ToadStoolResult<()>> + Send + '_ {
        async move {
            info!("ToadStool primal provider initialized");
            Ok(())
        }
    }

    fn shutdown(&mut self) -> impl std::future::Future<Output = ToadStoolResult<()>> + Send + '_ {
        async move {
            info!("ToadStool primal provider shutting down");
            Ok(())
        }
    }

    fn can_serve_context(&self, context: &PrimalContext) -> bool {
        // ToadStool can serve any context with appropriate security level
        context.security_level <= self.context.security_level
    }
}

#[cfg(test)]
mod tests {
    use super::super::types::{NetworkLocation, PrimalContext, PrimalType, SecurityLevel};
    use super::*;

    fn make_context(security_level: SecurityLevel) -> PrimalContext {
        PrimalContext {
            user_id: "test-user".to_string(),
            device_id: "test-device".to_string(),
            session_id: "test-session".to_string(),
            network_location: NetworkLocation {
                ip_address: "127.0.0.1".to_string(),
                subnet: None,
                network_id: None,
                geo_location: None,
            },
            security_level,
            metadata: std::collections::HashMap::new(),
        }
    }

    #[test]
    fn test_provider_new() {
        let context = make_context(SecurityLevel::Standard);
        let provider = ToadStoolPrimalProvider::new(context);
        assert_eq!(provider.context().user_id, "test-user");
        assert_eq!(provider.context().security_level, SecurityLevel::Standard);
    }

    #[test]
    fn test_primal_id() {
        let context = make_context(SecurityLevel::None);
        let provider = ToadStoolPrimalProvider::new(context);
        assert_eq!(provider.primal_id(), "toadstool");
    }

    #[test]
    fn test_instance_id() {
        let context = make_context(SecurityLevel::None);
        let provider = ToadStoolPrimalProvider::new(context);
        assert_eq!(provider.instance_id(), INSTANCE_ID);
    }

    #[test]
    fn test_primal_type() {
        let context = make_context(SecurityLevel::None);
        let provider = ToadStoolPrimalProvider::new(context);
        assert_eq!(provider.primal_type(), PrimalType::Compute);
    }

    #[test]
    fn test_capabilities_non_empty() {
        let context = make_context(SecurityLevel::None);
        let provider = ToadStoolPrimalProvider::new(context);
        let caps = provider.capabilities();
        assert!(!caps.is_empty());
    }

    #[test]
    fn test_can_serve_context_same_level() {
        let context = make_context(SecurityLevel::Standard);
        let provider = ToadStoolPrimalProvider::new(context.clone());
        assert!(provider.can_serve_context(&context));
    }

    #[test]
    fn test_can_serve_context_request_lower() {
        let provider_ctx = make_context(SecurityLevel::High);
        let provider = ToadStoolPrimalProvider::new(provider_ctx);
        let request_ctx = make_context(SecurityLevel::None);
        assert!(provider.can_serve_context(&request_ctx));
    }

    #[test]
    fn test_can_serve_context_request_higher() {
        let provider_ctx = make_context(SecurityLevel::None);
        let provider = ToadStoolPrimalProvider::new(provider_ctx);
        let request_ctx = make_context(SecurityLevel::Maximum);
        assert!(!provider.can_serve_context(&request_ctx));
    }

    #[test]
    fn test_can_serve_context_maximum_provider() {
        let provider_ctx = make_context(SecurityLevel::Maximum);
        let provider = ToadStoolPrimalProvider::new(provider_ctx);
        let request_ctx = make_context(SecurityLevel::None);
        assert!(provider.can_serve_context(&request_ctx));
    }
}
