// SPDX-License-Identifier: AGPL-3.0-or-later
#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![expect(
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation,
    clippy::missing_errors_doc,
    reason = "container runtime: numeric casts for resource limits; pedantic lints suppressed crate-wide"
)]

//! # `ToadStool` Container Runtime Engine
//!
//! High-performance container runtime engine with Docker, Containerd, and Podman support,
//! comprehensive security isolation, resource limits, and network policies.

// Module declarations
pub mod constants;
pub mod types;

mod byob_routes;
mod docker;
mod engine;
pub use byob_routes::{
    ByobApi, JsonRpcError, JsonRpcErrorExt, JsonRpcRequest, JsonRpcResponse, JsonRpcResponseExt,
};
pub mod byob_server;

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::RwLock;
use std::time::Instant;
use uuid::Uuid;

#[cfg(feature = "docker")]
use bollard::Docker;

use toadstool::resources::ResourceMonitorDispatch;
use toadstool::{RuntimeCapabilities, ToadStoolResult, WorkloadType};

// Re-export types for backward compatibility
pub use types::{
    ContainerEngineType, ContainerExecutionConfig, ContainerResourceLimits, ContainerResources,
    ContainerRuntimeConfig, ContainerSecurity, ContainerSecurityConfig, DnsConfig, ImageConfig,
    ImagePullPolicy, NetworkMode, NetworkPolicy, PortRange, RegistryConfig, VolumePolicy,
};

/// Active container handle
#[derive(Clone, Debug)]
#[cfg_attr(
    not(feature = "docker"),
    expect(dead_code, reason = "stub for non-docker builds")
)]
pub(crate) struct ContainerHandle {
    pub(crate) container_id: String,
    _image: String,
    _start_time: Instant,
    _config: ContainerRuntimeConfig,
}

/// Container runtime engine implementation
pub struct ContainerRuntimeEngine {
    config: ContainerRuntimeConfig,
    #[cfg(feature = "docker")]
    docker: Option<Docker>,
    #[cfg(not(feature = "docker"))]
    #[expect(dead_code, reason = "stub for non-docker builds")]
    docker: Option<()>,
    active_containers: Arc<RwLock<HashMap<Uuid, ContainerHandle>>>,
    resource_monitor: Option<Arc<ResourceMonitorDispatch>>,
    capabilities: RuntimeCapabilities,
}

impl std::fmt::Debug for ContainerRuntimeEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ContainerRuntimeEngine")
            .field("config", &self.config)
            .field("docker", &"<Docker>")
            .field("active_containers", &"<HashMap<Uuid, ContainerHandle>>")
            .field("resource_monitor", &"<Option<ResourceMonitor>>")
            .field("capabilities", &self.capabilities)
            .finish()
    }
}

impl ContainerRuntimeEngine {
    /// Create a new container runtime engine with default configuration
    pub fn new() -> ToadStoolResult<Self> {
        let config = ContainerRuntimeConfig::default();
        Self::with_config(config)
    }

    /// Create a new container runtime engine with custom configuration
    pub fn with_config(config: ContainerRuntimeConfig) -> ToadStoolResult<Self> {
        let docker = docker::create_docker_client(&config)?;

        let capabilities = RuntimeCapabilities {
            supported_workloads: vec![WorkloadType::Container],
            max_concurrent_executions: Some(100),
            supported_architectures: vec!["linux/amd64".to_string(), "linux/arm64".to_string()],
            platform_features: {
                let mut features = HashMap::new();
                features.insert("docker_support".to_string(), docker.is_some());
                features.insert("volume_mounts".to_string(), true);
                features.insert("network_isolation".to_string(), true);
                features
            },
            version: env!("CARGO_PKG_VERSION").to_string(),
        };

        Ok(Self {
            config,
            docker,
            active_containers: Arc::new(RwLock::new(HashMap::new())),
            resource_monitor: None,
            capabilities,
        })
    }

    /// Add a resource monitor to the engine
    #[must_use]
    pub fn with_resource_monitor(mut self, monitor: Arc<ResourceMonitorDispatch>) -> Self {
        self.resource_monitor = Some(monitor);
        self
    }
}

impl Default for ContainerRuntimeEngine {
    fn default() -> Self {
        Self::new().unwrap_or_else(|_| {
            // Return a minimal engine configuration if creation fails
            Self {
                config: ContainerRuntimeConfig::default(),
                docker: None,
                active_containers: Arc::new(RwLock::new(HashMap::new())),
                resource_monitor: None,
                capabilities: RuntimeCapabilities {
                    supported_workloads: vec![],
                    max_concurrent_executions: Some(0),
                    supported_architectures: vec!["x86_64".to_string(), "aarch64".to_string()],
                    platform_features: HashMap::new(),
                    version: "1.0.0".to_string(),
                },
            }
        })
    }
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
