// SPDX-License-Identifier: AGPL-3.0-or-later
// Core EcosystemIntegrator implementation - refactored by protocol
//
// ⚠️ LEGACY INTEGRATION LAYER
//
// This implementation historically coupled to fixed product names (crypto, coordination,
// storage stacks) and ports, violating infant discovery principles.
//
// **Migration Status**: The new capability-based Adapter API (`adapters/`) is available
// and should be used for new code. This legacy layer is maintained for CLI compatibility
// and will be migrated to Adapters in v0.2.0.
//
// See:
// - `crates/cli/src/ecosystem/adapters/` - New capability-based API
// - `CAPABILITY_BASED_DISCOVERY_STANDARD.md` in wateringHole — ecosystem standard

use crate::{CliContextExt, Result};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;
use tracing::{info, warn};

// Legacy integration layer; uses deprecated EcosystemService for ServiceConnection migration
#[expect(deprecated, reason = "entire impl block wraps legacy EcosystemService; callers migrating to capability-based discovery")]
impl EcosystemIntegrator {
    /// Create a new ecosystem integrator (legacy discovery layer)
    #[must_use]
    pub fn new() -> Self {
        Self {
            endpoints: HashMap::new(),
            connections: HashMap::new(),
            credentials: None,
        }
    }

    /// Discover ecosystem services on the network
    pub async fn discover_services(
        &mut self,
        service_types: Vec<String>,
        timeout_secs: u64,
    ) -> Result<DiscoveryResult> {
        info!("🔍 Scanning for ecosystem services");
        let start_time = std::time::Instant::now();

        // Service ports now discovered dynamically via capabilities
        // (Removed deprecated get_standard_service_ports() call)

        // Perform network discovery using capability-based approach
        let mut discovered_services = Vec::new();

        // Use DiscoveryEngine for capability-based service resolution
        // This replaces the legacy hardcoded service names approach
        use toadstool_common::infant_discovery::{DiscoveryEngine, CapabilityDiscovery};
        let discovery_engine = DiscoveryEngine::new();

        // Define capabilities instead of hardcoded service names
        use crate::ecosystem::constants::capability_categories;
        let scan_capabilities = if service_types.is_empty() {
            vec![
                capability_categories::NETWORK.to_string(),      // coordination / discovery
                capability_categories::CRYPTO.to_string(),         // crypto / security
                capability_categories::STORAGE.to_string(),       // storage
                capability_categories::ORCHESTRATION.to_string(), // Orchestration capabilities
            ]
        } else {
            use crate::templates::capability_helpers::dependency_label_to_capability;

            service_types
                .into_iter()
                .map(|st| {
                    if capability_categories::ALL.contains(&st.as_str()) {
                        return st;
                    }
                    match dependency_label_to_capability(&st) {
                        "crypto" | "security" => capability_categories::CRYPTO.to_string(),
                        "coordination" | "orchestration" | "discovery" => {
                            capability_categories::NETWORK.to_string()
                        }
                        "storage" => capability_categories::STORAGE.to_string(),
                        "compute" | "os" => capability_categories::COMPUTE.to_string(),
                        "routing" | "intelligence" => {
                            capability_categories::ORCHESTRATION.to_string()
                        }
                        _ => st,
                    }
                })
                .collect()
        };

        for capability in &scan_capabilities {
            info!("🔍 Discovering services with capability: {}", capability);

            // Use capability-based discovery with timeout
            match timeout(
                Duration::from_secs(timeout_secs),
                discovery_engine.discover_all(capability),
            )
            .await
            {
                Ok(Ok(services)) => {
                    for discovered in services {
                        info!(
                            "✅ Discovered service with capability '{}': {}",
                            capability,
                            discovered.endpoint
                        );
                        
                        // Convert DiscoveredService to ServiceEndpoint format
                        if let Ok(addr) = discovered.endpoint.parse() {
                            // Map capabilities to service types (capability-based)
                            let service_type = match capability.as_str() {
                                "network" | "discovery" | "orchestration" | "coordination" => {
                                    EcosystemService::Discovery
                                }
                                "crypto" | "pki" | "security" => EcosystemService::Crypto,
                                "storage" => EcosystemService::Storage,
                                _ => EcosystemService::Unknown(capability.clone()),
                            };
                            
                            let service = ServiceEndpoint {
                                service_type,
                                address: addr,
                                version: Arc::from(
                                    discovered
                                        .metadata
                                        .version
                                        .as_deref()
                                        .unwrap_or("unknown")
                                ),
                                capabilities: vec![capability.clone()],
                                trust_level: TrustLevel::Discovered,
                            };
                            discovered_services.push(service);
                        }
                    }
                }
                Ok(Err(e)) => {
                    warn!("Failed to discover capability '{}': {:?}", capability, e);
                }
                Err(_) => {
                    warn!("Timeout discovering capability '{}'", capability);
                }
            }
        }

        // Verify discovered services
        let mut verified_count = 0;
        for service in &mut discovered_services {
            match verify_service(service).await {
                Ok(true) => {
                    service.trust_level = TrustLevel::Verified;
                    verified_count += 1;
                }
                Ok(false) => {
                    warn!("⚠️  Service verification failed: {}", service.address);
                    service.trust_level = TrustLevel::Discovered;
                }
                Err(e) => {
                    warn!("⚠️  Service verification error: {}", e);
                    service.trust_level = TrustLevel::Unknown;
                }
            }
        }

        // Store discovered services
        for service in &discovered_services {
            let key = format!("{}:{}", service.service_type.name(), service.address);
            self.endpoints.insert(key, service.clone());
        }

        let scan_duration = start_time.elapsed();

        info!(
            "🎯 Discovery complete: {} services found, {} verified",
            discovered_services.len(),
            verified_count
        );

        let total_count = discovered_services.len();
        Ok(DiscoveryResult {
            services: discovered_services,
            scan_duration: Duration::from_secs(scan_duration.as_secs()),
            total_discovered: total_count,
            verified_count,
        })
    }

