// SPDX-License-Identifier: AGPL-3.0-or-later
//! Complete Primal Discovery with mDNS Integration
//!
//! This module completes the capability-based discovery system by wiring up
//! mDNS service discovery with the `PrimalDiscovery` interface.
//!
//! # Philosophy
//!
//! - **Each primal knows only itself** - No hardcoded knowledge of other services
//! - **Discover by capability** - Find services by WHAT they do, not WHO they are
//! - **Runtime resolution** - All dependencies discovered at runtime
//! - **Graceful degradation** - Falls back to configuration when mDNS unavailable
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────┐
//! │      Application Layer              │
//! │  (Needs "orchestration" capability) │
//! └──────────────┬──────────────────────┘
//!                │
//!                ▼
//! ┌─────────────────────────────────────┐
//! │    PrimalDiscoveryEngine            │
//! │  - Capability-based interface       │
//! │  - Caching & health checking        │
//! │  - Fallback handling                │
//! └──────────────┬──────────────────────┘
//!                │
//!       ┌────────┴────────┐
//!       ▼                 ▼
//! ┌──────────┐      ┌──────────────┐
//! │   mDNS   │      │ Config       │
//! │ Discovery│      │ Fallbacks    │
//! └──────────┘      └──────────────┘
//! ```

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::RwLock;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

use crate::constants::discovery_ports::{
    DEFAULT_COORDINATION_PORT, DEFAULT_SECURITY_PORT, DEFAULT_STORAGE_PORT, DISCOVERY_HTTP_FALLBACK,
};
use crate::constants::network::HTTP_PROTOCOL;
use crate::interned_strings::socket_env;
use crate::primal_identity::{Capability, DiscoveredService};
use crate::runtime_discovery::DiscoveryClient;
use crate::{ToadStoolError, ToadStoolResult};

/// Resolve a port from an environment variable, falling back to `default`.
fn resolve_env_port(env_var: &str, default: u16) -> u16 {
    std::env::var(env_var)
        .ok()
        .and_then(|v| v.parse::<u16>().ok())
        .unwrap_or(default)
}

#[cfg(test)]
mod tests;

/// Complete primal discovery engine with mDNS integration
pub struct PrimalDiscoveryEngine<C: DiscoveryClient> {
    /// Primary discovery backend (mDNS)
    mdns_client: Option<Arc<C>>,

    /// Discovered endpoints cache
    cache: Arc<RwLock<HashMap<String, CachedEndpoint>>>,

    /// Configuration
    config: Arc<DiscoveryConfig>,
}

/// Cached endpoint with freshness tracking
#[derive(Clone, Debug)]
struct CachedEndpoint {
    service: DiscoveredService,
    #[expect(dead_code, reason = "timestamped for future cache expiration logic")]
    discovered_at: Instant,
    last_checked: Instant,
}

impl CachedEndpoint {
    fn is_fresh(&self, ttl: Duration) -> bool {
        self.last_checked.elapsed() < ttl
    }
}

// Canonical DiscoveryConfig from cellmembrane-types.
// Local struct removed — was {cache_ttl, health_check_interval, fallbacks, enable_mdns, require_mdns}.
// Canonical: {methods, interval_secs, timeout_secs, max_services, cache_enabled, cache_ttl_secs, retries, enable_ipv6, fallbacks}.
// Environment-aware defaults and fallback logic moved to PrimalDiscoveryEngine constructors.
pub use cellmembrane_types::DiscoveryConfig;

/// Build environment-aware fallbacks for discovery.
///
/// Populated when `TOADSTOOL_DISCOVERY_FALLBACKS` is set or `TOADSTOOL_ENV=development`.
#[expect(
    deprecated,
    reason = "legacy env var names used as backward-compat fallbacks"
)]
pub(crate) fn build_discovery_fallbacks() -> HashMap<String, String> {
    let coordination_port = resolve_env_port(
        socket_env::TOADSTOOL_COORDINATION_PORT,
        DEFAULT_COORDINATION_PORT,
    );
    let security_port =
        resolve_env_port(socket_env::TOADSTOOL_SECURITY_PORT, DEFAULT_SECURITY_PORT);
    let storage_port =
        resolve_env_port(socket_env::TOADSTOOL_STORAGE_PORT, DEFAULT_STORAGE_PORT);

    let dev_mode = std::env::var(socket_env::TOADSTOOL_DISCOVERY_FALLBACKS).is_ok()
        || std::env::var(socket_env::TOADSTOOL_ENV)
            .is_ok_and(|e| e == crate::interned_strings::env::DEVELOPMENT);

    if !dev_mode {
        return HashMap::new();
    }

    let bind_host = std::env::var(socket_env::TOADSTOOL_BIND_HOST)
        .or_else(|_| std::env::var(socket_env::BIND_HOST))
        .unwrap_or_else(|_| crate::constants::network::DEFAULT_HOSTNAME.to_string());

    let specs: &[(&str, &str, u16, &[&str])] = &[
        (
            socket_env::TOADSTOOL_COORDINATION_URL,
            socket_env::LEGACY_SONGBIRD_URL,
            coordination_port,
            &["orchestration", "coordination"],
        ),
        (
            socket_env::TOADSTOOL_SECURITY_URL,
            socket_env::LEGACY_BEARDOG_URL,
            security_port,
            &["security", "authentication"],
        ),
        (
            socket_env::TOADSTOOL_STORAGE_URL,
            socket_env::LEGACY_NESTGATE_URL,
            storage_port,
            &["storage"],
        ),
    ];

    let mut fallbacks = HashMap::new();
    for (cap_env, legacy_env, port, keys) in specs {
        let url = std::env::var(cap_env)
            .or_else(|_| std::env::var(legacy_env))
            .unwrap_or_else(|_| format!("{HTTP_PROTOCOL}{bind_host}:{port}"));
        for key in *keys {
            fallbacks.insert((*key).to_string(), url.clone());
        }
    }
    fallbacks
}

