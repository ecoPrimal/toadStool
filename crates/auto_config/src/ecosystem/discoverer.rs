// SPDX-License-Identifier: AGPL-3.0-or-later
//! [`EcosystemDiscoverer`]: capability-based service patterns and parallel discovery.

use std::collections::HashMap;
use std::time::Duration;

use tracing::{debug, info};

use crate::ecosystem::constants::{capability_keys, wellknown_hosts};
use crate::ecosystem::helpers::{assemble_discovered_services, get_capability_endpoint};
#[cfg(feature = "network-scan")]
use crate::ecosystem_network::{discover_network_services, probe_service};

#[cfg(not(feature = "network-scan"))]
async fn discover_network_services(
    _service_patterns: &HashMap<String, ServicePattern>,
) -> ToadStoolResult<HashMap<String, ServiceInfo>> {
    Ok(HashMap::new())
}

#[cfg(not(feature = "network-scan"))]
async fn probe_service(_endpoint: &str, _pattern: &ServicePattern) -> ToadStoolResult<ServiceInfo> {
    Err(ToadStoolError::network(
        "TCP service probing requires the `network-scan` feature".to_string(),
    ))
}
use crate::ecosystem_types::{
    DiscoveredServices, DiscoverySummary, ServiceInfo, ServicePattern, ServiceType,
};
use crate::{ToadStoolError, ToadStoolResult};
use toadstool_common::interned_strings::socket_env;
use toadstool_config::env_config::EnvironmentConfig;

/// Ecosystem discovery system for finding and configuring primal services
pub struct EcosystemDiscoverer {
    /// Capability patterns (keyed by capability, not primal name)
    service_patterns: HashMap<String, ServicePattern>,
    /// Discovery timeout
    _discovery_timeout: Duration,
    /// Last discovery results (cached)
    last_discovery: Option<DiscoveredServices>,
}

impl EcosystemDiscoverer {
    const DEFAULT_TIMEOUT_SECS: u64 = 30;

    /// Create a new ecosystem discoverer (capability-based)
    #[must_use]
    pub fn new() -> Self {
        let mut service_patterns = HashMap::new();
        let config = EnvironmentConfig::from_env();

        // Discovery capability (network coordination)
        service_patterns.insert(
            capability_keys::DISCOVERY.to_string(),
            ServicePattern {
                name: capability_keys::DISCOVERY.to_string(),
                description: "Service discovery and coordination".to_string(),
                default_ports: vec![config.network.coordination_port],
                health_endpoints: vec!["/health".to_string(), "/api/health".to_string()],
                service_type: ServiceType::NetworkCoordination,
                required_capabilities: vec!["network".to_string(), "coordination".to_string()],
            },
        );

        // Crypto capability (security)
        service_patterns.insert(
            capability_keys::CRYPTO.to_string(),
            ServicePattern {
                name: capability_keys::CRYPTO.to_string(),
                description: "Cryptographic operations and security".to_string(),
                default_ports: vec![config.network.security_port],
                health_endpoints: vec!["/health".to_string(), "/api/security/health".to_string()],
                service_type: ServiceType::Security,
                required_capabilities: vec!["security".to_string(), "authentication".to_string()],
            },
        );

        // Storage capability
        service_patterns.insert(
            capability_keys::STORAGE.to_string(),
            ServicePattern {
                name: capability_keys::STORAGE.to_string(),
                description: "Distributed storage and data management".to_string(),
                default_ports: vec![config.network.storage_port],
                health_endpoints: vec!["/health".to_string(), "/api/storage/health".to_string()],
                service_type: ServiceType::Storage,
                required_capabilities: vec!["storage".to_string(), "data_management".to_string()],
            },
        );

        // Compute capability (AI/ML)
        service_patterns.insert(
            capability_keys::COMPUTE.to_string(),
            ServicePattern {
                name: capability_keys::COMPUTE.to_string(),
                description: "AI and machine learning services".to_string(),
                default_ports: vec![config.network.ai_processing_port],
                health_endpoints: vec!["/health".to_string(), "/api/ai/health".to_string()],
                service_type: ServiceType::AI,
                required_capabilities: vec!["ai".to_string(), "machine_learning".to_string()],
            },
        );

        // Orchestration capability (platform)
        service_patterns.insert(
            capability_keys::ORCHESTRATION.to_string(),
            ServicePattern {
                name: capability_keys::ORCHESTRATION.to_string(),
                description: "Platform orchestration and environment management".to_string(),
                default_ports: vec![config.network.biomeos_port],
                health_endpoints: vec!["/health".to_string(), "/api/biome/health".to_string()],
                service_type: ServiceType::OperatingSystem,
                required_capabilities: vec!["os_management".to_string(), "environment".to_string()],
            },
        );

        // Self-identity (ToadStool - the only known primal)
        service_patterns.insert(
            capability_keys::SELF.to_string(),
            ServicePattern {
                name: toadstool_common::constants::primal_identity::PRIMAL_NAME.to_string(),
                description: "Universal compute instances".to_string(),
                default_ports: vec![config.network.toadstool_port],
                health_endpoints: vec!["/health".to_string(), "/jsonrpc".to_string()],
                service_type: ServiceType::Compute,
                required_capabilities: vec![
                    "compute".to_string(),
                    "universal_execution".to_string(),
                ],
            },
        );

        Self {
            service_patterns,
            _discovery_timeout: Duration::from_secs(Self::DEFAULT_TIMEOUT_SECS),
            last_discovery: None,
        }
    }

