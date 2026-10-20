// SPDX-License-Identifier: AGPL-3.0-or-later
//! Biome manifest schema and runtime status types shared by the CLI.
//!
//! The canonical manifest schema lives in [`toadstool_core::manifest::BiomeManifest`].
//! This module's `BiomeManifest` is the CLI's internal representation, with
//! [`From<toadstool_core::manifest::BiomeManifest>`] bridging the two.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use uuid::Uuid;

/// Biome manifest structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BiomeManifest {
    /// Biome metadata
    pub metadata: BiomeMetadata,

    /// Primal configurations
    pub primals: HashMap<String, PrimalConfig>,

    /// Service definitions
    pub services: HashMap<String, ServiceConfig>,

    /// Resource requirements
    pub resources: BiomeResources,

    /// Security policies
    pub security: BiomeSecurity,

    /// Network configuration
    pub networking: BiomeNetworking,

    /// Storage configuration
    pub storage: BiomeStorage,
}

/// Biome manifest metadata (name, version, timestamps)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BiomeMetadata {
    /// Biome display name
    pub name: String,
    /// Semantic version string
    pub version: String,
    /// Optional human-readable description
    pub description: Option<String>,
    /// Optional author or maintainer
    pub author: Option<String>,
    /// Creation timestamp
    #[serde(with = "toadstool_common::system_time_serde")]
    pub created: std::time::SystemTime,
    /// Last update timestamp
    #[serde(with = "toadstool_common::system_time_serde")]
    pub updated: std::time::SystemTime,
    /// Tags for categorization
    pub tags: Vec<String>,
}

/// Configuration for a primal (workload) in the biome manifest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrimalConfig {
    /// Primal version
    pub version: String,
    /// Where to load the workload from (container, wasm, git, etc.)
    pub source: WorkloadSource,
    /// Whether the primal is enabled
    pub enabled: bool,
    /// Arbitrary config key-value pairs
    pub config: HashMap<String, serde_yaml_ng::Value>,
    /// Names of other primals or services this depends on
    pub dependencies: Vec<String>,
    /// Optional health check configuration
    pub health_check: Option<HealthCheckConfig>,
}

impl PrimalConfig {
    /// Returns true if this config declares the given capability.
    ///
    /// Checks the manifest `capabilities` config field (comma-separated labels).
    pub fn has_capability(&self, capability: &str) -> bool {
        let target = capability.to_ascii_lowercase();
        let Some(serde_yaml_ng::Value::String(caps)) = self.config.get("capabilities") else {
            return false;
        };
        caps.split(',')
            .map(str::trim)
            .any(|declared| capability_labels_match(declared, &target))
    }
}

fn capability_labels_match(declared: &str, target: &str) -> bool {
    let declared_lower = declared.to_ascii_lowercase();
    if declared_lower == target {
        return true;
    }
    use toadstool_common::interned_strings::CapabilityDomain;
    CapabilityDomain::from_label(&declared_lower).is_some_and(|domain| domain.as_str() == target)
}

impl BiomeManifest {
    /// Returns true if any primal entry provides the given capability.
    pub fn has_primal_with_capability(&self, capability: &str) -> bool {
        self.primals
            .iter()
            .any(|(name, config)| Self::entry_provides_capability(name, config, capability))
    }

    /// Find the first manifest primal that provides the given capability.
    pub fn find_primal_with_capability(&self, capability: &str) -> Option<(&str, &PrimalConfig)> {
        self.primals.iter().find_map(|(name, config)| {
            Self::entry_provides_capability(name, config, capability)
                .then_some((name.as_str(), config))
        })
    }

    fn entry_provides_capability(name: &str, config: &PrimalConfig, capability: &str) -> bool {
        if config.has_capability(capability) {
            return true;
        }
        let target = capability.to_ascii_lowercase();
        use toadstool_common::interned_strings::CapabilityDomain;
        if CapabilityDomain::from_label(name).is_some_and(|domain| domain.as_str() == target) {
            return true;
        }
        let name_lower = name.to_ascii_lowercase();
        name_lower.contains(&target)
            || (target == "crypto"
                && (name_lower.contains("pki") || name_lower.contains("security")))
    }
}