impl<C: DiscoveryClient> PrimalDiscoveryEngine<C> {
    /// Create with custom configuration
    ///
    /// # Errors
    ///
    /// Returns error if configuration is invalid or mDNS client is required but missing
    pub fn with_config(
        mdns_client: Option<Arc<C>>,
        config: DiscoveryConfig,
    ) -> ToadStoolResult<Self> {
        let has_mdns = config.methods.iter().any(|m| m == "mdns");

        if has_mdns && mdns_client.is_some() {
            info!("✅ mDNS discovery enabled");
        } else if !config.fallbacks.is_empty() {
            info!(
                "⚠️  mDNS unavailable, using {} configured fallbacks",
                config.fallbacks.len()
            );
        } else {
            warn!("⚠️  No mDNS and no fallbacks configured - discovery may fail");
        }

        Ok(Self {
            mdns_client,
            cache: Arc::new(RwLock::new(HashMap::new())),
            config: Arc::new(config),
        })
    }
}

impl PrimalDiscoveryEngine<crate::runtime_discovery::LocalhostDiscoveryClient> {
    /// Create new discovery engine without an external discovery client (cache + fallbacks only).
    ///
    /// # Errors
    ///
    /// Returns error if configuration is invalid
    pub fn new_without_client() -> ToadStoolResult<Self> {
        Self::with_config(None, DiscoveryConfig::default())
    }

    /// Same as [`Self::with_config`] with no external client.
    pub fn without_client(config: DiscoveryConfig) -> ToadStoolResult<Self> {
        Self::with_config(None, config)
    }
}

impl<C: DiscoveryClient> PrimalDiscoveryEngine<C> {
    /// Create new discovery engine with optional discovery client and default configuration.
    ///
    /// # Errors
    ///
    /// Returns error if mDNS client is required but unavailable
    pub fn with_client_and_defaults(mdns_client: Option<Arc<C>>) -> ToadStoolResult<Self> {
        Self::with_config(mdns_client, DiscoveryConfig::default())
    }