    /// Register with orchestrator via capability discovery
    pub async fn register_with_orchestrator(
        &mut self,
        endpoint: String,
        _token: Option<String>,
    ) -> Result<()> {
        info!("🎯 Registering with orchestrator via capability discovery");

        // ✅ MODERNIZED: Uses capability-based CoordinationAdapter
        use crate::ecosystem::adapters::{AdapterFactory, coordination::ServiceInfo};

        // Create adapter factory
        let factory = AdapterFactory::new();
        let coordination = factory.coordination_adapter()?;

        // Build service information
        let mut service_info = ServiceInfo::new(
            toadstool_common::constants::PRIMAL_NAME,
            toadstool_common::constants::PRIMAL_NAME,
        )
        .with_endpoint(endpoint.clone())
        .with_capability("wasm-execution")
        .with_capability("container-runtime")
        .with_capability("universal-substrate")
        .with_capability("sovereign-compute");
        service_info.metadata.insert("version".into(), "0.1.0".into());
        service_info.metadata.insert("platform".into(), std::env::consts::OS.into());
        service_info.metadata.insert("arch".into(), std::env::consts::ARCH.into());

        // Register with coordination service (discovers service dynamically)
        match coordination.register_service(service_info).await {
            Ok(reg_token) => {
                info!("✅ Successfully registered with coordination service");
                info!("   Token: {}", reg_token.token);
                info!("   Endpoint: {}", endpoint);

                // Parse endpoint for storage
                let addr: SocketAddr = endpoint
                    .parse()
                    .context(format!("Invalid endpoint: {endpoint}"))?;

                // Store connection (capability-based, not hardcoded)
                #[expect(deprecated, reason = "ServiceConnection uses deprecated EcosystemService during migration")]
                let connection = ServiceConnection {
                    endpoint: ServiceEndpoint {
                        service_type: EcosystemService::Discovery,
                        address: addr,
                        version: Arc::from("unknown"),
                        capabilities: vec!["discovery".to_string(), "coordination".to_string()],
                        trust_level: TrustLevel::Verified,
                    },
                    status: ConnectionStatus::Connected,
                    last_heartbeat: std::time::SystemTime::now(),
                };

                self.connections.insert("coordination".to_string(), connection);
                Ok(())
            }
            Err(e) => {
                warn!("⚠️  Failed to register with coordination service: {}", e);
                Err(e)
            }
        }
    }

