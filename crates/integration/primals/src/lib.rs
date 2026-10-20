// SPDX-License-Identifier: AGPL-3.0-or-later
#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![expect(
    clippy::must_use_candidate,
    clippy::match_same_arms,
    clippy::doc_markdown,
    reason = "ergonomic trait API; arms share intent; technical identifiers in docs"
)]

//! # Universal Primal Integration Framework
//!
//! This module provides a consistent interface for integrating with all Primals
//! in the ecoPrimals ecosystem. It defines the `PrimalIntegration` trait and
//! common types for universal orchestration from biome.yaml manifests.
//!
//! ## Supported Primals
//!
//! - **`ToadStool`**: Universal Compute Platform
//! - **Coordination**: Network Coordination and Service Mesh
//! - **`Security`**: Security and Authentication
//! - **`Storage`**: Storage and Data Management
//! - **Intelligence**: AI Agents and Model Control Protocol
//! - **biomeOS**: Universal Operating System

use std::future::Future;
use std::pin::Pin;

use toadstool::ToadStoolResult;

#[expect(
    missing_docs,
    reason = "wired in S217; docs will be added incrementally"
)]
pub mod error;
#[expect(
    missing_docs,
    reason = "wired in S217; docs will be added incrementally"
)]
pub mod types;

mod health;
mod integration_manifest;
mod manager;
#[expect(
    missing_docs,
    reason = "wired in S217; docs will be added incrementally"
)]
pub mod manifest;
mod messaging;
mod primal_types;
mod service;

#[expect(
    missing_docs,
    reason = "wired in S217; docs will be added incrementally"
)]
pub mod client;
#[expect(
    missing_docs,
    reason = "wired in S217; docs will be added incrementally"
)]
pub mod orchestrator;
#[expect(
    missing_docs,
    reason = "wired in S217; docs will be added incrementally"
)]
pub mod services;
/// Shader compiler capability discovery — coralReef IPC types.
pub mod shader_compiler;

// Re-exports for backward compatibility - all public types accessible from crate root
pub use health::{HealthCheck, HealthStatus};
pub use integration_manifest::{BiomeManifest, BiomeMetadata};
pub use manager::{
    BootstrapResult, PrimalBootstrapResult, PrimalIntegrationConfig, PrimalIntegrationManager,
};
pub use messaging::{PrimalMessage, PrimalMessageType, PrimalMetrics};
pub use primal_types::{GpuAllocation, PrimalConfig, PrimalResources, PrimalType};
pub use service::{ServiceEndpoint, ServiceRegistration, StartupResult, StartupStatus};

#[cfg(test)]
pub mod mock_primal;

/// Universal trait for Primal integration
///
/// This is the canonical definition of the `PrimalIntegration` trait.
/// All Primals in the ecoPrimals ecosystem should implement this trait.
///
/// Async methods return `Pin<Box<dyn Future<…>>>` (not RPITIT) so the trait stays
/// object-safe for `PrimalIntegrationManager`'s `Box<dyn PrimalIntegration>`.
pub trait PrimalIntegration: Send + Sync {
    /// Initialize the Primal from manifest configuration
    fn initialize_from_manifest(
        &self,
        config: &PrimalConfig,
    ) -> Pin<Box<dyn Future<Output = ToadStoolResult<()>> + Send + '_>>;

    /// Register with orchestrator via capability discovery
    #[cfg(feature = "runtime")]
    fn register_with_orchestrator(
        &self,
        discovery: &dyn toadstool_common::infant_discovery::CapabilityDiscovery,
    ) -> Pin<Box<dyn Future<Output = ToadStoolResult<ServiceRegistration>> + Send + '_>>;

    /// Validate dependencies before startup
    fn validate_dependencies(
        &self,
        manifest: &BiomeManifest,
    ) -> Pin<Box<dyn Future<Output = ToadStoolResult<()>> + Send + '_>>;

    /// Start Primal services
    fn start_services(
        &self,
    ) -> Pin<Box<dyn Future<Output = ToadStoolResult<StartupResult>> + Send + '_>>;

    /// Shutdown Primal services gracefully
    fn shutdown(&self) -> Pin<Box<dyn Future<Output = ToadStoolResult<()>> + Send + '_>>;

    /// Get current health status
    fn get_health_status(
        &self,
    ) -> Pin<Box<dyn Future<Output = ToadStoolResult<HealthStatus>> + Send + '_>>;

    /// Get Primal capabilities
    fn get_capabilities(
        &self,
    ) -> Pin<Box<dyn Future<Output = ToadStoolResult<Vec<String>>> + Send + '_>>;

    /// Handle configuration updates
    fn update_configuration(
        &self,
        config: &PrimalConfig,
    ) -> Pin<Box<dyn Future<Output = ToadStoolResult<()>> + Send + '_>>;

    /// Get metrics and monitoring data
    fn get_metrics(
        &self,
    ) -> Pin<Box<dyn Future<Output = ToadStoolResult<PrimalMetrics>> + Send + '_>>;

    /// Handle inter-Primal communication
    fn handle_primal_message(
        &self,
        message: &PrimalMessage,
    ) -> Pin<Box<dyn Future<Output = ToadStoolResult<PrimalMessage>> + Send + '_>>;
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::mock_primal::MockPrimal;
    use super::*;

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_primal_integration_manager() {
        let mut manager = PrimalIntegrationManager::new(PrimalIntegrationConfig::default());

        let mock_primal = MockPrimal {
            name: "test".to_string(),
            should_fail: false,
        };

        manager.register_primal("test".to_string(), Box::new(mock_primal));

        let manifest = BiomeManifest {
            api_version: "biomeOS/v1".to_string(),
            kind: "Biome".to_string(),
            metadata: BiomeMetadata {
                name: "test-biome".to_string(),
                version: "1.0.0".to_string(),
                description: None,
                author: None,
                team: None,
                environment: None,
                tags: vec![],
                labels: HashMap::new(),
                annotations: HashMap::new(),
            },
            primals: HashMap::new(),
            services: HashMap::new(),
            compositions: vec![],
            resources: None,
            security: None,
            networking: None,
            storage: None,
            agents: None,
            federation: None,
        };

        let mut primals = HashMap::new();
        primals.insert(
            "test".to_string(),
            toadstool_core::manifest::ManifestPrimalConfig {
                version: None,
                enabled: true,
                source: None,
                config: HashMap::new(),
                capabilities: vec!["compute".to_string()],
                dependencies: vec![],
                health_check: None,
                resources: None,
                gossip_events: vec![],
            },
        );
        let manifest = BiomeManifest {
            primals,
            ..manifest
        };

        let result = manager.bootstrap_from_manifest(&manifest).await.unwrap();
        assert_eq!(result.successful_primals, 1);
        assert_eq!(result.total_primals, 1);
    }
}
