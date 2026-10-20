// SPDX-License-Identifier: AGPL-3.0-or-later
//! Service discovery implementation

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use std::sync::RwLock;
use tracing::{debug, info, warn};

use crate::constants::PRIMAL_NAME;
use crate::discovery_defaults::{DiscoveryConfig, LocalhostFallbacks};
use crate::primal_identity::{
    Capability, ComputeCapability, CoordinationCapability, PrimalIdentity, ServiceEndpoint,
    StorageCapability,
};

use super::discovery_config::discover_from_config;
#[cfg(feature = "mdns")]
use super::discovery_mdns::discover_via_mdns;
use super::discovery_registry::discover_from_registry;
use super::fallback::services_from_eco_primals_runtime_sockets;
use super::types::{
    DiscoveredService, DiscoveryError, DiscoveryMethod, DiscoveryResult, ServiceDiscoveryTrait,
};

/// Main service discovery implementation
pub struct ServiceDiscovery {
    pub(crate) config: DiscoveryConfig,
    pub(crate) method: DiscoveryMethod,
    cache: Arc<RwLock<HashMap<String, DiscoveredService>>>,
    /// Tracks when the cache was last fully refreshed so that
    /// `find_services_by_capability` avoids redundant mDNS rounds
    /// when a recent refresh already returned zero results.
    last_refreshed: Arc<RwLock<Option<Instant>>>,
    fallbacks: LocalhostFallbacks,
}

impl ServiceDiscovery {
    /// Create discovery with specified method
    ///
    /// # Errors
    ///
    /// Never fails; initial refresh failures are logged but not propagated.
    pub async fn new(method: DiscoveryMethod) -> DiscoveryResult<Self> {
        let config = DiscoveryConfig::default();
        let fallbacks = LocalhostFallbacks::default();
        let discovery = Self {
            config,
            method,
            cache: Arc::new(RwLock::new(HashMap::new())),
            last_refreshed: Arc::new(RwLock::new(None)),
            fallbacks,
        };
        if let Err(e) = discovery.refresh().await {
            warn!("Initial discovery failed: {}", e);
        }
        Ok(discovery)
    }

    /// Create discovery with config
    ///
    /// # Errors
    ///
    /// Returns error if initial discovery refresh fails.
    pub async fn with_config(
        method: DiscoveryMethod,
        config: DiscoveryConfig,
    ) -> DiscoveryResult<Self> {
        let fallbacks = LocalhostFallbacks::default();
        let discovery = Self {
            config,
            method,
            cache: Arc::new(RwLock::new(HashMap::new())),
            last_refreshed: Arc::new(RwLock::new(None)),
            fallbacks,
        };
        discovery.refresh().await?;
        Ok(discovery)
    }

    /// Create discovery without initial refresh (for tests with mock servers).
    #[cfg(test)]
    pub(crate) fn new_no_refresh(method: DiscoveryMethod) -> Self {
        let config = DiscoveryConfig::default();
        let fallbacks = LocalhostFallbacks::default();
        Self {
            config,
            method,
            cache: Arc::new(RwLock::new(HashMap::new())),
            last_refreshed: Arc::new(RwLock::new(None)),
            fallbacks,
        }
    }

    /// Find a single service by capability
    ///
    /// # Errors
    ///
    /// Returns error if no service with the capability is found.
    pub async fn find_service_by_capability(
        &self,
        capability: Capability,
    ) -> DiscoveryResult<DiscoveredService> {
        let services = self.find_services_by_capability(&capability).await?;
        if let Some(service) = services.iter().find(|s| s.healthy).cloned() {
            return Ok(service);
        }
        services
            .into_iter()
            .next()
            .ok_or(DiscoveryError::NoServiceFound { capability })
    }