    /// Install cryptographic permissions via capability discovery
    ///
    /// This method replaces fixed crypto product coupling with capability-based
    /// discovery. It works with ANY crypto service that provides permission management.
    ///
    /// # Example
    ///  
    /// ```rust,ignore
    /// // Modern capability-based discovery
    /// integrator.install_crypto_permissions(path, false).await?;
    ///
    /// // NEW (capability-based, service-agnostic)
    /// integrator.install_crypto_permissions(path, false).await?;
    /// ```
    ///
    /// # Benefits
    /// - Works with any crypto / security provider (KMS, Vault, HSM, etc.)
    /// - No hardcoded service names or ports
    /// - Automatic failover to backup services
    /// - Future-proof: works with services that don't exist yet
    pub async fn install_crypto_permissions(
        &mut self,
        permissions_path: PathBuf,
        validate_only: bool,
    ) -> Result<()> {
        use crate::ecosystem::adapters::AdapterFactory;

        // Use factory to get crypto adapter (no boilerplate!)
        let factory = AdapterFactory::new();
        let crypto = factory.crypto_adapter()?;

        // Use capability-based crypto adapter
        crypto
            .install_permissions(&permissions_path, validate_only)
            .await
    }

    // ✅ REMOVED: legacy install_*_permissions() — deprecated since 0.1.0
    // Use install_crypto_permissions() instead for capability-based discovery

    /// Connect to distributed storage via capability discovery
    pub async fn connect_distributed_storage(
        &mut self,
        endpoint: String,
        mount_point: PathBuf,
        _dataset: Option<String>,
    ) -> Result<StorageMount> {
        info!("🏠 Connecting to distributed storage via capability discovery");

        // ✅ MODERNIZED: Uses capability-based StorageAdapter
        use crate::ecosystem::adapters::{AdapterFactory, storage::{StorageRequirements, AccessMode}};

        // Create adapter factory
        let factory = AdapterFactory::new();
        let storage = factory.storage_adapter()?;

        // Build storage requirements
        let requirements = StorageRequirements {
            mount_point: mount_point.clone(),
            capacity_gb: None,
            access_mode: AccessMode::ReadWrite,
            encryption: false,
        };

        // Mount distributed storage (discovers service dynamically)
        match storage.mount_distributed_storage(requirements).await {
            Ok(mount_info) => {
                info!("✅ Successfully mounted distributed storage");
                info!("   Dataset: {}", mount_info.dataset_name);
                info!("   Mount point: {}", mount_info.mount_point.display());
                info!("   Endpoint: {}", mount_info.endpoint);

                // Parse endpoint for storage
                let addr: SocketAddr = endpoint
                    .parse()
                    .context(format!("Invalid endpoint: {endpoint}"))?;

                // Store connection (capability-based, not hardcoded)
                #[expect(deprecated, reason = "ServiceConnection uses deprecated EcosystemService during migration")]
                let connection = ServiceConnection {
                    endpoint: ServiceEndpoint {
                        service_type: EcosystemService::Storage,
                        address: addr,
                        version: Arc::from("unknown"),
                        capabilities: vec!["storage".to_string(), "zfs".to_string()],
                        trust_level: TrustLevel::Verified,
                    },
                    status: ConnectionStatus::Connected,
                    last_heartbeat: std::time::SystemTime::now(),
                };

                self.connections.insert("storage".to_string(), connection);

                // Convert MountInfo to storage mount struct for backward compatibility
                Ok(StorageMount {
                    dataset_name: mount_info.dataset_name,
                    mount_point: mount_info.mount_point,
                    endpoint: mount_info.endpoint,
                    zfs_dataset: None,
                    access_mode: "read-write".to_string(),
                    encryption_key: None,
                })
            }
            Err(e) => {
                warn!("⚠️  Failed to connect to distributed storage: {}", e);
                Err(e)
            }
        }
    }