/// Configuration for a service in the biome manifest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceConfig {
    /// Service version
    pub version: String,
    /// Where to load the service workload from
    pub source: WorkloadSource,
    /// Number of replicas (None = 1)
    pub replicas: Option<u32>,
    /// CPU, memory, storage limits
    pub resources: ServiceResources,
    /// Environment variables
    pub environment: HashMap<String, String>,
    /// Port mappings
    pub ports: Vec<ServicePort>,
    /// Volume mounts
    pub volumes: Vec<ServiceVolume>,
    /// Names of services this depends on
    pub dependencies: Vec<String>,
    /// Optional health check configuration
    pub health_check: Option<HealthCheckConfig>,
}

/// Source type for loading a workload (container, wasm, git, ipfs, local)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WorkloadSource {
    /// OCI container registry
    Container {
        /// Registry host (e.g. docker.io)
        registry: String,
        /// Image name
        image: String,
        /// Image tag
        tag: String,
        /// Optional digest for pinning
        digest: Option<String>,
    },
    /// WebAssembly module
    Wasm {
        /// URL or path to the WASM module
        source: String,
        /// SHA256 or similar checksum for verification
        checksum: String,
        /// Optional WASI runtime config
        wasi_config: Option<HashMap<String, serde_yaml_ng::Value>>,
    },
    /// Git repository
    Git {
        /// Repository URL
        repository: String,
        /// Branch to checkout
        branch: Option<String>,
        /// Commit or tag to pin
        commit: Option<String>,
        /// Subpath within the repo
        path: Option<String>,
    },
    /// IPFS content
    Ipfs {
        /// IPFS CID
        hash: String,
        /// Gateway URL for fetching
        gateway: Option<String>,
    },
    /// Local file path
    Local {
        /// Path to the workload file or directory
        path: PathBuf,
    },
}

/// Resource limits for a biome
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BiomeResources {
    /// CPU cores limit
    pub cpu_limit: Option<f64>,
    /// Memory limit (e.g. "512Mi")
    pub memory_limit: Option<String>,
    /// Storage limit (e.g. "10Gi")
    pub storage_limit: Option<String>,
    /// GPU count limit
    pub gpu_limit: Option<u32>,
    /// Network bandwidth limit
    pub network_bandwidth: Option<String>,
}

/// Security policies for a biome
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BiomeSecurity {
    /// Isolation level (e.g. process, container)
    pub isolation_level: String,
    /// Trust level for workload execution
    pub trust_level: String,
    /// Whether a security/crypto service is required
    #[serde(alias = "beardog_required")]
    pub security_required: bool,
    /// Crypto policy names
    pub crypto_policies: Vec<String>,
    /// Allowed network CIDRs
    pub allowed_networks: Vec<String>,
    /// Syscalls to forbid
    pub forbidden_syscalls: Vec<String>,
}

/// Network configuration for a biome
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BiomeNetworking {
    /// Network mode (bridge, host, none)
    pub mode: String,
    /// DNS server addresses
    pub dns_servers: Vec<String>,
    /// Host-to-container port mappings
    pub port_mappings: Vec<PortMapping>,
    /// Network policy names
    pub network_policies: Vec<String>,
}

/// Storage configuration for a biome
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BiomeStorage {
    /// Storage service integration version or config
    #[serde(alias = "nestgate_integration")]
    pub storage_integration: Option<String>,
    /// Dataset definitions
    pub datasets: Vec<DatasetConfig>,
    /// Volume definitions
    pub volumes: Vec<VolumeConfig>,
    /// Backup policy name or config
    pub backup_policy: Option<String>,
}

