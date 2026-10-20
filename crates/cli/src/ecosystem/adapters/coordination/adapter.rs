// SPDX-License-Identifier: AGPL-3.0-or-later
//! [`CoordinationAdapter`] implementation — capability-based coordination calls.

use crate::{CliContextExt, Result};
use serde_json::json;
use std::sync::Arc;

use super::super::universal::{Request, UniversalServiceAdapter};
use super::types::{LockHandle, PeerInfo, RegistrationToken, ServiceInfo};
use crate::ecosystem::capabilities::StandardCapability;

/// Coordination adapter - provides coordination operations via capability discovery
///
/// This adapter discovers and invokes coordination services without knowing their identity.
/// Backends include Consul, etcd, Kubernetes service discovery, or any custom coordination provider.
pub struct CoordinationAdapter {
    /// Universal service adapter for invoking capabilities
    universal: Arc<UniversalServiceAdapter>,
}

impl CoordinationAdapter {
    /// Create a new coordination adapter
    pub const fn new(universal: Arc<UniversalServiceAdapter>) -> Self {
        Self { universal }
    }

    /// Register this service with the coordination service
    ///
    /// Discovers a service registry and registers this service, making it
    /// discoverable by other services in the ecosystem.
    ///
    /// # Example
    /// ```ignore
    /// // Forward-looking example - API under development
    /// # use toadstool_cli::ecosystem::adapters::CoordinationAdapter;
    /// # async fn example(coordination: CoordinationAdapter) -> anyhow::Result<()> {
    /// let token = coordination.register_service(ServiceInfo {
    ///     name: "toadstool".to_string(),
    ///     capabilities: vec!["compute.wasm.component-model".to_string()],
    ///     endpoint: format!("http://{}:{}", toadstool_common::constants::DEFAULT_HOSTNAME, 8080),
    ///     metadata: Default::default(),
    /// }).await?;
    ///
    /// println!("Registered with token: {:?}", token);
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    /// Returns an error if:
    /// - The coordination service cannot be reached
    /// - Service registration is rejected or fails
    /// - The response is malformed or missing the registration token
    #[must_use = "Service registration result should be checked"]
    pub async fn register_service(&self, service_info: ServiceInfo) -> Result<RegistrationToken> {
        let capability = StandardCapability::CoordinationServiceRegistry.id();

        let request = Request::new(
            "register",
            json!({
                "name": service_info.name,
                "capabilities": service_info.capabilities,
                "endpoint": service_info.endpoints.first().unwrap_or(&String::new()),
                "metadata": service_info.metadata,
            }),
        );

        let response = self
            .universal
            .invoke(capability, request)
            .await
            .context("Failed to register service")?;

        let data = response.data()?;
        let token = data
            .get("token")
            .and_then(|v| v.as_str())
            .ok_or_else(|| crate::CliError::Other("Missing token in response".to_string()))?;

        Ok(RegistrationToken {
            token: token.to_string(),
        })
    }

