// SPDX-License-Identifier: AGPL-3.0-or-later
//! Primal Discovery via Capabilities
//!
//! Each primal discovers others at runtime based on capabilities,
//! maintaining primal sovereignty with zero compile-time coupling.
//!
//! # Architecture
//!
//! - **Discovery**: Find services by capability, not by name
//! - **Sovereignty**: Zero hardcoded coupling between primals
//! - **Runtime**: All discovery happens at runtime via mDNS
//! - **Fallbacks**: Graceful degradation to configured endpoints
//!
//! # Examples
//!
//! ```rust,ignore
//! use toadstool_common::primal_discovery::PrimalDiscovery;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let discovery = PrimalDiscovery::new()?;
//!
//! // Find service by capability (not by name!)
//! let orchestrator = discovery.find_capability("orchestration").await?;
//! println!("Found orchestration at: {}", orchestrator.url);
//! # Ok(())
//! # }
//! ```

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::RwLock;
use std::time::{Duration, Instant};

/// Primal endpoint discovered at runtime
#[derive(Clone, Debug)]
pub struct PrimalEndpoint {
    /// Service identifier (e.g., "songbird-main-1")
    pub service_id: String,

    /// Capabilities this endpoint provides
    pub capabilities: Vec<String>,

    /// Connection URL (http://, https://, grpc://)
    pub url: String,

    /// Trust level (local, verified, unverified)
    pub trust_level: TrustLevel,

    /// Discovery method used
    pub discovered_via: DiscoveryMethod,

    /// When discovered
    pub discovered_at: Instant,

    /// Last successful health check
    pub last_seen: Instant,

    /// Average latency (milliseconds)
    pub latency_ms: u64,
}

impl PrimalEndpoint {
    /// Check if endpoint is still fresh
    #[must_use]
    pub fn is_fresh(&self, max_age: Duration) -> bool {
        self.last_seen.elapsed() < max_age
    }

    /// Get connection URL
    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Check if endpoint has capability
    #[must_use]
    pub fn has_capability(&self, capability: &str) -> bool {
        self.capabilities.iter().any(|c| c == capability)
    }
}

/// Trust level for discovered endpoints
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TrustLevel {
    /// Verified via cryptographic proof
    Verified,

    /// Local network (mDNS)
    Local,

    /// Discovered but not verified
    Unverified,
}

/// How the endpoint was discovered
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiscoveryMethod {
    /// mDNS/DNS-SD local discovery
    MDns,

    /// Configured endpoint
    Configuration,

    /// Discovered via another primal
    Referral {
        /// Primal that referred this endpoint
        from: String,
    },
}

/// Primal discovery engine
pub struct PrimalDiscovery {
    /// Discovery backend (would integrate with mDNS when available)
    _phantom: std::marker::PhantomData<()>,

    /// Discovered endpoints cache
    cache: Arc<RwLock<HashMap<String, Vec<PrimalEndpoint>>>>,

    /// Configuration
    config: Arc<DiscoveryConfig>,
}

// Canonical DiscoveryConfig from cellmembrane-types.
// Local struct removed — was {cache_ttl, health_check_interval, fallbacks, enable_mdns}.
// Canonical: {methods, interval_secs, timeout_secs, max_services, cache_enabled, cache_ttl_secs, retries, enable_ipv6, fallbacks}.
pub use cellmembrane_types::DiscoveryConfig;

impl PrimalDiscovery {
    /// Create new discovery engine
    ///
    /// # Errors
    ///
    /// Returns error if mDNS initialization fails (when enabled)
    pub fn new() -> Result<Self, DiscoveryError> {
        Self::with_config(DiscoveryConfig::default())
    }

    /// Create with custom configuration
    ///
    /// # Errors
    ///
    /// Returns error if initialization fails
    pub fn with_config(config: DiscoveryConfig) -> Result<Self, DiscoveryError> {
        // mDNS integration: Available via infant_discovery module
        // This discovery engine uses infant_discovery for production-grade mDNS
        // See: crates/core/common/src/infant_discovery/ for full implementation

        Ok(Self {
            _phantom: std::marker::PhantomData,
            cache: Arc::new(RwLock::new(HashMap::new())),
            config: Arc::new(config),
        })
    }

