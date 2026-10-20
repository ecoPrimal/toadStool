// SPDX-License-Identifier: AGPL-3.0-or-later
//! Custom template: User-specified configurations
//!
//! Allows users to define custom biome configurations via CustomTemplateSpec.

use std::collections::HashMap;

use super::super::basic_templates::TemplateComponents;
use super::super::constants::{commands, registries, service_names, versions};
use super::super::types_mod::CustomTemplateSpec;
use crate::{
    HealthCheckConfig, PrimalConfig, ServiceConfig, ServicePort, ServiceResources, WorkloadSource,
};

/// Create custom template from user specification
pub fn create_custom_template(spec: &CustomTemplateSpec) -> TemplateComponents {
    let name = format!("{}-biome", spec.name);
    let description = spec.description.clone();

    let (_, _, mut primals, mut services, mut resources, mut security, networking, storage) =
        super::super::basic_templates::create_basic_template();

    // Add requested primals
    for primal_name in &spec.primals {
        if !primals.contains_key(primal_name) {
            primals.insert(
                primal_name.clone(),
                PrimalConfig {
                    version: versions::LATEST.to_string(),
                    source: WorkloadSource::Container {
                        registry: registries::SOVEREIGN_SCIENCE.to_string(),
                        image: primal_name.clone(),
                        tag: versions::LATEST.to_string(),
                        digest: None,
                    },
                    enabled: true,
                    config: HashMap::new(),
                    dependencies: vec![service_names::CRYPTO.to_string()],
                    health_check: Some(HealthCheckConfig {
                        command: vec![primal_name.clone(), commands::HEALTH.to_string()],
                        interval: 30,
                        timeout: 10,
                        retries: 3,
                        start_period: 60,
                    }),
                },
            );
        }
    }

    // Add custom services
    for service_spec in &spec.services {
        services.insert(
            service_spec.name.clone(),
            ServiceConfig {
                version: "latest".to_string(),
                source: WorkloadSource::Container {
                    registry: "docker.io".to_string(),
                    image: service_spec.image.clone(),
                    tag: "latest".to_string(),
                    digest: None,
                },
                replicas: Some(1),
                resources: ServiceResources {
                    cpu_limit: Some(4.0),
                    memory_limit: Some("8GB".to_string()),
                    storage_limit: Some("50GB".to_string()),
                },
                environment: service_spec.environment.clone(),
                ports: service_spec
                    .ports
                    .iter()
                    .map(|&port| ServicePort {
                        container_port: port,
                        host_port: Some(port),
                        protocol: "tcp".to_string(),
                    })
                    .collect(),
                volumes: vec![],
                dependencies: vec!["capability:pki".to_string()],
                health_check: None,
            },
        );
    }

    // Apply resource profile
    match spec.resource_profile.as_str() {
        "low" => {
            resources.cpu_limit = Some(4.0);
            resources.memory_limit = Some("8GB".to_string());
            resources.storage_limit = Some("50GB".to_string());
        }
        "high" => {
            resources.cpu_limit = Some(32.0);
            resources.memory_limit = Some("128GB".to_string());
            resources.storage_limit = Some("2TB".to_string());
        }
        _ => {
            // Medium (default)
            resources.cpu_limit = Some(16.0);
            resources.memory_limit = Some("32GB".to_string());
            resources.storage_limit = Some("500GB".to_string());
        }
    }

    // Apply security level
    security.isolation_level = spec.security_level.clone();

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