    async fn discover_via_method(&self) -> DiscoveryResult<Vec<DiscoveredService>> {
        let services = match &self.method {
            DiscoveryMethod::Auto => self.discover_auto().await,
            DiscoveryMethod::Environment => self.discover_from_env(),
            #[cfg(feature = "mdns")]
            DiscoveryMethod::Mdns => discover_via_mdns().await,
            #[cfg(not(feature = "mdns"))]
            DiscoveryMethod::Mdns => Ok(Vec::new()),
            DiscoveryMethod::ConfigFile { path } => discover_from_config(path),
            DiscoveryMethod::Registry { endpoint } => discover_from_registry(endpoint).await,
            DiscoveryMethod::Multi(methods) => self.discover_multi(methods).await,
        }?;
        Ok(services)
    }

    async fn discover_auto(&self) -> DiscoveryResult<Vec<DiscoveredService>> {
        let methods = vec![DiscoveryMethod::Environment, DiscoveryMethod::Mdns];
        self.discover_multi(&methods).await
    }

    async fn discover_multi(
        &self,
        methods: &[DiscoveryMethod],
    ) -> DiscoveryResult<Vec<DiscoveredService>> {
        let mut all_services = Vec::new();
        let mut successful = false;
        for method in methods {
            match self.discover_specific_method(method).await {
                Ok(services) => {
                    debug!("Discovered {} services via {:?}", services.len(), method);
                    all_services.extend(services);
                    successful = true;
                }
                Err(e) => debug!("Discovery via {:?} failed: {}", method, e),
            }
        }
        if !successful && all_services.is_empty() && self.fallbacks.should_use_fallback() {
            info!(
                "Discovery produced no results; trying fallbacks (ecoPrimals runtime sockets, then TCP)"
            );
            return self.discover_from_fallbacks();
        }
        Ok(all_services)
    }

    async fn discover_specific_method(
        &self,
        method: &DiscoveryMethod,
    ) -> DiscoveryResult<Vec<DiscoveredService>> {
        match method {
            DiscoveryMethod::Environment => self.discover_from_env(),
            #[cfg(feature = "mdns")]
            DiscoveryMethod::Mdns => discover_via_mdns().await,
            #[cfg(not(feature = "mdns"))]
            DiscoveryMethod::Mdns => Ok(Vec::new()),
            DiscoveryMethod::ConfigFile { path } => discover_from_config(path),
            DiscoveryMethod::Registry { endpoint } => discover_from_registry(endpoint).await,
            _ => Ok(Vec::new()),
        }
    }

    /// Discover from environment variables (pub for tests)
    ///
    /// # Errors
    ///
    /// Returns [`DiscoveryError`] if a service URL cannot be parsed.
    pub fn discover_from_env(&self) -> DiscoveryResult<Vec<DiscoveredService>> {
        let mut services = Vec::new();
        for (key, value) in std::env::vars() {
            if key.starts_with("TOADSTOOL_SERVICE_") && key.ends_with("_URL") {
                let service_name = key
                    .strip_prefix("TOADSTOOL_SERVICE_")
                    .and_then(|s| s.strip_suffix("_URL"))
                    .unwrap_or("unknown");
                let cap_key = format!("TOADSTOOL_SERVICE_{service_name}_CAPABILITIES");
                let capabilities_str = std::env::var(&cap_key).unwrap_or_default();
                let service = DiscoveredService::discovered_now(
                    format!("env-{}", service_name.to_lowercase()),
                    service_name.to_lowercase(),
                    "unknown",
                    Self::parse_capabilities(&capabilities_str),
                    vec![ServiceEndpoint::from_url_string(&value)?],
                );
                debug!("Discovered service from environment: {}", service.name);
                services.push(service);
            }
        }
        Ok(services)
    }

