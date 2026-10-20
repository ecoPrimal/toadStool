// SPDX-License-Identifier: AGPL-3.0-or-later
//! Primal Integration Manager and bootstrap logic.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use toadstool::{ToadStoolError, ToadStoolResult};

const DEFAULT_DISCOVERY_TIMEOUT_SECS: u64 = 30;
const DEFAULT_HEALTH_CHECK_INTERVAL_SECS: u64 = 30;
const DEFAULT_RETRY_DELAY_SECS: u64 = 5;

use crate::BiomeManifest;
use crate::PrimalConfig;
use crate::PrimalIntegration;
use crate::health::{HealthCheck, HealthStatus};
use crate::service::StartupStatus;

/// Primal Integration Manager
pub struct PrimalIntegrationManager {
    /// Registered Primals
    primals: HashMap<String, Box<dyn PrimalIntegration + Send + Sync>>,
    /// Configuration
    _config: PrimalIntegrationConfig,
}

/// Configuration for Primal Integration Manager
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrimalIntegrationConfig {
    /// Auto-discovery enabled
    pub auto_discovery: bool,
    /// Discovery timeout
    pub discovery_timeout: Duration,
    /// Health check interval
    pub health_check_interval: Duration,
    /// Maximum retry attempts
    pub max_retry_attempts: u32,
    /// Retry delay
    pub retry_delay: Duration,
}

impl Default for PrimalIntegrationConfig {
    fn default() -> Self {
        Self {
            auto_discovery: true,
            discovery_timeout: Duration::from_secs(DEFAULT_DISCOVERY_TIMEOUT_SECS),
            health_check_interval: Duration::from_secs(DEFAULT_HEALTH_CHECK_INTERVAL_SECS),
            max_retry_attempts: 3,
            retry_delay: Duration::from_secs(DEFAULT_RETRY_DELAY_SECS),
        }
    }
}

impl PrimalIntegrationManager {
    /// Create a new Primal Integration Manager
    #[must_use]
    pub fn new(config: PrimalIntegrationConfig) -> Self {
        Self {
            primals: HashMap::new(),
            _config: config,
        }
    }

    /// Register a Primal implementation
    pub fn register_primal(
        &mut self,
        name: String,
        primal: Box<dyn PrimalIntegration + Send + Sync>,
    ) {
        self.primals.insert(name, primal);
    }

    /// Bootstrap all Primals from manifest
    ///
    /// # Errors
    ///
    /// Returns when dependency validation, startup ordering, or primal initialization fails.
    pub async fn bootstrap_from_manifest(
        &self,
        manifest: &BiomeManifest,
    ) -> ToadStoolResult<BootstrapResult> {
        let start_time = std::time::Instant::now();
        let mut results: HashMap<String, PrimalBootstrapResult> = HashMap::new();

        // Phase 1: Validate all Primal configurations
        for name in manifest.primals.keys() {
            if let Some(primal) = self.primals.get(name)
                && let Err(e) = primal.validate_dependencies(manifest).await
            {
                results.insert(name.clone(), PrimalBootstrapResult::Failed(e.to_string()));
            }
        }

        // Phase 2: Initialize Primals in dependency order
        let startup_order = self.calculate_startup_order(manifest)?;
        for primal_name in &startup_order {
            if let Some(primal) = self.primals.get(primal_name as &str)
                && let Some(config) = manifest.primals.get(primal_name as &str)
            {
                let integration_config = PrimalConfig::from_manifest(primal_name, config);
                match primal.initialize_from_manifest(&integration_config).await {
                    Ok(()) => {
                        results.insert(primal_name.clone(), PrimalBootstrapResult::Success);
                    }
                    Err(e) => {
                        results.insert(
                            primal_name.clone(),
                            PrimalBootstrapResult::Failed(e.to_string()),
                        );
                    }
                }
            }
        }

        // Phase 3: Start services
        for primal_name in &startup_order {
            if let Some(primal) = self.primals.get(primal_name as &str)
                && results.get(primal_name as &str) == Some(&PrimalBootstrapResult::Success)
            {
                match primal.start_services().await {
                    Ok(startup_result) => {
                        if startup_result.status == StartupStatus::Success {
                            results.insert(primal_name.clone(), PrimalBootstrapResult::Running);
                        } else {
                            results.insert(
                                primal_name.clone(),
                                PrimalBootstrapResult::Failed("Service startup failed".to_string()),
                            );
                        }
                    }
                    Err(e) => {
                        results.insert(
                            primal_name.clone(),
                            PrimalBootstrapResult::Failed(e.to_string()),
                        );
                    }
                }
            }
        }

        // Phase 4: Register with Coordination
        // Registration with orchestrator moved to separate phase
        // Each primal will handle its own registration via capability discovery
        for primal_name in &startup_order {
            if results.get(primal_name as &str) == Some(&PrimalBootstrapResult::Running) {
                tracing::info!("Primal {} started successfully", primal_name);
            }
        }

        let successful_primals = results
            .values()
            .filter(|r| matches!(r, PrimalBootstrapResult::Running))
            .count();

        Ok(BootstrapResult {
            duration: start_time.elapsed(),
            results,
            total_primals: manifest.primals.len(),
            successful_primals,
        })
    }