/// Resource limits for a single service
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServiceResources {
    /// CPU cores limit
    pub cpu_limit: Option<f64>,
    /// Memory limit (e.g. "256Mi")
    pub memory_limit: Option<String>,
    /// Storage limit
    pub storage_limit: Option<String>,
}

/// Port mapping for a service
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServicePort {
    /// Port inside the container
    pub container_port: u16,
    /// Host port (None = same as container)
    pub host_port: Option<u16>,
    /// Protocol (tcp, udp)
    pub protocol: String,
}

/// Volume mount for a service
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceVolume {
    /// Host path or volume name
    pub source: String,
    /// Mount path inside the container
    pub target: String,
    /// Whether the mount is read-only
    pub read_only: bool,
}

// Canonical HealthCheck (result type) from cellmembrane-types.
// Re-exported for downstream consumers; not directly used in the CLI crate itself.
#[allow(unused_imports)]
pub use cellmembrane_types::HealthCheck;

/// Health check configuration for a service or primal (manifest config type).
///
/// Distinct from `cellmembrane_types::HealthCheck` which is a check *result*.
/// This type configures *how* to run health checks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckConfig {
    /// Command and args to run (e.g. `["curl", "-f", "http://localhost/health"]`)
    pub command: Vec<String>,
    /// Interval between checks in seconds
    pub interval: u64,
    /// Timeout per check in seconds
    pub timeout: u64,
    /// Consecutive failures before unhealthy
    pub retries: u32,
    /// Grace period before first check in seconds
    pub start_period: u64,
}

/// Host-to-container port mapping
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortMapping {
    /// Port on the host
    pub host_port: u16,
    /// Port in the container
    pub container_port: u16,
    /// Protocol (tcp, udp)
    pub protocol: String,
}

/// Dataset configuration for storage service
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetConfig {
    /// Dataset name
    pub name: String,
    /// Size limit (e.g. "10Gi")
    pub size: Option<String>,
    /// Compression algorithm
    pub compression: Option<String>,
    /// Whether to encrypt at rest
    pub encryption: bool,
}

/// Volume driver configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeConfig {
    /// Volume name
    pub name: String,
    /// Storage driver (e.g. local, nfs)
    pub driver: String,
    /// Driver-specific options
    pub options: HashMap<String, String>,
}

/// Running biome information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BiomeInfo {
    /// Unique biome instance ID
    pub id: Uuid,
    /// Biome name from manifest
    pub name: String,
    /// Current lifecycle status
    pub status: BiomeStatus,
    /// When the biome was created
    #[serde(with = "toadstool_common::system_time_serde")]
    pub created: std::time::SystemTime,
    /// When the biome was started (if running)
    #[serde(
        skip_serializing_if = "Option::is_none",
        with = "toadstool_common::system_time_serde::opt"
    )]
    pub started: Option<std::time::SystemTime>,
    /// Path to the biome manifest file
    pub manifest_path: PathBuf,
    /// Current CPU, memory, network usage
    pub resource_usage: ResourceUsage,
    /// Status of each service
    pub services: Vec<ServiceInfo>,
}

/// Lifecycle status of a biome instance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BiomeStatus {
    /// Biome is bootstrapping
    Starting,
    /// Biome is running
    Running,
    /// Biome is shutting down
    Stopping,
    /// Biome has stopped
    Stopped,
    /// Biome failed with error message
    Error(String),
    /// Biome is being migrated
    Migrating,
}

/// Current resource usage for a biome
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUsage {
    /// CPU utilization percentage
    pub cpu_percent: f64,
    /// Memory usage in bytes
    pub memory_bytes: u64,
    /// Storage usage in bytes
    pub storage_bytes: u64,
    /// Network bytes received
    pub network_rx_bytes: u64,
    /// Network bytes transmitted
    pub network_tx_bytes: u64,
}