    /// Discover service by capability (main interface)
    ///
    /// Searches in order:
    /// 1. Cache (if fresh)
    /// 2. mDNS discovery (if enabled)
    /// 3. Configured fallback
    ///
    /// # Errors
    ///
    /// Returns error if no service with the capability is found
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// # use toadstool_common::primal_discovery_complete::PrimalDiscoveryEngine;
    /// # use toadstool_common::primal_identity::Capability;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let engine = PrimalDiscoveryEngine::new_without_client()?;
    ///
    /// // Find orchestration service (discovers Songbird if available)
    /// let services = engine.discover_by_capability(&Capability::Coordination(
    ///     toadstool_common::primal_identity::CoordinationCapability::ServiceDiscovery
    /// )).await?;
    ///
    /// for service in services {
    ///     println!("Found: {:?}", service.endpoints);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn discover_by_capability(
        &self,
        capability: &Capability,
    ) -> ToadStoolResult<Vec<DiscoveredService>> {
        let capability_str = Self::capability_to_string(capability);

        // 1. Check cache
        if let Some(cached) = self.get_from_cache(&capability_str).await
            && cached.is_fresh(Duration::from_secs(self.config.cache_ttl_secs))
        {
            debug!("✅ Cache hit for capability: {}", capability_str);
            return Ok(vec![cached.service]);
        }

        // 2. Try mDNS discovery
        let has_mdns = self.config.methods.iter().any(|m| m == "mdns");
        if has_mdns
            && let Some(ref mdns) = self.mdns_client
        {
            debug!("🔍 Querying mDNS for capability: {}", capability_str);

            match mdns.discover_by_capability(capability).await {
                Ok(services) if !services.is_empty() => {
                    info!(
                        "✅ mDNS discovered {} services for {}",
                        services.len(),
                        capability_str
                    );

                    // Cache all discovered services
                    for service in &services {
                        self.cache_service(&capability_str, service.clone()).await;
                    }

                    return Ok(services);
                }
                Ok(_) => {
                    debug!("mDNS query returned no results for {}", capability_str);
                }
                Err(e) => {
                    warn!("mDNS query failed for {}: {}", capability_str, e);
                }
            }
        }

        // 3. Try configured fallback
        if let Some(url) = self.config.fallbacks.get(&capability_str) {
            info!(
                "⚡ Using configured fallback for {}: {}",
                capability_str, url
            );

            let service = Self::create_fallback_service(&capability_str, url, capability);
            self.cache_service(&capability_str, service.clone()).await;

            return Ok(vec![service]);
        }

        // 4. Not found
        Err(ToadStoolError::not_found(format!(
            "No service found with capability: {capability_str}"
        )))
    }

    /// Convert capability to string key for caching/fallback lookup
    fn capability_to_string(capability: &Capability) -> String {
        match capability {
            Capability::Coordination(_) => "orchestration".to_string(),
            Capability::Compute(_) => "compute".to_string(),
            Capability::Storage(_) => "storage".to_string(),
            Capability::Crypto(_) => "crypto".to_string(),
            Capability::Authentication(_) => "authentication".to_string(),
            Capability::Discovery(_) => "discovery".to_string(),
            Capability::Custom { name, .. } => name.clone(),
        }
    }

    /// Create fallback service from configured URL
    fn create_fallback_service(
        capability_str: &str,
        url: &str,
        capability: &Capability,
    ) -> DiscoveredService {
        // Parse URL to extract host and port
        // Note: For simple parsing, we'll use string operations instead of url crate
        let endpoint = if url.starts_with("http://") || url.starts_with("https://") {
            // Simple URL parsing without external crate
            let parts: Vec<&str> = url.split("://").collect();
            if parts.len() == 2 {
                let protocol = parts[0];
                let remainder = parts[1];
                let (host_port, path) = remainder.find('/').map_or((remainder, "/"), |idx| {
                    (&remainder[..idx], &remainder[idx..])
                });

                let (host, port) = host_port.find(':').map_or_else(
                    || (host_port, if protocol == "https" { 443 } else { 80 }),
                    |idx| {
                        (
                            &host_port[..idx],
                            host_port[idx + 1..]
                                .parse::<u16>()
                                .unwrap_or(DISCOVERY_HTTP_FALLBACK),
                        )
                    },
                );

                crate::primal_identity::ServiceEndpoint {
                    protocol: protocol.to_string(),
                    address: host.to_string(),
                    port,
                    path: Some(path.to_string()),
                    metadata: HashMap::new(),
                }
            } else {
                // Fallback when URL parsing fails - use configurable localhost constant
                crate::primal_identity::ServiceEndpoint {
                    protocol: "http".to_string(),
                    address: crate::constants::network::LOCALHOST_IPV4.to_string(),
                    port: DISCOVERY_HTTP_FALLBACK,
                    path: Some("/".to_string()),
                    metadata: HashMap::new(),
                }
            }
        } else if let Ok(parsed_addr) = url.parse::<std::net::SocketAddr>() {
            // Handle "localhost:PORT" format
            crate::primal_identity::ServiceEndpoint {
                protocol: "http".to_string(),
                address: parsed_addr.ip().to_string(),
                port: parsed_addr.port(),
                path: Some("/".to_string()),
                metadata: HashMap::new(),
            }
        } else {
            // Fallback for any other format - use configurable localhost constant
            crate::primal_identity::ServiceEndpoint {
                protocol: "http".to_string(),
                address: crate::constants::network::LOCALHOST_IPV4.to_string(),
                port: DISCOVERY_HTTP_FALLBACK,
                path: Some("/".to_string()),
                metadata: HashMap::new(),
            }
        };

        DiscoveredService {
            id: Some(format!("{capability_str}-fallback")),
            capabilities: vec![capability.clone()],
            endpoints: vec![endpoint],
            healthy: true, // Assume healthy until health check proves otherwise
            metadata: {
                let mut map = HashMap::new();
                map.insert("source".to_string(), "configuration".to_string());
                map
            },
        }
    }

    /// Get service from cache
    async fn get_from_cache(&self, capability: &str) -> Option<CachedEndpoint> {
        self.cache
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(capability)
            .cloned()
    }

    /// Cache discovered service
    async fn cache_service(&self, capability: &str, service: DiscoveredService) {
        let now = Instant::now();
        let cached = CachedEndpoint {
            service,
            discovered_at: now,
            last_checked: now,
        };

        self.cache
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(capability.to_string(), cached);
    }

    /// Clear cache (useful for testing or forced refresh)
    pub async fn clear_cache(&self) {
        self.cache
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
        debug!("Discovery cache cleared");
    }

    /// Get cache statistics
    pub async fn cache_stats(&self) -> CacheStats {
        let (total, fresh) = {
            let cache = self
                .cache
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let total = cache.len();
            let fresh = cache
                .values()
                .filter(|e| e.is_fresh(Duration::from_secs(self.config.cache_ttl_secs)))
                .count();
            drop(cache);
            (total, fresh)
        };

        CacheStats {
            total_entries: total,
            fresh_entries: fresh,
            stale_entries: total - fresh,
        }
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    /// Total number of cached entries
    pub total_entries: usize,
    /// Entries still within TTL
    pub fresh_entries: usize,
    /// Entries past TTL
    pub stale_entries: usize,
}