    /// Calculate the startup order based on dependencies
    fn calculate_startup_order(&self, manifest: &BiomeManifest) -> ToadStoolResult<Vec<String>> {
        let mut order = Vec::new();
        let mut visited = HashSet::new();
        let mut visiting = HashSet::new();

        // Topological sort to resolve dependencies
        for primal_name in manifest.primals.keys() {
            if !visited.contains(primal_name) {
                self.visit_primal(
                    primal_name,
                    manifest,
                    &mut visited,
                    &mut visiting,
                    &mut order,
                )?;
            }
        }

        Ok(order)
    }

    /// Visit a Primal during topological sort
    #[expect(
        clippy::self_only_used_in_recursion,
        reason = "recursive DFS traversal: self carries the dependency graph context"
    )]
    fn visit_primal(
        &self,
        primal_name: &str,
        manifest: &BiomeManifest,
        visited: &mut HashSet<String>,
        visiting: &mut HashSet<String>,
        order: &mut Vec<String>,
    ) -> ToadStoolResult<()> {
        if visiting.contains(primal_name) {
            return Err(ToadStoolError::runtime(format!(
                "Circular dependency detected: {primal_name}"
            )));
        }

        if visited.contains(primal_name) {
            return Ok(());
        }

        visiting.insert(primal_name.to_string());

        if let Some(config) = manifest.primals.get(primal_name) {
            for dependency in &config.dependencies {
                self.visit_primal(dependency, manifest, visited, visiting, order)?;
            }
        }

        visiting.remove(primal_name);
        visited.insert(primal_name.to_string());
        order.push(primal_name.to_string());

        Ok(())
    }

    /// Get health status for all Primals
    pub async fn get_all_health_status(&self) -> HashMap<String, HealthStatus> {
        let mut statuses = HashMap::new();

        for (name, primal) in &self.primals {
            match primal.get_health_status().await {
                Ok(status) => {
                    statuses.insert(name.clone(), status);
                }
                Err(e) => {
                    tracing::error!("Failed to get health status for {}: {}", name, e);
                    statuses.insert(
                        name.clone(),
                        HealthStatus::sick(e.to_string())
                            .with_check(HealthCheck {
                                name: "system".to_string(),
                                passed: false,
                                message: Some(e.to_string()),
                                latency_ms: Some(0),
                            }),
                    );
                }
            }
        }

        statuses
    }

    /// Shutdown all Primals gracefully
    ///
    /// # Errors
    ///
    /// Currently always returns `Ok`; individual shutdown failures are logged only.
    pub async fn shutdown_all(&self) -> ToadStoolResult<()> {
        for (name, primal) in &self.primals {
            if let Err(e) = primal.shutdown().await {
                tracing::error!("Failed to shutdown {}: {}", name, e);
            }
        }
        Ok(())
    }
}