/// Biome-level service status (display/lifecycle only).
///
/// This is NOT the ecosystem-level `cellmembrane_types::ServiceInfo` used for
/// service discovery and coordination. This struct tracks the status of
/// individual services within a running biome instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInfo {
    /// Service name (key from the biome manifest).
    pub name: String,
    /// Current lifecycle status (e.g. "running", "stopped").
    pub status: String,
    /// Number of running replicas.
    pub replicas: u32,
    /// Exposed ports.
    pub ports: Vec<u16>,
    /// Health status summary (e.g. "healthy", "unhealthy").
    pub health: String,
}

// ---------------------------------------------------------------------------
// Conversion from canonical manifest (toadstool_core::manifest::BiomeManifest)
// ---------------------------------------------------------------------------

impl From<toadstool_core::manifest::BiomeManifest> for BiomeManifest {
    fn from(canonical: toadstool_core::manifest::BiomeManifest) -> Self {
        let now = std::time::SystemTime::now();
        Self {
            metadata: BiomeMetadata {
                name: canonical.metadata.name,
                version: canonical.metadata.version,
                description: canonical.metadata.description,
                author: canonical.metadata.author,
                created: now,
                updated: now,
                tags: canonical.metadata.tags,
            },
            primals: canonical
                .primals
                .into_iter()
                .map(|(name, pc)| {
                    let config = pc
                        .config
                        .into_iter()
                        .map(|(k, v)| {
                            let yaml_val = json_to_yaml(&v);
                            (k, yaml_val)
                        })
                        .collect();
                    (
                        name,
                        PrimalConfig {
                            version: pc.version.unwrap_or_default(),
                            source: pc.source.map(convert_source).unwrap_or(
                                WorkloadSource::Local {
                                    path: PathBuf::from("."),
                                },
                            ),
                            enabled: pc.enabled,
                            config,
                            dependencies: pc.dependencies,
                            health_check: pc.health_check.map(convert_health_check),
                        },
                    )
                })
                .collect(),
            services: canonical
                .services
                .into_iter()
                .map(|(name, sc)| {
                    (
                        name,
                        ServiceConfig {
                            version: sc.version.unwrap_or_default(),
                            source: sc.source.map(convert_source).unwrap_or(
                                WorkloadSource::Local {
                                    path: PathBuf::from("."),
                                },
                            ),
                            replicas: Some(sc.replicas),
                            resources: sc
                                .resources
                                .map(|r| ServiceResources {
                                    cpu_limit: r.cpu_limit,
                                    memory_limit: r.memory_limit,
                                    storage_limit: r.storage_limit,
                                })
                                .unwrap_or_default(),
                            environment: sc.environment,
                            ports: sc
                                .ports
                                .into_iter()
                                .map(|p| ServicePort {
                                    container_port: p.container_port,
                                    host_port: p.host_port,
                                    protocol: p.protocol,
                                })
                                .collect(),
                            volumes: sc
                                .volumes
                                .into_iter()
                                .map(|v| ServiceVolume {
                                    source: v.source,
                                    target: v.target,
                                    read_only: v.read_only,
                                })
                                .collect(),
                            dependencies: sc.dependencies,
                            health_check: sc.health_check.map(convert_health_check),
                        },
                    )
                })
                .collect(),
            resources: canonical
                .resources
                .map(|r| BiomeResources {
                    cpu_limit: r.cpu_limit,
                    memory_limit: r.memory_limit,
                    storage_limit: r.storage_limit,
                    gpu_limit: r.gpu_limit,
                    network_bandwidth: None,
                })
                .unwrap_or(BiomeResources {
                    cpu_limit: None,
                    memory_limit: None,
                    storage_limit: None,
                    gpu_limit: None,
                    network_bandwidth: None,
                }),
            security: canonical
                .security
                .map(|s| BiomeSecurity {
                    isolation_level: s.isolation_level,
                    trust_level: s.trust_level,
                    security_required: s.crypto_required,
                    crypto_policies: s.crypto_policies,
                    allowed_networks: s.allowed_networks,
                    forbidden_syscalls: vec![],
                })
                .unwrap_or(BiomeSecurity {
                    isolation_level: "process".into(),
                    trust_level: "medium".into(),
                    security_required: false,
                    crypto_policies: vec![],
                    allowed_networks: vec![],
                    forbidden_syscalls: vec![],
                }),
            networking: canonical
                .networking
                .map(|n| BiomeNetworking {
                    mode: n.mode,
                    dns_servers: n.dns_servers,
                    port_mappings: n
                        .port_mappings
                        .into_iter()
                        .map(|p| PortMapping {
                            host_port: p.host_port.unwrap_or(p.container_port),
                            container_port: p.container_port,
                            protocol: p.protocol,
                        })
                        .collect(),
                    network_policies: vec![],
                })
                .unwrap_or(BiomeNetworking {
                    mode: "bridge".into(),
                    dns_servers: vec![],
                    port_mappings: vec![],
                    network_policies: vec![],
                }),
            storage: canonical
                .storage
                .map(|s| BiomeStorage {
                    storage_integration: s.integration,
                    datasets: vec![],
                    volumes: s
                        .volumes
                        .into_iter()
                        .map(|v| VolumeConfig {
                            name: v.source,
                            driver: "local".into(),
                            options: HashMap::new(),
                        })
                        .collect(),
                    backup_policy: s.backup_policy,
                })
                .unwrap_or(BiomeStorage {
                    storage_integration: None,
                    datasets: vec![],
                    volumes: vec![],
                    backup_policy: None,
                }),
        }
    }
}