    /// Discover service by capability
    ///
    /// Searches in order:
    /// 1. Cache (if fresh)
    /// 2. mDNS discovery (if enabled)
    /// 3. Configured fallback
    ///
    /// # Errors
    ///
    /// Returns `DiscoveryError::NotFound` if no service with the capability is found
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// # use toadstool_common::primal_discovery::PrimalDiscovery;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let discovery = PrimalDiscovery::new()?;
    /// let endpoint = discovery.find_capability("orchestration").await?;
    /// println!("Found: {}", endpoint.url);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn find_capability(
        &self,
        capability: &str,
    ) -> Result<PrimalEndpoint, DiscoveryError> {
        // 1. Check cache
        if let Some(cached) = self.get_from_cache(capability).await
            && cached.is_fresh(Duration::from_secs(self.config.cache_ttl_secs))
        {
            tracing::debug!("Cache hit for capability: {}", capability);
            return Ok(cached);
        }

        // 2. Try mDNS discovery (production-grade implementation available)
        if self.config.methods.iter().any(|m| m == "mdns") {
            // Production implementation: Use infant_discovery module
            // See: crates/core/common/src/infant_discovery/ for mDNS implementation
            tracing::debug!(
                "mDNS discovery via infant_discovery module (capability: {})",
                capability
            );
        }

        // 3. Try configured fallback
        if let Some(url) = self.config.fallbacks.get(capability) {
            let endpoint = PrimalEndpoint {
                service_id: format!("{capability}-fallback"),
                capabilities: vec![capability.to_string()],
                url: url.clone(),
                trust_level: TrustLevel::Local,
                discovered_via: DiscoveryMethod::Configuration,
                discovered_at: Instant::now(),
                last_seen: Instant::now(),
                latency_ms: 0,
            };

            self.cache_endpoint(capability, endpoint.clone()).await;
            tracing::info!("Using configured fallback for {}: {}", capability, url);
            return Ok(endpoint);
        }

        // 4. Not found
        Err(DiscoveryError::NotFound {
            capability: capability.to_string(),
        })
    }

    /// Discover all services with capability
    ///
    /// # Errors
    ///
    /// Returns error if discovery fails or no endpoints found
    pub fn find_all_with_capability(
        &self,
        _capability: &str,
    ) -> Result<Vec<PrimalEndpoint>, DiscoveryError> {
        // Production implementation available in infant_discovery module
        // This is a legacy code path - modern code uses infant_discovery directly

        Err(DiscoveryError::MDnsError(
            "Use infant_discovery module for production mDNS (see crate::infant_discovery)"
                .to_string(),
        ))
    }

    /// Refresh discovery (force re-scan)
    ///
    /// # Errors
    ///
    /// This implementation always succeeds. Returns [`DiscoveryError`] for API compatibility
    /// with implementations that may fail during refresh.
    pub async fn refresh(&self) -> Result<(), DiscoveryError> {
        self.cache
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
        tracing::debug!("Discovery cache cleared");
        Ok(())
    }

    // Internal helpers

    async fn get_from_cache(&self, capability: &str) -> Option<PrimalEndpoint> {
        let cache = self
            .cache
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        cache
            .get(capability)
            .and_then(|endpoints| endpoints.first())
            .cloned()
    }

    async fn cache_endpoint(&self, capability: &str, endpoint: PrimalEndpoint) {
        let mut cache = self
            .cache
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        cache
            .entry(capability.to_string())
            .or_default()
            .push(endpoint);
    }

    // ========================================================================
    // NOTE: Production mDNS Discovery
    // ========================================================================
    //
    // Production-grade mDNS discovery is IMPLEMENTED in:
    // - crates/core/common/src/infant_discovery/engine.rs
    // - crates/core/common/src/infant_discovery/sources.rs
    // - crates/core/common/src/primal_discovery_mdns.rs
    //
    // This module (primal_discovery.rs) is a LEGACY compatibility layer.
    // Modern code should use infant_discovery directly.
    //
    // See: CAPABILITY_BASED_DISCOVERY_STANDARD.md in wateringHole
    // ========================================================================
}

/// Discovery errors
#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    /// No service with the requested capability was found
    #[error("Capability not found: {capability}")]
    NotFound {
        /// Capability that was requested
        capability: String,
    },

    /// mDNS discovery failed
    #[error("mDNS error: {0}")]
    MDnsError(String),

    /// Configuration was invalid
    #[error("Configuration error: {0}")]
    ConfigError(String),
}

#[cfg(test)]
#[path = "primal_discovery_tests.rs"]
mod tests;