    /// Look up a service pattern by capability name (sovereignty-compliant).
    ///
    /// Returns the first `ServicePattern` whose `required_capabilities` contains
    /// the given capability, or `None`.
    #[must_use]
    pub fn find_pattern_by_capability(&self, capability: &str) -> Option<&ServicePattern> {
        self.service_patterns
            .values()
            .find(|p| p.required_capabilities.iter().any(|c| c == capability))
    }

    /// Discover all available ecosystem services
    ///
    /// ✅ EVOLUTION: Fast, concurrent, test-aware discovery
    ///
    /// # Errors
    /// Returns a `ToadStoolError` if network scanning fails or service
    /// discovery encounters errors.
    #[must_use = "Service discovery result should be checked"]
    pub async fn discover_services(&mut self) -> ToadStoolResult<DiscoveredServices> {
        // ✅ DEEP DEBT SOLUTION: Skip slow network I/O in test/CI environments
        let is_test = std::thread::current()
            .name()
            .is_some_and(|n| n.contains("test"))
            || cfg!(test)
            || std::env::var(socket_env::CI).is_ok()
            || std::env::var(socket_env::TOADSTOOL_SKIP_DISCOVERY).is_ok();

        if is_test {
            debug!("⚡ Fast mode: Skipping network discovery (test/CI environment)");
            let services = DiscoveredServices {
                discovered_services: HashMap::new(),
                discovery_summary: DiscoverySummary {
                    total_services_found: 0,
                    discovery_methods_used: vec!["fast_mode".to_string()],
                    services_by_type: HashMap::new(),
                    discovery_errors: Vec::new(),
                },
                discovery_timestamp: std::time::SystemTime::now(),
            };
            self.last_discovery = Some(services.clone());
            return Ok(services);
        }

        self.run_full_ecosystem_discovery().await
    }

    /// Full parallel discovery (local, network scan, well-known hosts, mDNS placeholder).
    async fn run_full_ecosystem_discovery(&mut self) -> ToadStoolResult<DiscoveredServices> {
        info!("🌐 Starting ecosystem service discovery...");

        let (local_result, network_result, wellknown_result, mdns_result) = tokio::join!(
            self.discover_local_services(),
            discover_network_services(&self.service_patterns),
            self.discover_wellknown_services(),
            async { Ok::<_, ToadStoolError>(Self::discover_mdns_services()) },
        );

        let services = assemble_discovered_services(
            local_result,
            network_result,
            wellknown_result,
            mdns_result,
        );

        self.last_discovery = Some(services.clone());

        info!("✅ Ecosystem discovery complete:");
        for (name, service) in &services.discovered_services {
            info!(
                "   🔗 {} -> {:?}",
                name, service.endpoints
            );
        }

        Ok(services)
    }

