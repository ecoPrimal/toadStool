// SPDX-License-Identifier: AGPL-3.0-or-later
//! Test double for [`crate::PrimalIntegration`] — only built for `#[cfg(test)]`.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

use uuid::Uuid;

use crate::{
    BiomeManifest, HealthStatus, PrimalConfig, PrimalIntegration, PrimalMessage, PrimalMetrics,
    ServiceRegistration, StartupResult, StartupStatus,
};
use toadstool::{ToadStoolError, ToadStoolResult};

/// Minimal mock Primal for unit tests.
pub struct MockPrimal {
    /// Logical service name returned in registrations and startup results.
    pub name: String,
    /// When true, `initialize_from_manifest` fails (error-path coverage).
    pub should_fail: bool,
}

impl PrimalIntegration for MockPrimal {
    fn initialize_from_manifest(
        &self,
        _config: &PrimalConfig,
    ) -> Pin<Box<dyn Future<Output = ToadStoolResult<()>> + Send + '_>> {
        let should_fail = self.should_fail;
        Box::pin(async move {
            if should_fail {
                Err(ToadStoolError::runtime("Mock failure".to_string()))
            } else {
                Ok(())
            }
        })
    }

    #[cfg(feature = "runtime")]
    fn register_with_orchestrator(
        &self,
        _discovery: &dyn toadstool_common::infant_discovery::CapabilityDiscovery,
    ) -> Pin<Box<dyn Future<Output = ToadStoolResult<ServiceRegistration>> + Send + '_>> {
        let service_name = self.name.clone();
        Box::pin(async move {
            Ok(ServiceRegistration {
                service_id: Uuid::new_v4(),
                service_name,
                endpoints: vec![],
                metadata: HashMap::new(),
                health_endpoint: None,
            })
        })
    }

    fn validate_dependencies(
        &self,
        _manifest: &BiomeManifest,
    ) -> Pin<Box<dyn Future<Output = ToadStoolResult<()>> + Send + '_>> {
        Box::pin(async { Ok(()) })
    }

    fn start_services(
        &self,
    ) -> Pin<Box<dyn Future<Output = ToadStoolResult<StartupResult>> + Send + '_>> {
        let name = self.name.clone();
        Box::pin(async move {
            Ok(StartupResult {
                duration: std::time::Duration::from_millis(100),
                services_started: vec![name],
                logs: vec![],
                status: StartupStatus::Success,
            })
        })
    }

    fn shutdown(&self) -> Pin<Box<dyn Future<Output = ToadStoolResult<()>> + Send + '_>> {
        Box::pin(async { Ok(()) })
    }

    fn get_health_status(
        &self,
    ) -> Pin<Box<dyn Future<Output = ToadStoolResult<HealthStatus>> + Send + '_>> {
        Box::pin(async {
            Ok(HealthStatus::ok())
        })
    }

    fn get_capabilities(
        &self,
    ) -> Pin<Box<dyn Future<Output = ToadStoolResult<Vec<String>>> + Send + '_>> {
        Box::pin(async { Ok(vec!["test".to_string()]) })
    }

    fn update_configuration(
        &self,
        _config: &PrimalConfig,
    ) -> Pin<Box<dyn Future<Output = ToadStoolResult<()>> + Send + '_>> {
        Box::pin(async { Ok(()) })
    }

    fn get_metrics(
        &self,
    ) -> Pin<Box<dyn Future<Output = ToadStoolResult<PrimalMetrics>> + Send + '_>> {
        Box::pin(async {
            Ok(PrimalMetrics {
                cpu_usage: 0.0,
                memory_usage: 0.0,
                storage_usage: 0.0,
                network_bytes_sent: 0,
                network_bytes_received: 0,
                custom_metrics: HashMap::new(),
                timestamp: std::time::SystemTime::now(),
            })
        })
    }

    fn handle_primal_message(
        &self,
        message: &PrimalMessage,
    ) -> Pin<Box<dyn Future<Output = ToadStoolResult<PrimalMessage>> + Send + '_>> {
        let message = message.clone();
        Box::pin(async move { Ok(message) })
    }
}
