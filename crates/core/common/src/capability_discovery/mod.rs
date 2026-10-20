// SPDX-License-Identifier: AGPL-3.0-or-later
//! Capability-Based Service Discovery - Pure Infant Discovery Pattern
//!
//! This module implements the **pure infant discovery** pattern where ToadStool:
//! 1. Knows only itself (self-knowledge)
//! 2. Discovers other primals at runtime by capabilities
//! 3. Has zero hardcoded primal names or ports
//!
//! ## Philosophy
//!
//! > "Each primal is born knowing nothing except itself.
//! >  It discovers others by what they can do, not who they are."
//!
//! This enables:
//! - **Multi-vendor support**: Any service providing a capability works
//! - **Federation**: Multiple providers for same capability
//! - **Edge capability**: Local service discovery (mDNS via coordination service)
//!
//! ## Evolution (Feb 15, 2026)
//!
//! Service discovery is delegated to the coordination service (comms layer). ToadStool only
//! exposes mDNS capability requirements; that layer handles the actual discovery.
//! Vendor-specific discovery (K8s, Consul, cloud providers) removed.
//!
//! ## Usage Example
//!
//! ```rust,no_run
//! use toadstool_common::capability_discovery::CapabilityDiscovery;
//! use toadstool_common::primal_identity::{Capability, CryptoCapability};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create discovery engine
//! let discovery = CapabilityDiscovery::new_async().await?;
//!
//! // Find ANY service that can encrypt (agnostic of provider)
//! let crypto_services = discovery
//!     .find_by_capability(Capability::Crypto(CryptoCapability::Encryption))
//!     .await?;
//!
//! // Use the first available (or implement selection logic)
//! if let Some(service) = crypto_services.first() {
//!     println!("Found crypto service: {}", service.name);
//!     // Connect and use...
//! }
//! # Ok(())
//! # }
//! ```

mod types;

#[cfg(test)]
#[expect(
    deprecated,
    reason = "tests reference legacy BEARDOG_* env constants to clear them"
)]
mod tests;

pub use types::{DiscoveryConfig, DiscoveryError, DiscoveryMethod};

use crate::primal_identity::Capability;
use crate::service_discovery::{
    DiscoveredService, DiscoveryError as SvcDiscoveryError, ServiceDiscovery,
    ServiceDiscoveryTrait, localhost_capability_fallback,
};
use std::time::Duration;

/// PATH-based binary lookup (pure Rust, no external `which` crate).
#[cfg(target_os = "linux")]
fn which_in_path(binary: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|path_var| {
        std::env::split_paths(&path_var).any(|dir| dir.join(binary).is_file())
    })
}

/// Capability-based discovery client
///
/// This is the **primary interface** for discovering services by capability.
/// Service discovery is delegated to the coordination service (comms layer) via mDNS.
pub struct CapabilityDiscovery {
    /// Underlying discovery implementation
    discovery: ServiceDiscovery,

    /// Discovery timeout
    timeout: Duration,

    /// Enable fallback to localhost in development
    enable_localhost_fallback: bool,
}

impl CapabilityDiscovery {
    /// Create new capability discovery client.
    ///
    /// Automatically detects available discovery methods:
    /// - mDNS/DNS-SD via coordination service (if on local network)
    /// - Environment variables (always available)
    ///
    /// # Errors
    ///
    /// Returns error if discovery backend cannot be initialized.
    pub async fn new_async() -> Result<Self, DiscoveryError> {
        Self::with_config_async(&DiscoveryConfig::default()).await
    }

    /// Create with custom configuration.
    ///
    /// # Errors
    ///
    /// Returns `DiscoveryError` if the discovery backend cannot be initialized.
    pub async fn with_config_async(config: &DiscoveryConfig) -> Result<Self, DiscoveryError> {
        let discovery = Self::detect_discovery_backend_async().await?;

        Ok(Self {
            discovery,
            timeout: std::time::Duration::from_secs(config.timeout_secs),
            enable_localhost_fallback: config.fallbacks.contains_key("localhost"),
        })
    }

