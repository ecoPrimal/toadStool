// SPDX-License-Identifier: AGPL-3.0-or-later
//! Basic and development template implementations
//!
//! This module contains the foundational biome templates:
//! - `create_basic_template()`: Minimal universal compute biome with BearDog
//! - `create_development_template()`: Development environment with enhanced tooling
//!
//! These templates form the basis for all specialized templates.
//!
//! ⚠️ **MIGRATION NOTICE**: Uses deprecated hardcoded ports during transition to capability-based discovery.

use std::collections::HashMap;
use toadstool_config::env_config::EnvironmentConfig;

use super::constants::service_names;

use crate::{
    BiomeNetworking, BiomeResources, BiomeSecurity, BiomeStorage, HealthCheckConfig, PrimalConfig,
    ServiceConfig, ServicePort, ServiceResources, WorkloadSource,
};

/// Template return type for consistency
pub type TemplateComponents = (
    String,                         // name
    String,                         // description
    HashMap<String, PrimalConfig>,  // primals
    HashMap<String, ServiceConfig>, // services
    BiomeResources,                 // resources
    BiomeSecurity,                  // security
    BiomeNetworking,                // networking
    BiomeStorage,                   // storage
);

/// Create basic template with essential services
pub fn create_basic_template() -> TemplateComponents {
    let name = "basic-biome".to_string();
    let description = "Basic universal compute biome with essential services".to_string();

    // Essential primals — keyed by capability, not primal name.
    // The container image is a *default* implementation; at runtime, the
    // deployment engine resolves the actual provider via capability discovery.
    let mut primals = HashMap::new();
    primals.insert(
        "pki-provider".to_string(), // Capability-based key
        PrimalConfig {
            version: "latest".to_string(),
            source: WorkloadSource::Container {
                registry: "registry.ecosystem.sovereignscience.org".to_string(),
                image: service_names::CRYPTO.to_string(), // Default implementation for PKI capability
                tag: "latest".to_string(),
                digest: None,
            },
            enabled: true,
            config: {
                let mut config = HashMap::new();
                config.insert(
                    "capabilities".to_string(),
                    serde_yaml_ng::Value::String("pki,secrets,auth".to_string()),
                );
                config
            },
            dependencies: vec![],
            health_check: Some(HealthCheckConfig {
                command: vec!["health-check".to_string()], // Generic health check
                interval: 30,
                timeout: 10,
                retries: 3,
                start_period: 60,
            }),
        },
    );

    // Basic services
    let mut services = HashMap::new();
    services.insert(
        "compute".to_string(),
        ServiceConfig {
            version: "latest".to_string(),
            source: WorkloadSource::Container {
                registry: "registry.ecosystem.sovereignscience.org".to_string(),
                image: "universal-compute".to_string(),
                tag: "latest".to_string(),
                digest: None,
            },
            replicas: Some(1),
            resources: ServiceResources {
                cpu_limit: Some(2.0),
                memory_limit: Some("4GB".to_string()),
                storage_limit: Some("10GB".to_string()),
            },
            environment: HashMap::new(),
            ports: vec![{
                let config = EnvironmentConfig::from_env();
                ServicePort {
                    container_port: config.network.coordination_port,
                    host_port: Some(config.network.coordination_port),
                    protocol: "tcp".to_string(),
                }
            }],
            volumes: vec![],
            // Capability-based dependencies - orchestrator resolves to available providers
            dependencies: vec!["capability:pki".to_string()],
            health_check: Some({
                let config = EnvironmentConfig::from_env();
                HealthCheckConfig {
                    command: vec![
                        "curl".to_string(),
                        "-f".to_string(),
                        format!(
                            "http://{}:{}/health",
                            config.network.bind_address, config.network.coordination_port
                        ),
                    ],
                    interval: 30,
                    timeout: 10,
                    retries: 3,
                    start_period: 30,
                }
            }),
        },
    );

    let resources = BiomeResources {
        cpu_limit: Some(4.0),
        memory_limit: Some("8GB".to_string()),
        storage_limit: Some("50GB".to_string()),
        gpu_limit: None,
        network_bandwidth: Some("1Gbps".to_string()),
    };

    let security = BiomeSecurity {
        isolation_level: "high".to_string(),
        trust_level: "verified".to_string(),
        security_required: true,
        crypto_policies: vec!["default".to_string()],
        allowed_networks: vec!["private".to_string()],
        forbidden_syscalls: vec!["mount".to_string(), "reboot".to_string()],
    };

    let networking = BiomeNetworking {
        mode: "bridge".to_string(),
        // Empty: inherit from the host or configure per-deployment via TOADSTOOL_DNS_SERVERS.
        dns_servers: std::env::var(
            toadstool_common::interned_strings::socket_env::TOADSTOOL_DNS_SERVERS,
        )
        .ok()
        .map(|v| v.split(',').map(str::trim).map(String::from).collect())
        .unwrap_or_default(),
        port_mappings: vec![],
        network_policies: vec!["default-deny".to_string()],
    };

    let storage = BiomeStorage {
        storage_integration: None,
        datasets: vec![],
        volumes: vec![],
        backup_policy: None,
    };

    (
        name,
        description,
        primals,
        services,
        resources,
        security,
        networking,
        storage,
    )
}

/// Create development template with debugging tools
pub fn create_development_template() -> TemplateComponents {
    let name = "dev-biome".to_string();
    let description = "Development and testing environment with debugging tools".to_string();

    let (_, _, primals, mut services, resources, mut security, networking, storage) =
        create_basic_template();

    // Development tools
    services.insert(
        "vscode-server".to_string(),
        ServiceConfig {
            version: "latest".to_string(),
            source: WorkloadSource::Container {
                registry: "docker.io".to_string(),
                image: "codercom/code-server".to_string(),
                tag: "latest".to_string(),
                digest: None,
            },
            replicas: Some(1),
            resources: ServiceResources {
                cpu_limit: Some(4.0),
                memory_limit: Some("8GB".to_string()),
                storage_limit: Some("50GB".to_string()),
            },
            environment: vec![("PASSWORD".to_string(), "${APP_PASSWORD:-}".to_string())]
                .into_iter()
                .collect(),
            ports: vec![{
                let config = EnvironmentConfig::from_env();
                ServicePort {
                    container_port: config.network.coordination_port,
                    host_port: Some(config.network.coordination_port),
                    protocol: "tcp".to_string(),
                }
            }],
            volumes: vec![],
            // Capability-based dependencies - orchestrator resolves to available providers
            dependencies: vec!["capability:pki".to_string()],
            health_check: Some({
                let config = EnvironmentConfig::from_env();
                HealthCheckConfig {
                    command: vec![
                        "curl".to_string(),
                        "-f".to_string(),
                        format!(
                            "http://{}:{}",
                            config.network.bind_address, config.network.coordination_port
                        ),
                    ],
                    interval: 30,
                    timeout: 10,
                    retries: 3,
                    start_period: 60,
                }
            }),
        },
    );

    // Relaxed security for development
    security.isolation_level = "medium".to_string();
    security.trust_level = "development".to_string();

    (
        name,
        description,
        primals,
        services,
        resources,
        security,
        networking,
        storage,
    )
}
