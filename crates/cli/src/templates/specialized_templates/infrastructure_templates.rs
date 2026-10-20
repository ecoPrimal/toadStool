// SPDX-License-Identifier: AGPL-3.0-or-later
//! Infrastructure domain templates: Distributed, Sovereign
//!
//! Cluster orchestration and security-focused templates for multi-node
//! distributed computing and air-gapped sovereign deployments.

use std::collections::HashMap;
use toadstool_config::env_config::EnvironmentConfig;

use super::super::basic_templates::TemplateComponents;
use super::super::constants::{
    commands, registries, resource_sizes, service_names, template_names, versions,
};
use crate::{HealthCheckConfig, PrimalConfig, ServiceConfig, ServiceResources, WorkloadSource};

/// Create distributed computing cluster template
pub fn create_distributed_template() -> TemplateComponents {
    let name = template_names::DISTRIBUTED.to_string();
    let description =
        "Multi-node distributed computing cluster with coordination-service orchestration"
            .to_string();

    let (_, _, mut primals, mut services, mut resources, security, mut networking, mut storage) =
        super::super::basic_templates::create_basic_template();

    // Add discovery capability provider for orchestration
    primals.insert(
        "capability:discovery".to_string(),
        PrimalConfig {
            version: versions::LATEST.to_string(),
            source: WorkloadSource::Container {
                registry: registries::SOVEREIGN_SCIENCE.to_string(),
                image: service_names::COORDINATION.to_string(),
                tag: versions::LATEST.to_string(),
                digest: None,
            },
            enabled: true,
            config: HashMap::new(),
            dependencies: vec!["capability:pki".to_string()],
            health_check: Some({
                let config = EnvironmentConfig::from_env();
                HealthCheckConfig {
                    command: vec![
                        commands::CURL.to_string(),
                        "-f".to_string(),
                        format!(
                            "http://{}:{}/health",
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

    // Add storage capability provider (image from registry; discovered by capability at runtime)
    let storage_key = "capability:storage";
    primals.insert(
        storage_key.to_string(),
        PrimalConfig {
            version: versions::LATEST.to_string(),
            source: WorkloadSource::Container {
                registry: registries::SOVEREIGN_SCIENCE.to_string(),
                image: service_names::STORAGE.to_string(),
                tag: versions::LATEST.to_string(),
                digest: None,
            },
            enabled: true,
            config: HashMap::new(),
            dependencies: vec!["capability:pki".to_string()],
            health_check: Some({
                let config = EnvironmentConfig::from_env();
                HealthCheckConfig {
                    command: vec![
                        commands::CURL.to_string(),
                        "-f".to_string(),
                        format!(
                            "http://{}:{}/health",
                            config.network.bind_address, config.network.storage_port
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

    // Worker nodes
    services.insert(
        "worker".to_string(),
        ServiceConfig {
            version: versions::LATEST.to_string(),
            source: WorkloadSource::Container {
                registry: registries::SOVEREIGN_SCIENCE.to_string(),
                image: "compute-worker".to_string(),
                tag: versions::LATEST.to_string(),
                digest: None,
            },
            replicas: Some(10), // Multiple worker instances
            resources: ServiceResources {
                cpu_limit: Some(8.0),
                memory_limit: Some(resource_sizes::GB_16.to_string()),
                storage_limit: Some(resource_sizes::GB_100.to_string()),
            },
            environment: HashMap::new(),
            ports: vec![],
            volumes: vec![],
            dependencies: vec![
                "capability:pki".to_string(),
                "capability:discovery".to_string(),
            ],
            health_check: Some({
                let config = EnvironmentConfig::from_env();
                HealthCheckConfig {
                    command: vec![
                        commands::CURL.to_string(),
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

    // Cluster resources (scalable)
    resources.cpu_limit = Some(128.0);
    resources.memory_limit = Some("1TB".to_string());
    resources.storage_limit = Some("50TB".to_string());

    // Cluster mesh networking
    networking.mode = "mesh".to_string();
    networking.network_policies = vec!["cluster-internal".to_string()];

    // Distributed storage
    storage.storage_integration = Some(versions::LATEST.to_string());

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

/// Create sovereign/air-gapped template
pub fn create_sovereign_template() -> TemplateComponents {
    let name = template_names::SOVEREIGN.to_string();
    let description =
        "Maximum security sovereign computing with air-gapped configuration".to_string();

    let (_, _, primals, services, mut resources, mut security, mut networking, mut storage) =
        super::super::basic_templates::create_basic_template();

    // Maximum security settings
    security.isolation_level = "maximum".to_string();
    security.trust_level = "sovereign".to_string();
    security.crypto_policies = vec![
        "post-quantum".to_string(),
        "aes-256-gcm".to_string(),
        "ed25519".to_string(),
    ];
    security.allowed_networks = vec!["none".to_string()]; // Air-gapped
    security.forbidden_syscalls = vec![
        "mount".to_string(),
        "reboot".to_string(),
        "network".to_string(),
    ];

    // Air-gapped networking (no external connections)
    networking.mode = "none".to_string();
    networking.dns_servers = vec![];
    networking.network_policies = vec!["deny-all".to_string()];

    // Moderate resources for security-focused workloads
    resources.cpu_limit = Some(8.0);
    resources.memory_limit = Some(resource_sizes::GB_16.to_string());
    resources.storage_limit = Some(resource_sizes::GB_100.to_string());

    // Secure storage with storage capability provider for maximum data protection
    storage.storage_integration = Some(versions::LATEST.to_string());
    storage.backup_policy = Some("encrypted-daily".to_string());

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