/// Result of bootstrapping Primals
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootstrapResult {
    /// Total bootstrap duration
    pub duration: Duration,
    /// Individual Primal results
    pub results: HashMap<String, PrimalBootstrapResult>,
    /// Total number of Primals
    pub total_primals: usize,
    /// Number of successfully started Primals
    pub successful_primals: usize,
}

/// Result of bootstrapping a single Primal
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PrimalBootstrapResult {
    /// Not started
    NotStarted,
    /// Successfully initialized
    Success,
    /// Successfully running
    Running,
    /// Failed with error
    Failed(String),
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::time::Duration;

    use crate::mock_primal::MockPrimal;
    use crate::{BiomeManifest, BiomeMetadata, PrimalIntegrationConfig, PrimalIntegrationManager};
    use toadstool_core::manifest::ManifestPrimalConfig;

    fn empty_manifest(primals: HashMap<String, ManifestPrimalConfig>) -> BiomeManifest {
        BiomeManifest {
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
            primals,
            services: HashMap::new(),
            compositions: vec![],
            resources: None,
            security: None,
            networking: None,
            storage: None,
            agents: None,
            federation: None,
        }
    }

    fn primal_entry(dependencies: Vec<String>) -> ManifestPrimalConfig {
        ManifestPrimalConfig {
            version: None,
            enabled: true,
            source: None,
            config: HashMap::new(),
            capabilities: vec![],
            dependencies,
            health_check: None,
            resources: None,
            gossip_events: vec![],
        }
    }

    #[test]
    fn primal_integration_config_default_values() {
        let c = PrimalIntegrationConfig::default();
        assert!(c.auto_discovery);
        assert_eq!(c.discovery_timeout, Duration::from_secs(30));
        assert_eq!(c.health_check_interval, Duration::from_secs(30));
        assert_eq!(c.max_retry_attempts, 3);
        assert_eq!(c.retry_delay, Duration::from_secs(5));
    }

    #[test]
    fn calculate_startup_order_topological_dependencies() {
        let manager = PrimalIntegrationManager::new(PrimalIntegrationConfig::default());
        let mut primals = HashMap::new();
        primals.insert("worker".to_string(), primal_entry(vec!["base".to_string()]));
        primals.insert("base".to_string(), primal_entry(vec![]));
        let manifest = empty_manifest(primals);
        let order = manager.calculate_startup_order(&manifest).unwrap();
        let base_pos = order.iter().position(|n| n == "base").unwrap();
        let worker_pos = order.iter().position(|n| n == "worker").unwrap();
        assert!(
            base_pos < worker_pos,
            "expected base before worker, got {order:?}"
        );
    }

    #[test]
    fn calculate_startup_order_rejects_cycle() {
        let manager = PrimalIntegrationManager::new(PrimalIntegrationConfig::default());
        let mut primals = HashMap::new();
        primals.insert("a".to_string(), primal_entry(vec!["b".to_string()]));
        primals.insert("b".to_string(), primal_entry(vec!["a".to_string()]));
        let manifest = empty_manifest(primals);
        let err = manager.calculate_startup_order(&manifest).unwrap_err();
        assert!(err.to_string().contains("Circular dependency"), "{err}");
    }

    #[tokio::test]
    async fn register_primal_and_bootstrap_tracks_totals() {
        let mut manager = PrimalIntegrationManager::new(PrimalIntegrationConfig::default());
        manager.register_primal(
            "alpha".to_string(),
            Box::new(MockPrimal {
                name: "alpha".to_string(),
                should_fail: false,
            }),
        );

        let mut primals = HashMap::new();
        primals.insert("alpha".to_string(), primal_entry(vec![]));
        let manifest = empty_manifest(primals);

        let result = manager.bootstrap_from_manifest(&manifest).await.unwrap();
        assert_eq!(result.total_primals, 1);
        assert_eq!(result.successful_primals, 1);
    }
}
