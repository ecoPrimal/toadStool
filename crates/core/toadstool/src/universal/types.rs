// SPDX-License-Identifier: AGPL-3.0-or-later
//! Core universal types for primal operations

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// Canonical SecurityLevel from cellmembrane-types.
// Local enum removed — was (Basic, Standard, High, Maximum).
// Mapping: Basic→None, Standard→Standard, High→High, Maximum→Maximum.
pub use cellmembrane_types::SecurityLevel;

/// Network location information
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NetworkLocation {
    /// IP address
    pub ip_address: String,
    /// Subnet
    pub subnet: Option<String>,
    /// Network identifier
    pub network_id: Option<String>,
    /// Geographic location
    pub geo_location: Option<String>,
}

/// Context for user/device-specific primal routing
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrimalContext {
    /// User identifier
    pub user_id: String,
    /// Device identifier
    pub device_id: String,
    /// Session identifier
    pub session_id: String,
    /// Network location
    pub network_location: NetworkLocation,
    /// Security level required
    pub security_level: SecurityLevel,
    /// Additional context metadata
    pub metadata: HashMap<String, String>,
}

/// Primal type categories
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PrimalType {
    /// Compute primal (`ToadStool`)
    Compute,
    /// Security primal (crypto/PKI capability)
    Security,
    /// Storage primal (artifact/data capability)
    Storage,
    /// AI primal (intelligence capability)
    AI,
    /// Network primal (coordination capability)
    Network,
    /// OS primal (`BiomeOS`)
    OS,
    /// Custom primal type
    Custom(String),
}

impl PrimalType {
    /// Canonical lowercase name for routing and discovery.
    ///
    /// Use this instead of `format!("{:?}", …)` to avoid coupling to Debug output.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Compute => "compute",
            Self::Security => "security",
            Self::Storage => "storage",
            Self::AI => "ai",
            Self::Network => "network",
            Self::OS => "os",
            Self::Custom(name) => name.as_str(),
        }
    }

    /// Parse from a string (case-insensitive).
    #[must_use]
    pub fn from_str_lossy(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "compute" => Self::Compute,
            "security" => Self::Security,
            "storage" => Self::Storage,
            "ai" => Self::AI,
            "network" => Self::Network,
            "os" => Self::OS,
            other => Self::Custom(other.to_string()),
        }
    }
}

/// Primal capabilities
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrimalCapability {
    // Compute capabilities
    /// Container runtime support
    ContainerRuntime {
        /// Supported orchestrators (e.g. docker, k8s).
        orchestrators: Vec<String>,
    },
    /// Serverless execution
    ServerlessExecution {
        /// Supported languages.
        languages: Vec<String>,
    },
    /// GPU acceleration
    GpuAcceleration {
        /// CUDA support available.
        cuda_support: bool,
    },
    /// Load balancing
    LoadBalancing {
        /// Supported algorithms.
        algorithms: Vec<String>,
    },
    /// Auto-scaling
    AutoScaling {
        /// Metrics used for scaling.
        metrics: Vec<String>,
    },
    /// Native execution
    NativeExecution {
        /// Supported CPU architectures.
        architectures: Vec<String>,
    },
    /// WASM execution
    WasmExecution {
        /// WASI support available.
        wasi_support: bool,
    },

    // Security capabilities
    /// Authentication
    Authentication {
        /// Supported auth methods.
        methods: Vec<String>,
    },
    /// Encryption
    Encryption {
        /// Supported algorithms.
        algorithms: Vec<String>,
    },
    /// Key management
    KeyManagement {
        /// HSM support available.
        hsm_support: bool,
    },

    // Storage capabilities
    /// File system support
    FileSystem {
        /// ZFS support available.
        supports_zfs: bool,
    },
    /// Object storage
    ObjectStorage {
        /// Supported backends (e.g. s3, gcs).
        backends: Vec<String>,
    },
    /// Data replication
    DataReplication {
        /// Consistency model.
        consistency: String,
    },

    // AI capabilities
    /// Model inference
    ModelInference {
        /// Supported model types.
        models: Vec<String>,
    },
    /// Agent framework
    AgentFramework {
        /// MCP support available.
        mcp_support: bool,
    },
    /// Machine learning
    MachineLearning {
        /// Training support available.
        training_support: bool,
    },

    // Network capabilities
    /// Service discovery
    ServiceDiscovery {
        /// Supported protocols (e.g. mdns, dns-sd).
        protocols: Vec<String>,
    },
    /// Network routing
    NetworkRouting {
        /// Supported routing protocols.
        protocols: Vec<String>,
    },
    /// Proxy services
    ProxyServices {
        /// Proxy types (e.g. http, tcp).
        types: Vec<String>,
    },

    // OS capabilities
    /// Process management
    ProcessManagement {
        /// Container support available.
        container_support: bool,
    },
    /// Resource management
    ResourceManagement {
        /// Quota enforcement support.
        quota_support: bool,
    },
    /// Team isolation
    TeamIsolation {
        /// Multi-tenant support.
        multi_tenant: bool,
    },

    /// Custom capability with arbitrary name and attributes
    Custom {
        /// Capability name.
        name: String,
        /// Arbitrary attributes.
        attributes: HashMap<String, String>,
    },
}

/// Primal health status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrimalHealth {
    /// Healthy and operational
    Healthy,
    /// Degraded but operational
    Degraded {
        /// List of issues.
        issues: Vec<String>,
    },
    /// Unhealthy and not operational
    Unhealthy {
        /// Failure reason.
        reason: String,
    },
}

// Note: PrimalEndpoints, PrimalRequest, PrimalResponse, ResponseStatus
// are now in requests.rs to avoid duplication

#[cfg(test)]
#[path = "types_tests.rs"]
mod tests;
