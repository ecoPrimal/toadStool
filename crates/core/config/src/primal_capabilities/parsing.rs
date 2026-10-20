// SPDX-License-Identifier: AGPL-3.0-or-later
//! TOML loading and serde schema for `primal-capabilities.toml`.

#[cfg(feature = "runtime")]
use etcetera::BaseStrategy;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use toadstool_common::interned_strings::socket_env;

/// Primal capability registry error.
#[derive(Debug, thiserror::Error)]
pub enum CapabilityError {
    /// Failed to load capabilities file from disk.
    #[error("Failed to load capabilities file: {0}")]
    LoadFailed(String),

    /// Failed to parse capabilities TOML.
    #[error("Failed to parse capabilities: {0}")]
    ParseFailed(String),

    /// Requested primal not found in registry.
    #[error("Primal not found: {0}")]
    PrimalNotFound(String),

    /// Requested capability not found.
    #[error("Capability not found: {0}")]
    CapabilityNotFound(String),

    /// No endpoint configured for the primal.
    #[error("No endpoint configured for primal: {0}")]
    NoEndpoint(String),
}

/// Result type for capability registry operations.
pub type CapabilityResult<T> = Result<T, CapabilityError>;

/// Primal capabilities registry
///
/// Loads and manages primal capability information from primal-capabilities.toml
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrimalCapabilitiesRegistry {
    /// Registry metadata
    #[serde(default)]
    pub registry: RegistryMetadata,

    /// Primal definitions
    #[serde(default)]
    pub primals: HashMap<String, PrimalDefinition>,

    /// Discovery configuration
    #[serde(default)]
    pub discovery: DiscoveryConfig,

    /// Migration mappings (deprecated - for backward compatibility)
    #[serde(default)]
    pub migration: HashMap<String, MigrationMapping>,
}

/// Registry metadata from primal-capabilities.toml.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RegistryMetadata {
    /// Registry schema version.
    #[serde(default)]
    pub version: String,

    /// Discovery protocol (e.g. mdns, environment).
    #[serde(default)]
    pub discovery_protocol: String,

    /// Fallback strategy when discovery fails.
    #[serde(default)]
    pub fallback_strategy: String,
}

/// Primal definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrimalDefinition {
    /// Primal name
    pub name: String,

    /// Human-readable description
    #[serde(default)]
    pub description: String,

    /// Primary role (compute, security, storage, coordination, intelligence, orchestration)
    pub primary_role: String,

    /// Capabilities this primal provides
    #[serde(default)]
    pub capabilities: Vec<String>,

    /// Supported protocols
    #[serde(default)]
    pub protocols: Vec<String>,

    /// Default port (discovered at runtime, not hardcoded in code!)
    pub default_port: u16,

    /// Health check endpoint
    #[serde(default)]
    pub health_endpoint: String,

    /// Metrics endpoint (optional)
    #[serde(default)]
    pub metrics_endpoint: Option<String>,

    /// Discovery endpoint (for coordination services)
    #[serde(default)]
    pub discovery_endpoint: Option<String>,
}

// Canonical DiscoveryConfig from cellmembrane-types.
// Local struct removed — was {methods, cache_enabled, cache_ttl_seconds, retry_attempts,
// retry_delay_ms, timeout_seconds, health_check_enabled, health_check_interval_seconds,
// health_check_timeout_seconds, preferences}.
// Canonical: {methods, interval_secs, timeout_secs, max_services, cache_enabled,
// cache_ttl_secs, retries, enable_ipv6, fallbacks}.
pub use cellmembrane_types::DiscoveryConfig;

/// Discovery preferences for service selection.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DiscoveryPreferences {
    /// Prefer services on localhost.
    #[serde(default)]
    pub prefer_local: bool,

    /// Only use services that pass health check.
    #[serde(default)]
    pub require_healthy: bool,

    /// Load balancing strategy (round-robin, random, etc.).
    #[serde(default)]
    pub load_balance: String,

    /// Use any available service if preferred unavailable.
    #[serde(default)]
    pub fallback_to_any: bool,
}

/// Migration mapping (deprecated - for backward compatibility)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationMapping {
    /// Capability to discover
    pub capability: String,

    /// Fallback URL (last resort only)
    pub fallback_url: String,
}

impl PrimalCapabilitiesRegistry {
    /// Load registry from file
    ///
    /// # Errors
    /// Returns error if file cannot be read or parsed
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> CapabilityResult<Self> {
        let content = fs::read_to_string(path.as_ref())
            .map_err(|e| CapabilityError::LoadFailed(e.to_string()))?;

        let registry: Self =
            toml::from_str(&content).map_err(|e| CapabilityError::ParseFailed(e.to_string()))?;

        Ok(registry)
    }

    /// Load registry from default location
    ///
    /// Tries these paths in order:
    /// 1. `PRIMAL_CAPABILITIES_PATH` environment variable
    /// 2. `./primal-capabilities.toml`
    /// 3. `~/.config/toadstool/primal-capabilities.toml`
    ///
    /// # Errors
    /// Returns error if no file found or parse fails
    pub fn load_default() -> CapabilityResult<Self> {
        // Try environment variable
        if let Ok(path) = std::env::var(socket_env::PRIMAL_CAPABILITIES_PATH)
            && Path::new(&path).exists()
        {
            return Self::load_from_file(&path);
        }

        // Try current directory
        let local_path = Path::new("primal-capabilities.toml");
        if local_path.exists() {
            return Self::load_from_file(local_path);
        }

        // Try config directory (Pure Rust Evolution - Jan 17, 2026)
        #[cfg(feature = "runtime")]
        if let Ok(strategy) = etcetera::choose_base_strategy() {
            let primal_name = toadstool_common::constants::primal_identity::PRIMAL_NAME;
            let config_path = strategy
                .config_dir()
                .join(primal_name)
                .join("primal-capabilities.toml");
            if config_path.exists() {
                return Self::load_from_file(config_path);
            }
        } else {
            // Strategy selection failed, continue to error
        }

        Err(CapabilityError::LoadFailed(
            "No primal-capabilities.toml found in default locations".to_string(),
        ))
    }
}
