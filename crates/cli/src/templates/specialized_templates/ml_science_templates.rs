// SPDX-License-Identifier: AGPL-3.0-or-later
//! ML/Science domain templates: Science, AI Research, Quantum, Genomics, Vision
//!
//! Data science and research workflow templates that extend the base science template
//! with domain-specific tools (PyTorch, Qiskit, Bioconductor, OpenCV).

use std::collections::HashMap;
use toadstool_config::env_config::EnvironmentConfig;

use super::super::basic_templates::TemplateComponents;
use super::super::constants::{images, resource_sizes};
use crate::{
    DatasetConfig, HealthCheckConfig, PrimalConfig, ServiceConfig, ServicePort, ServiceResources,
    WorkloadSource,
};

/// Create science template with data analysis tools
pub fn create_science_template() -> TemplateComponents {
    use super::super::constants::{registries, service_names, template_names, versions};

    let name = template_names::SCIENCE.to_string();
    let description = "Scientific computing biome with Jupyter and data analysis tools".to_string();

    let (_, _, mut primals, mut services, mut resources, security, networking, mut storage) =
        super::super::basic_templates::create_basic_template();

    // Add storage capability provider for data management
    primals.insert(
        "capability:storage".to_string(),
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
                        super::super::constants::commands::CURL.to_string(),
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

    // Jupyter notebook service
    services.insert(
        service_names::JUPYTER.to_string(),
        ServiceConfig {
            version: versions::LATEST.to_string(),
            source: WorkloadSource::Container {
                registry: registries::DOCKER_HUB.to_string(),
                image: images::JUPYTER_SCIPY.to_string(),
                tag: versions::LATEST.to_string(),
                digest: None,
            },
            replicas: Some(1),
            resources: ServiceResources {
                cpu_limit: Some(8.0),
                memory_limit: Some(resource_sizes::GB_16.to_string()),
                storage_limit: Some(resource_sizes::GB_100.to_string()),
            },
            environment: vec![(
                super::super::constants::env_vars::JUPYTER_ENABLE_LAB.to_string(),
                "yes".to_string(),
            )]
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
            dependencies: vec![
                "capability:pki".to_string(),
                "capability:storage".to_string(),
            ],
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

    // PostgreSQL for data storage
    services.insert(
        "postgres".to_string(),
        ServiceConfig {
            version: "15".to_string(),
            source: WorkloadSource::Container {
                registry: "docker.io".to_string(),
                image: "postgres".to_string(),
                tag: "15-alpine".to_string(),
                digest: None,
            },
            replicas: Some(1),
            resources: ServiceResources {
                cpu_limit: Some(4.0),
                memory_limit: Some("8GB".to_string()),
                storage_limit: Some("200GB".to_string()),
            },
            environment: vec![
                (
                    "POSTGRES_PASSWORD".to_string(),
                    "${DB_PASSWORD:-}".to_string(),
                ),
                ("POSTGRES_DB".to_string(), "research".to_string()),
            ]
            .into_iter()
            .collect(),
            ports: vec![ServicePort {
                container_port: 5432,
                host_port: Some(5432),
                protocol: "tcp".to_string(),
            }],
            volumes: vec![],
            dependencies: vec!["capability:pki".to_string()],
            health_check: Some(HealthCheckConfig {
                command: vec![
                    "pg_isready".to_string(),
                    "-U".to_string(),
                    "postgres".to_string(),
                ],
                interval: 10,
                timeout: 5,
                retries: 5,
                start_period: 30,
            }),
        },
    );

    // Enhanced resources for scientific computing
    resources.cpu_limit = Some(16.0);
    resources.memory_limit = Some("32GB".to_string());
    resources.storage_limit = Some("500GB".to_string());

    // Enable storage service integration
    storage.storage_integration = Some("latest".to_string());
    storage.datasets = vec![
        DatasetConfig {
            name: "research-data".to_string(),
            size: Some("500GB".to_string()),
            compression: Some("zstd".to_string()),
            encryption: true,
        },
        DatasetConfig {
            name: "reference-data".to_string(),
            size: Some("1TB".to_string()),
            compression: Some("zstd".to_string()),
            encryption: false,
        },
    ];

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

/// Create AI research template with GPU support
pub fn create_ai_research_template() -> TemplateComponents {
    use super::super::constants::{registries, template_names, versions};

    let name = template_names::AI_RESEARCH.to_string();
    let description =
        "AI/ML research biome with PyTorch, TensorFlow, and GPU acceleration".to_string();

    let (_, _, primals, mut services, mut resources, security, networking, mut storage) =
        create_science_template();

    // PyTorch environment
    services.insert(
        "pytorch".to_string(),
        ServiceConfig {
            version: versions::LATEST.to_string(),
            source: WorkloadSource::Container {
                registry: registries::DOCKER_HUB.to_string(),
                image: "pytorch/pytorch".to_string(),
                tag: versions::LATEST.to_string(),
                digest: None,
            },
            replicas: Some(1),
            resources: ServiceResources {
                cpu_limit: Some(16.0),
                memory_limit: Some(resource_sizes::GB_64.to_string()),
                storage_limit: Some(resource_sizes::GB_500.to_string()),
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
            dependencies: vec!["capability:pki".to_string()],
            health_check: None,
        },
    );

    // TensorBoard for experiment tracking
    services.insert(
        "tensorboard".to_string(),
        ServiceConfig {
            version: "latest".to_string(),
            source: WorkloadSource::Container {
                registry: "docker.io".to_string(),
                image: "tensorflow/tensorflow".to_string(),
                tag: "latest".to_string(),
                digest: None,
            },
            replicas: Some(1),
            resources: ServiceResources {
                cpu_limit: Some(4.0),
                memory_limit: Some("8GB".to_string()),
                storage_limit: Some("100GB".to_string()),
            },
            environment: HashMap::new(),
            ports: vec![ServicePort {
                container_port: 6006,
                host_port: Some(6006),
                protocol: "tcp".to_string(),
            }],
            volumes: vec![],
            dependencies: vec!["capability:pki".to_string()],
            health_check: None,
        },
    );

    // Massive resources for AI/ML
    resources.cpu_limit = Some(32.0);
    resources.memory_limit = Some("128GB".to_string());
    resources.storage_limit = Some("2TB".to_string());
    resources.gpu_limit = Some(4);

    // Model storage
    storage.datasets.push(DatasetConfig {
        name: "models".to_string(),
        size: Some("2TB".to_string()),
        compression: Some("zstd".to_string()),
        encryption: true,
    });

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

/// Create quantum computing template
pub fn create_quantum_template() -> TemplateComponents {
    use super::super::constants::{registries, template_names, versions};

    let name = template_names::QUANTUM.to_string();
    let description = "Quantum computing research with Qiskit and simulators".to_string();

    let (_, _, primals, mut services, mut resources, security, networking, storage) =
        create_science_template();

    // Qiskit environment
    services.insert(
        "qiskit".to_string(),
        ServiceConfig {
            version: versions::LATEST.to_string(),
            source: WorkloadSource::Container {
                registry: registries::DOCKER_HUB.to_string(),
                image: "qiskit/qiskit".to_string(),
                tag: versions::LATEST.to_string(),
                digest: None,
            },
            replicas: Some(1),
            resources: ServiceResources {
                cpu_limit: Some(32.0),
                memory_limit: Some("128GB".to_string()),
                storage_limit: Some(resource_sizes::GB_500.to_string()),
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
            dependencies: vec!["capability:pki".to_string()],
            health_check: None,
        },
    );

    // Extreme resources for quantum simulation
    resources.cpu_limit = Some(64.0);
    resources.memory_limit = Some("256GB".to_string());
    resources.storage_limit = Some(resource_sizes::TB_1.to_string());

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

/// Create genomics/bioinformatics template
pub fn create_genomics_template() -> TemplateComponents {
    use super::super::constants::{registries, template_names, versions};

    let name = template_names::GENOMICS.to_string();
    let description = "Bioinformatics and genomics analysis with enhanced security".to_string();

    let (_, _, primals, mut services, mut resources, mut security, networking, mut storage) =
        create_science_template();

    // Bioconductor R environment
    services.insert(
        "bioconductor".to_string(),
        ServiceConfig {
            version: versions::LATEST.to_string(),
            source: WorkloadSource::Container {
                registry: registries::DOCKER_HUB.to_string(),
                image: "bioconductor/bioconductor_docker".to_string(),
                tag: versions::LATEST.to_string(),
                digest: None,
            },
            replicas: Some(1),
            resources: ServiceResources {
                cpu_limit: Some(16.0),
                memory_limit: Some(resource_sizes::GB_64.to_string()),
                storage_limit: Some(resource_sizes::TB_1.to_string()),
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
            dependencies: vec![
                "capability:pki".to_string(),
                "capability:storage".to_string(),
            ],
            health_check: None,
        },
    );

    // Enhanced security for genomic data (HIPAA-compliant)
    security.isolation_level = "maximum".to_string();
    security.crypto_policies = vec!["post-quantum".to_string(), "aes-256-gcm".to_string()];

    // Large resources for genomics
    resources.cpu_limit = Some(32.0);
    resources.memory_limit = Some("128GB".to_string());
    resources.storage_limit = Some("2TB".to_string());

    // Encrypted datasets for genomic data
    storage.datasets = vec![
        DatasetConfig {
            name: "reference-genomes".to_string(),
            size: Some(resource_sizes::GB_500.to_string()),
            compression: Some("zstd".to_string()),
            encryption: true,
        },
        DatasetConfig {
            name: "sequencing-data".to_string(),
            size: Some("10TB".to_string()),
            compression: Some("zstd".to_string()),
            encryption: true,
        },
    ];

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

/// Create computer vision template
pub fn create_vision_template() -> TemplateComponents {
    use super::super::constants::{registries, template_names, versions};

    let name = template_names::VISION.to_string();
    let description = "Computer vision and image processing with GPU acceleration".to_string();

    let (_, _, primals, mut services, mut resources, security, networking, storage) =
        create_science_template();

    // OpenCV environment with CUDA
    services.insert(
        "opencv".to_string(),
        ServiceConfig {
            version: versions::LATEST.to_string(),
            source: WorkloadSource::Container {
                registry: registries::DOCKER_HUB.to_string(),
                image: "opencv/opencv".to_string(),
                tag: versions::LATEST.to_string(),
                digest: None,
            },
            replicas: Some(1),
            resources: ServiceResources {
                cpu_limit: Some(8.0),
                memory_limit: Some(resource_sizes::GB_32.to_string()),
                storage_limit: Some(resource_sizes::GB_500.to_string()),
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
            dependencies: vec!["capability:pki".to_string()],
            health_check: None,
        },
    );

    // GPU resources for vision processing
    resources.cpu_limit = Some(16.0);
    resources.memory_limit = Some(resource_sizes::GB_64.to_string());
    resources.storage_limit = Some(resource_sizes::TB_1.to_string());
    resources.gpu_limit = Some(2);

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