    /// Discover peer services
    ///
    /// Queries the coordination service to find other services providing
    /// specific capabilities.
    ///
    /// # Example
    /// ```ignore
    /// // Forward-looking example - API under development
    /// # use toadstool_cli::ecosystem::adapters::CoordinationAdapter;
    /// # async fn example(coordination: CoordinationAdapter) -> anyhow::Result<()> {
    /// // Find all services providing crypto capabilities
    /// let peers = coordination.discover_peers(Some("crypto.*")).await?;
    ///
    /// for peer in peers {
    ///     println!("Found: {} at {}", peer.name, peer.endpoint);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    /// Returns an error if:
    /// - The coordination service cannot be reached
    /// - Peer discovery fails or times out
    /// - The response is malformed or missing peer information
    #[must_use = "Peer discovery result should be checked"]
    pub async fn discover_peers(&self, capability_filter: Option<&str>) -> Result<Vec<PeerInfo>> {
        let capability = StandardCapability::CoordinationPeerDiscovery.id();

        let request = Request::new(
            "discover",
            json!({
                "capability": capability_filter,
            }),
        );

        let response = self
            .universal
            .invoke(capability, request)
            .await
            .context("Failed to discover peers")?;

        let data = response.data()?;
        let peers = data
            .get("peers")
            .and_then(|v| v.as_array())
            .ok_or_else(|| crate::CliError::Other("Missing peers in response".to_string()))?;

        let mut result = Vec::new();
        for peer in peers {
            if let Some(obj) = peer.as_object() {
                result.push(PeerInfo {
                    name: obj
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    endpoint: obj
                        .get("endpoint")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    capabilities: obj
                        .get("capabilities")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str())
                                .map(String::from)
                                .collect()
                        })
                        .unwrap_or_default(),
                    health: obj
                        .get("health")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string(),
                });
            }
        }

        Ok(result)
    }

    /// Send heartbeat to maintain service registration
    ///
    /// # Errors
    /// Returns an error if:
    /// - The coordination service cannot be reached
    /// - The heartbeat is rejected (invalid or expired token)
    /// - Network communication fails
    #[must_use = "Heartbeat send result should be checked"]
    pub async fn send_heartbeat(&self, token: &RegistrationToken) -> Result<()> {
        let capability = StandardCapability::CoordinationHealthCheck.id();

        let request = Request::new(
            "heartbeat",
            json!({
                "token": token.token,
            }),
        );

        self.universal
            .invoke(capability, request)
            .await
            .context("Failed to send heartbeat")?;

        Ok(())
    }

    /// Unregister service
    ///
    /// # Errors
    /// Returns an error if:
    /// - The coordination service cannot be reached
    /// - The unregistration fails (invalid token, service not found)
    /// - Network communication fails
    #[must_use = "Service unregistration result should be checked"]
    pub async fn unregister_service(&self, token: &RegistrationToken) -> Result<()> {
        let capability = StandardCapability::CoordinationServiceRegistry.id();

        let request = Request::new(
            "unregister",
            json!({
                "token": token.token,
            }),
        );

        self.universal
            .invoke(capability, request)
            .await
            .context("Failed to unregister service")?;

        Ok(())
    }

    /// Get configuration value
    ///
    /// # Errors
    /// Returns an error if:
    /// - The coordination service cannot be reached
    /// - The configuration key is not found
    /// - The response is malformed or missing the value
    #[must_use = "Configuration get result should be checked"]
    pub async fn get_config(&self, key: &str) -> Result<serde_json::Value> {
        let capability = StandardCapability::CoordinationConfigManagement.id();

        let request = Request::new(
            "get",
            json!({
                "key": key,
            }),
        );

        let response = self
            .universal
            .invoke(capability, request)
            .await
            .context("Failed to get configuration")?;

        let data = response.data()?;
        let value = data
            .get("value")
            .cloned()
            .ok_or_else(|| crate::CliError::Other("Missing value in response".to_string()))?;

        Ok(value)
    }

    /// Set configuration value
    ///
    /// # Errors
    /// Returns an error if:
    /// - The coordination service cannot be reached
    /// - The configuration cannot be set (permissions, validation)
    /// - Network communication fails
    #[must_use = "Configuration set result should be checked"]
    pub async fn set_config(&self, key: &str, value: serde_json::Value) -> Result<()> {
        let capability = StandardCapability::CoordinationConfigManagement.id();

        let request = Request::new(
            "set",
            json!({
                "key": key,
                "value": value,
            }),
        );

        self.universal
            .invoke(capability, request)
            .await
            .context("Failed to set configuration")?;

        Ok(())
    }

    /// Acquire distributed lock
    ///
    /// # Errors
    /// Returns an error if:
    /// - The coordination service cannot be reached
    /// - The lock is already held by another process
    /// - The response is malformed or missing the lock ID
    #[must_use = "Lock acquisition result should be checked"]
    pub async fn acquire_lock(&self, lock_name: &str, ttl_seconds: u64) -> Result<LockHandle> {
        let capability = StandardCapability::CoordinationDistributedLock.id();

        let request = Request::new(
            "acquire",
            json!({
                "lock_name": lock_name,
                "ttl_seconds": ttl_seconds,
            }),
        );

        let response = self
            .universal
            .invoke(capability, request)
            .await
            .context("Failed to acquire lock")?;

        let data = response.data()?;
        let lock_id = data
            .get("lock_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| crate::CliError::Other("Missing lock_id in response".to_string()))?;

        Ok(LockHandle {
            lock_name: lock_name.to_string(),
            lock_id: lock_id.to_string(),
        })
    }

    /// Release distributed lock
    ///
    /// # Errors
    /// Returns an error if:
    /// - The coordination service cannot be reached
    /// - The lock cannot be released (already released, expired)
    /// - Network communication fails
    #[must_use = "Lock release result should be checked"]
    pub async fn release_lock(&self, handle: LockHandle) -> Result<()> {
        let capability = StandardCapability::CoordinationDistributedLock.id();

        let request = Request::new(
            "release",
            json!({
                "lock_id": handle.lock_id,
            }),
        );

        self.universal
            .invoke(capability, request)
            .await
            .context("Failed to release lock")?;

        Ok(())
    }
}