    /// Find services by capability
    ///
    /// Returns ALL services providing the requested capability.
    ///
    /// # Errors
    ///
    /// Returns error if timeout expires, discovery fails, or no services found (when localhost fallback disabled).
    /// Caller can choose based on additional criteria (latency, trust level, etc.)
    ///
    /// ## Multi-Provider Example
    ///
    /// ```rust,no_run
    /// # use toadstool_common::capability_discovery::CapabilityDiscovery;
    /// # use toadstool_common::primal_identity::{Capability, CryptoCapability};
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let discovery = CapabilityDiscovery::new_async().await?;
    ///
    /// // Find ALL crypto providers
    /// let providers = discovery
    ///     .find_by_capability(Capability::Crypto(CryptoCapability::Encryption))
    ///     .await?;
    ///
    /// // Select based on criteria (metadata can contain latency, trust info)
    /// let local_provider = providers.iter()
    ///     .filter(|s| s.healthy)
    ///     .next();
    ///
    /// let latest_version = providers.iter()
    ///     .max_by(|a, b| a.version.cmp(&b.version));
    /// # Ok(())
    /// # }
    /// ```
    pub async fn find_by_capability(
        &self,
        capability: Capability,
    ) -> Result<Vec<DiscoveredService>, DiscoveryError> {
        let result: Result<Vec<DiscoveredService>, SvcDiscoveryError> = tokio::time::timeout(
            self.timeout,
            self.discovery.find_services_by_capability(&capability),
        )
        .await
        .map_err(|_| DiscoveryError::Timeout)?;

        match result {
            Ok(services) if !services.is_empty() => Ok(services),
            Ok(_) if self.enable_localhost_fallback => {
                // Fallback for development
                Ok(Self::try_localhost_fallback(&capability))
            }
            Ok(_) => Err(DiscoveryError::NoServicesFound(format!("{capability:?}"))),
            Err(e) => Err(DiscoveryError::DiscoveryFailed(e.to_string())),
        }
    }

    /// Find the "best" service for a capability
    ///
    /// # Errors
    ///
    /// Returns error if [`find_by_capability`](Self::find_by_capability) fails or no services available.
    ///
    /// Selection criteria (in order):
    /// 1. Healthy services preferred
    /// 2. Most recently seen (fresher discovery)
    /// 3. Metadata-based trust level (if available)
    pub async fn find_best(
        &self,
        capability: Capability,
    ) -> Result<DiscoveredService, DiscoveryError> {
        let services = self.find_by_capability(capability).await?;

        services
            .into_iter()
            .max_by(|a, b| {
                // Healthy services first
                let health_cmp = a.healthy.cmp(&b.healthy);
                if health_cmp != std::cmp::Ordering::Equal {
                    return health_cmp;
                }

                // Then by freshness (more recent is better)
                a.last_seen.cmp(&b.last_seen)
            })
            .ok_or_else(|| DiscoveryError::NoServicesFound("No services available".to_string()))
    }

    /// Async detection of available discovery backend.
    ///
    /// **Deep Debt Compliance**: Runtime environment detection
    /// - Checks for mDNS capability (local network via coordination service)
    /// - Falls back to environment variables (self-knowledge)
    /// - Graceful degradation at every level
    ///
    /// ## Evolution (Mar 29, 2026)
    ///
    /// Removed nested `Runtime::new()` + `block_on` anti-pattern; initialization is async-only.
    async fn detect_discovery_backend_async() -> Result<ServiceDiscovery, DiscoveryError> {
        use crate::service_discovery::DiscoveryMethod;

        #[cfg(target_os = "linux")]
        {
            if which_in_path("avahi-browse") {
                tracing::info!(
                    "mDNS (Avahi) available - coordination service can use for local discovery"
                );
            }
        }

        #[cfg(target_os = "macos")]
        {
            tracing::info!(
                "mDNS (Bonjour) available on macOS - coordination service can use for discovery"
            );
        }

        tracing::info!("Using environment-based service discovery");

        ServiceDiscovery::new(DiscoveryMethod::Auto)
            .await
            .map_err(|e| DiscoveryError::DiscoveryFailed(e.to_string()))
    }

    /// Try localhost fallback for development (ecoPrimals / biomeOS sockets, `TOADSTOOL_LOCAL_PORT`, …).
    ///
    /// Delegates to [`crate::service_discovery::localhost_capability_fallback`] so behavior stays
    /// aligned with [`crate::service_discovery::ServiceDiscovery::discover_from_fallbacks`].
    fn try_localhost_fallback(capability: &Capability) -> Vec<DiscoveredService> {
        localhost_capability_fallback(capability)
    }
}