    /// Show ecosystem connection status
    ///
    /// # Errors
    /// Returns an error if:
    /// - Status information cannot be retrieved from services
    /// - Output formatting fails
    /// - Display rendering encounters errors
    ///
    /// Zero-Copy Optimization: Takes `&str` instead of `String` to avoid allocation.
    #[must_use = "Ecosystem status display result should be checked"]
    pub async fn show_ecosystem_status(&self, format: &str) -> Result<()> {
        match format {
            "json" => {
                let status = EcosystemStatus {
                    endpoints: self.endpoints.clone(),
                    active_connections: self.connections.len(),
                    total_discovered: self.endpoints.len(),
                    credential_status: self.credentials.is_some(),
                };
                println!("{}", serde_json::to_string_pretty(&status)?);
            }
            "table" => {
                self.print_ecosystem_table().await?;
            }
            _ => {
                self.print_ecosystem_table().await?;
            }
        }

        Ok(())
    }

    // Internal helper methods

    #[expect(clippy::unused_async, reason = "async for CLI API consistency")]
    async fn print_ecosystem_table(&self) -> Result<()> {
        if self.endpoints.is_empty() && self.connections.is_empty() {
            println!("No ecosystem services discovered or connected");
            return Ok(());
        }

        println!("\n🌐 Ecosystem Services Status:");
        println!("{}", "=".repeat(80));

        if !self.endpoints.is_empty() {
            println!("\n📡 Discovered Services:");
            println!(
                "{:<20} {:<20} {:<15} {:<15}",
                "SERVICE", "ADDRESS", "TRUST", "CAPABILITIES"
            );
            println!("{}", "-".repeat(70));

            for endpoint in self.endpoints.values() {
                println!(
                    "{:<20} {:<20} {:<15} {:<15}",
                    endpoint.service_type.name(),
                    endpoint.address.to_string(),
                    format!("{:?}", endpoint.trust_level),
                    endpoint.capabilities.join(",")
                );
            }
        }

        if !self.connections.is_empty() {
            println!("\n🔗 Active Connections:");
            println!(
                "{:<20} {:<20} {:<15} {:<20}",
                "SERVICE", "ADDRESS", "STATUS", "LAST HEARTBEAT"
            );
            println!("{}", "-".repeat(75));

            for connection in self.connections.values() {
                println!(
                    "{:<20} {:<20} {:<15} {:<20}",
                    connection.endpoint.service_type.name(),
                    connection.endpoint.address.to_string(),
                    format!("{:?}", connection.status),
                    toadstool_common::system_time_serde::format_rfc3339(
                        connection.last_heartbeat,
                    )
                );
            }
        }

        if let Some(creds) = &self.credentials {
            println!("\n🔐 Authentication Status:");
            println!("   Identity: {}", creds.identity);
            println!("   Certificates: {}", creds.certificates.len());
        }

        println!();
        Ok(())
    }


}

const SERVICE_VERIFY_TIMEOUT_SECS: u64 = 2;

/// Verify a discovered service by attempting a TCP connection.
async fn verify_service(service: &ServiceEndpoint) -> Result<bool> {
    match timeout(
        Duration::from_secs(SERVICE_VERIFY_TIMEOUT_SECS),
        tokio::net::TcpStream::connect(&service.address),
    )
    .await
    {
        Ok(Ok(_)) => {
            info!("✅ Service verified: {}", service.address);
            Ok(true)
        }
        Ok(Err(e)) => {
            warn!("⚠️  Service verification failed: {}", e);
            Ok(false)
        }
        Err(_) => {
            warn!("⚠️  Service verification timeout");
            Ok(false)
        }
    }
}

// Tests are included in the main integrator_impl.rs file

