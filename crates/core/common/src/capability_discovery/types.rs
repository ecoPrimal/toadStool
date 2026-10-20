// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;

// Canonical DiscoveryConfig from cellmembrane-types.
// Local struct removed — was {timeout, enable_localhost_fallback, methods: Vec<DiscoveryMethod>}.
// Canonical: {methods: Vec<String>, interval_secs, timeout_secs, max_services, cache_enabled, cache_ttl_secs, retries, enable_ipv6, fallbacks}.
pub use cellmembrane_types::DiscoveryConfig;

/// Discovery methods
///
/// Service discovery is delegated to the coordination service (comms layer).
/// ToadStool only supports mDNS (via that layer) and environment variables.
#[derive(Debug, Clone, Copy)]
pub enum DiscoveryMethod {
    /// Automatically detect best method
    Auto,

    /// mDNS/DNS-SD (local network via coordination service)
    Mdns,

    /// Environment variables (self-knowledge)
    Environment,
}

/// Discovery errors
#[derive(Debug, Error)]
pub enum DiscoveryError {
    /// Discovery operation exceeded timeout
    #[error("Discovery timeout")]
    Timeout,

    /// No services advertising the capability were found
    #[error("No services found for capability: {0}")]
    NoServicesFound(String),

    /// Discovery backend failed
    #[error("Discovery failed: {0}")]
    DiscoveryFailed(String),

    /// Configuration was invalid
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
}