    fn discover_from_fallbacks(&self) -> DiscoveryResult<Vec<DiscoveredService>> {
        if !self.fallbacks.should_use_fallback() {
            return Ok(Vec::new());
        }

        let socket_services = services_from_eco_primals_runtime_sockets();
        if !socket_services.is_empty() {
            info!(
                count = socket_services.len(),
                "Discovered service(s) via ecoPrimals runtime sockets ($XDG_RUNTIME_DIR/ecoPrimals/{{capability}}.sock)"
            );
            return Ok(socket_services);
        }

        if let Some(url) = self.fallbacks.get_fallback_url(PRIMAL_NAME) {
            warn!(
                "TCP URL fallback for {} is deprecated for inter-primal discovery; prefer Unix sockets at $XDG_RUNTIME_DIR/ecoPrimals/{{capability}}.sock (wateringHole), or TOADSTOOL_SERVICE_*_URL endpoints",
                PRIMAL_NAME
            );
            let svc = DiscoveredService::discovered_now(
                format!("fallback-{PRIMAL_NAME}"),
                PRIMAL_NAME,
                "dev",
                vec![Capability::Compute(
                    crate::primal_identity::ComputeCapability::NativeExecution,
                )],
                vec![ServiceEndpoint::from_url_string(&url)?],
            )
            .with_metadata("source", "fallback-tcp")
            .with_metadata("deprecation", "tcp_url_fallback");
            return Ok(vec![svc]);
        }
        Ok(Vec::new())
    }

    pub(crate) fn parse_capabilities(capabilities_str: &str) -> Vec<Capability> {
        capabilities_str
            .split(',')
            .filter_map(|s| {
                let s = s.trim();
                match s {
                    "coordination" => Some(Capability::Coordination(
                        CoordinationCapability::ServiceDiscovery,
                    )),
                    "storage" => Some(Capability::Storage(StorageCapability::ObjectStorage)),
                    "compute" => Some(Capability::Compute(ComputeCapability::NativeExecution)),
                    _ => None,
                }
            })
            .collect()
    }
}

impl ServiceDiscoveryTrait for ServiceDiscovery {
    async fn find_services_by_capability(
        &self,
        capability: &Capability,
    ) -> DiscoveryResult<Vec<DiscoveredService>> {
        let cached: Vec<DiscoveredService> = {
            let cache = self
                .cache
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            cache
                .values()
                .filter(|s| s.has_capability(capability))
                .filter(|s| s.is_fresh(std::time::Duration::from_secs(self.config.cache_ttl_secs)))
                .cloned()
                .collect()
        };
        if !cached.is_empty() {
            debug!(
                "Found {} services in cache for capability: {:?}",
                cached.len(),
                capability
            );
            return Ok(cached);
        }

        // Avoid redundant mDNS/network scans: if a full discovery pass ran
        // within the cache TTL and found nothing, return empty rather than
        // re-scanning for every capability lookup.
        {
            let lr = self
                .last_refreshed
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(t) = *lr
                && t.elapsed() < std::time::Duration::from_secs(self.config.cache_ttl_secs)
            {
                debug!(
                    "Cache recently refreshed ({}ms ago); skipping re-discovery for {:?}",
                    t.elapsed().as_millis(),
                    capability
                );
                return Ok(vec![]);
            }
        }

        let services = self.discover_via_method().await?;
        {
            let mut cache = self
                .cache
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            for service in &services {
                cache
                    .entry(service.id.clone())
                    .or_insert_with(|| service.clone());
            }
        }
        *self
            .last_refreshed
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(Instant::now());
        Ok(services
            .into_iter()
            .filter(|s| s.has_capability(capability))
            .collect())
    }

    async fn discover_all(&self) -> DiscoveryResult<Vec<DiscoveredService>> {
        self.discover_via_method().await
    }

    async fn announce_self(&self, identity: &dyn PrimalIdentity) -> DiscoveryResult<()> {
        debug!(
            "announce_self: {} ({} capabilities) — use MdnsAdapter::announce() \
             from primal_discovery_mdns for full mDNS registration",
            identity.primal_name(),
            identity.capabilities().len()
        );
        Ok(())
    }

    async fn refresh(&self) -> DiscoveryResult<()> {
        let services = self.discover_via_method().await?;
        let count = {
            let mut cache = self
                .cache
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            cache.clear();
            for service in services {
                cache.insert(service.id.clone(), service);
            }
            cache.len()
        };
        *self
            .last_refreshed
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(Instant::now());
        info!("Discovery cache refreshed: {count} services");
        Ok(())
    }
}

#[cfg(test)]
#[path = "service_tests.rs"]
mod service_tests;