    /// Discover services on localhost and common local IPs
    pub(crate) async fn discover_local_services(
        &self,
    ) -> ToadStoolResult<HashMap<String, ServiceInfo>> {
        if cfg!(test) {
            debug!("Skipping local network probing in test mode");
            return Ok(HashMap::new());
        }

        let mut services = HashMap::new();

        for (capability_key, pattern) in &self.service_patterns {
            // Capability-based env fallbacks (not primal names). Primals discover each other at runtime.
            let legacy_keys: Vec<&str> = match capability_key.as_str() {
                capability_keys::DISCOVERY => vec!["COORDINATION"],
                capability_keys::CRYPTO => vec!["CRYPTO", "PKI"],
                capability_keys::STORAGE => vec!["STORAGE", "ARTIFACT"],
                capability_keys::COMPUTE => vec!["COMPUTE"],
                capability_keys::ORCHESTRATION => vec!["ORCHESTRATION"],
                capability_keys::SELF => vec!["SELF"],
                _ => vec![],
            };
            if let Some(endpoint) = get_capability_endpoint(capability_key, &legacy_keys) {
                debug!("Using {} from environment: {}", capability_key, endpoint);
                if let Ok(service_info) = probe_service(&endpoint, pattern).await {
                    services.insert(capability_key.clone(), service_info);
                }
            }
        }

        let config = EnvironmentConfig::from_env();
        let discovery_bind_fallback = std::env::var(socket_env::TOADSTOOL_DISCOVERY_BIND_ADDR)
            .unwrap_or_else(|_| {
                toadstool_config::defaults::network::BIND_ADDRESS_DEFAULT.to_owned()
            });
        let local_ips = vec![
            config.network.bind_address.clone(),
            toadstool_common::constants::network::LOCALHOST_IPV4.to_string(),
            discovery_bind_fallback,
        ];

        for ip in local_ips {
            for (capability_key, pattern) in &self.service_patterns {
                if services.contains_key(capability_key) {
                    continue;
                }

                for &port in &pattern.default_ports {
                    let endpoint = format!("http://{ip}:{port}");

                    if let Ok(service_info) = probe_service(&endpoint, pattern).await {
                        debug!("Found {} capability at {}", capability_key, endpoint);
                        services.insert(format!("{capability_key}_{port}"), service_info);
                    }
                }
            }
        }

        debug!("Local discovery found {} services", services.len());
        Ok(services)
    }

    /// Discover services on well-known ports
    pub(crate) async fn discover_wellknown_services(
        &self,
    ) -> ToadStoolResult<HashMap<String, ServiceInfo>> {
        if cfg!(test) {
            debug!("Skipping well-known host probing in test mode");
            return Ok(HashMap::new());
        }

        let mut services = HashMap::new();

        for host in wellknown_hosts::ALL {
            for (capability_key, pattern) in &self.service_patterns {
                for &port in &pattern.default_ports {
                    let endpoint = format!("http://{host}:{port}");

                    if let Ok(service_info) = probe_service(&endpoint, pattern).await {
                        debug!("Found {} capability at {}", capability_key, endpoint);
                        services.insert(format!("{capability_key}_{host}"), service_info);
                    }
                }
            }
        }

        debug!("Well-known discovery found {} services", services.len());
        Ok(services)
    }

    /// Discover services using mDNS/Zeroconf.
    ///
    /// Delegates to `toadstool_common::primal_integration::try_discover_via_mdns`
    /// which probes `_toadstool._tcp.local.` and filters by capability TXT records.
    pub(crate) fn discover_mdns_services() -> HashMap<String, ServiceInfo> {
        use toadstool_common::primal_integration::try_discover_via_mdns;

        if cfg!(test) {
            debug!("Skipping mDNS probing in test mode");
            return HashMap::new();
        }

        let mut services = HashMap::new();

        let capability_keys = ["discovery", "crypto", "storage", "compute", "orchestration"];

        for capability in capability_keys {
            debug!("Probing mDNS for capability: {}", capability);
            if let Some(endpoints) = try_discover_via_mdns(capability) {
                for endpoint in endpoints {
                    let status = if endpoint.healthy {
                        crate::ecosystem_types::ServiceStatus::Running
                    } else {
                        crate::ecosystem_types::ServiceStatus::Unknown
                    };
                    let mut si = ServiceInfo::new(&endpoint.service_id, &endpoint.service_id)
                        .with_endpoint(endpoint.url);
                    si.capabilities = endpoint.capabilities;
                    si.status = status;
                    si.metadata.insert("discovered_via".into(), "mdns".into());
                    si.metadata.insert("service_type".into(), capability.to_string());
                    services.insert(endpoint.service_id.clone(), si);
                }
            }
        }

        debug!("mDNS discovery found {} services", services.len());
        services
    }

    /// Get the last discovery results (cached)
    #[must_use]
    pub const fn get_last_discovery(&self) -> Option<&DiscoveredServices> {
        self.last_discovery.as_ref()
    }

    /// Clear the discovery cache
    pub fn clear_cache(&mut self) {
        self.last_discovery = None;
    }

    /// Insert an extra pattern (used by unit tests to exercise the `match` fallback arm in local discovery).
    #[cfg(test)]
    pub(crate) fn insert_service_pattern_for_test(&mut self, key: String, pattern: ServicePattern) {
        self.service_patterns.insert(key, pattern);
    }

    /// Test-only access to registered patterns (sibling `tests` module cannot access private fields).
    #[cfg(test)]
    pub(crate) fn service_patterns(&self) -> &HashMap<String, ServicePattern> {
        &self.service_patterns
    }
}

impl Default for EcosystemDiscoverer {
    fn default() -> Self {
        Self::new()
    }
}