fn convert_source(src: toadstool_core::manifest::ManifestWorkloadSource) -> WorkloadSource {
    use toadstool_core::manifest::ManifestWorkloadSource;
    match src {
        ManifestWorkloadSource::Container {
            image,
            tag,
            registry,
            digest,
        } => WorkloadSource::Container {
            registry: registry.unwrap_or_else(|| "docker.io".into()),
            image,
            tag,
            digest,
        },
        ManifestWorkloadSource::Wasm {
            source,
            checksum,
            wasi_config: _,
        } => WorkloadSource::Wasm {
            source,
            checksum: checksum.unwrap_or_default(),
            wasi_config: None,
        },
        ManifestWorkloadSource::Git {
            repository,
            branch,
            commit,
            path,
        } => WorkloadSource::Git {
            repository,
            branch,
            commit,
            path,
        },
        ManifestWorkloadSource::Native { path, args: _ } => WorkloadSource::Local {
            path: PathBuf::from(path),
        },
        ManifestWorkloadSource::Local { path } => WorkloadSource::Local {
            path: PathBuf::from(path),
        },
    }
}

fn convert_health_check(hc: toadstool_core::manifest::ManifestHealthCheck) -> HealthCheckConfig {
    HealthCheckConfig {
        command: hc.command,
        interval: hc.interval_secs,
        timeout: hc.timeout_secs,
        retries: hc.retries,
        start_period: 0,
    }
}

fn json_to_yaml(v: &serde_json::Value) -> serde_yaml_ng::Value {
    match v {
        serde_json::Value::Null => serde_yaml_ng::Value::Null,
        serde_json::Value::Bool(b) => serde_yaml_ng::Value::Bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                serde_yaml_ng::Value::Number(serde_yaml_ng::Number::from(i))
            } else if let Some(f) = n.as_f64() {
                serde_yaml_ng::Value::Number(serde_yaml_ng::Number::from(f))
            } else {
                serde_yaml_ng::Value::Null
            }
        }
        serde_json::Value::String(s) => serde_yaml_ng::Value::String(s.clone()),
        serde_json::Value::Array(arr) => {
            serde_yaml_ng::Value::Sequence(arr.iter().map(json_to_yaml).collect())
        }
        serde_json::Value::Object(obj) => {
            let map = obj
                .iter()
                .map(|(k, v)| (serde_yaml_ng::Value::String(k.clone()), json_to_yaml(v)))
                .collect();
            serde_yaml_ng::Value::Mapping(map)
        }
    }
}
