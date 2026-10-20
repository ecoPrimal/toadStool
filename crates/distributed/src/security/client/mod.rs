// SPDX-License-Identifier: AGPL-3.0-or-later
//! Security Unix Socket Client (Pure Rust!)
//!
//! **Design Philosophy**:
//! - **Pure Rust**: Unix sockets, no HTTP/TLS (no ring dependency!)
//! - **Async-first**: Non-blocking operations with tokio
//! - **Local IPC**: Fast, secure primal-to-primal communication
//! - **No hardcoding**: Socket paths discovered at runtime
//! - **TRUE PRIMAL**: Coordination handles external HTTP, we use local IPC
//!
//! ## Architecture
//!
//! ToadStool = Compute orchestration (internal)
//! Security = Security services (local)
//! Communication = JSON-RPC over unix sockets (pure Rust!)

use std::sync::Arc;

#[cfg(unix)]
use base64::Engine;
#[cfg(unix)]
use toadstool_common::primal_sockets::discover_crypto_socket;
#[cfg(unix)]
use toadstool_common::unix_jsonrpc_client::UnixJsonRpcClient;
use toadstool_common::{ToadStoolError, ToadStoolResult};

use super::SecurityConfig;
use super::discovery::SecurityDiscovery;
use super::types::{
    EncryptionRequest, EncryptionResponse, KeyManagementRequest, KeyManagementResponse,
    PermissionResponse, SecurityEndpoint, SignatureResponse,
};
#[cfg(unix)]
use super::types::{
    RevocationRequest, SignatureRequest, ValidationResponse, VerificationRequest,
    VerificationResponse,
};

/// Security Unix Socket Client (Pure Rust!)
///
/// **Design**: JSON-RPC 2.0 over unix sockets (no HTTP, no TLS, no ring!)
/// **TRUE PRIMAL**: Local IPC for compute primal → security primal communication
pub struct SecurityClient {
    discovery: Arc<SecurityDiscovery>,
    #[cfg(unix)]
    rpc_client: UnixJsonRpcClient,
}

impl SecurityClient {
    #[cfg(not(unix))]
    fn unix_unavailable<T>() -> ToadStoolResult<T> {
        Err(ToadStoolError::configuration(
            "Unix socket security client is unavailable on this platform",
        ))
    }

    /// Parse CryptoCapability from JSON response (pure function, testable without network)
    #[doc(hidden)]
    pub fn parse_capabilities_from_json(
        response: &serde_json::Value,
    ) -> toadstool::encryption::CryptoCapability {
        let algorithms: Vec<String> = response
            .get("algorithms")
            .and_then(|v: &serde_json::Value| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v: &serde_json::Value| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_else(|| vec!["chacha20poly1305".to_string(), "aes-256-gcm".to_string()]);

        let security_level = response
            .get("security_level")
            .and_then(|v: &serde_json::Value| v.as_str())
            .map(|s: &str| match s.to_lowercase().as_str() {
                "standard" => toadstool::encryption::SecurityLevel::Standard,
                "hardware_secured" | "hardware" => {
                    toadstool::encryption::SecurityLevel::Maximum
                }
                _ => toadstool::encryption::SecurityLevel::High,
            })
            .unwrap_or(toadstool::encryption::SecurityLevel::High);

        let hardware_backed = response
            .get("hardware_backed")
            .and_then(|v: &serde_json::Value| v.as_bool())
            .unwrap_or_default();

        toadstool::encryption::CryptoCapability {
            algorithms,
            security_level,
            hardware_backed,
        }
    }

    /// Create new Security client with capability-based discovery (RECOMMENDED)
    ///
    /// **Deep Debt Compliant**: Discovers crypto service by capability, not name.
    /// Works with ANY service providing crypto.encryption capability.
    ///
    /// **Pure Rust**: No HTTP client, uses unix sockets!
    ///
    /// # Errors
    /// Returns error if no crypto service is discovered
    pub async fn new_async(config: SecurityConfig) -> ToadStoolResult<Self> {
        #[cfg(unix)]
        {
            let socket_path = discover_crypto_socket().await.map_err(|e| {
                ToadStoolError::configuration(format!(
                    "No crypto service discovered: {e}. Ensure a security/crypto service is running.",
                ))
            })?;

            let rpc_client = UnixJsonRpcClient::new(socket_path);

            Ok(Self {
                discovery: Arc::new(SecurityDiscovery::new(config)),
                rpc_client,
            })
        }
        #[cfg(not(unix))]
        {
            let _ = config;
            Self::unix_unavailable()
        }
    }

    /// Synchronous constructor for testing (no async discovery).
    ///
    /// Uses `get_socket_path_for_capability` directly. Production callers
    /// should use [`new_async`](Self::new_async) for proper capability discovery.
    pub fn new_test(config: SecurityConfig) -> ToadStoolResult<Self> {
        #[cfg(unix)]
        {
            let socket_path =
                toadstool_common::primal_sockets::get_socket_path_for_capability("crypto");
            let rpc_client = UnixJsonRpcClient::new(socket_path);

            Ok(Self {
                discovery: Arc::new(SecurityDiscovery::new(config)),
                rpc_client,
            })
        }
        #[cfg(not(unix))]
        {
            let _ = config;
            Self::unix_unavailable()
        }
    }

    /// Create client with custom socket path (for testing error paths when service unavailable)
    #[cfg(all(unix, test))]
    pub fn new_with_socket_path(
        config: SecurityConfig,
        socket_path: std::path::PathBuf,
    ) -> ToadStoolResult<Self> {
        let rpc_client = UnixJsonRpcClient::new(socket_path);
        Ok(Self {
            discovery: Arc::new(SecurityDiscovery::new(config)),
            rpc_client,
        })
    }

    /// Discover security services
    pub async fn discover(&self) -> ToadStoolResult<Vec<SecurityEndpoint>> {
        self.discovery.discover().await
    }

    /// Query actual capabilities from the crypto service
    pub async fn query_capabilities_async(
        &self,
    ) -> ToadStoolResult<toadstool::encryption::CryptoCapability> {
        #[cfg(not(unix))]
        {
            return Self::unix_unavailable();
        }
        #[cfg(unix)]
        {
            let response: serde_json::Value = self
                .rpc_client
                .call_typed("crypto.capabilities", serde_json::json!({}))
                .await
                .map_err(|e| {
                    tracing::warn!("security/crypto service capabilities query failed: {}", e);
                    ToadStoolError::network(format!(
                        "security/crypto service capabilities query failed: {e}"
                    ))
                })?;

            Ok(Self::parse_capabilities_from_json(&response))
        }
    }

    /// Encrypt data using Security via unix socket
    pub async fn encrypt(&self, request: EncryptionRequest) -> ToadStoolResult<EncryptionResponse> {
        #[cfg(not(unix))]
        {
            let _ = request;
            return Self::unix_unavailable();
        }
        #[cfg(unix)]
        {
            let params = serde_json::to_value(&request).map_err(|e| {
                ToadStoolError::network(format!("Failed to serialize request: {e}"))
            })?;

            self.rpc_client.call_typed("crypto.encrypt", params).await
        }
    }

    /// Decrypt data using Security via unix socket
    pub async fn decrypt(&self, request: EncryptionRequest) -> ToadStoolResult<EncryptionResponse> {
        #[cfg(not(unix))]
        {
            let _ = request;
            return Self::unix_unavailable();
        }
        #[cfg(unix)]
        {
            let params = serde_json::to_value(&request).map_err(|e| {
                ToadStoolError::network(format!("Failed to serialize request: {e}"))
            })?;

            self.rpc_client.call_typed("crypto.decrypt", params).await
        }
    }

    /// Manage keys using Security via unix socket
    pub async fn key_management(
        &self,
        request: KeyManagementRequest,
    ) -> ToadStoolResult<KeyManagementResponse> {
        #[cfg(not(unix))]
        {
            let _ = request;
            return Self::unix_unavailable();
        }
        #[cfg(unix)]
        {
            let params = serde_json::to_value(&request).map_err(|e| {
                ToadStoolError::network(format!("Failed to serialize request: {e}"))
            })?;

            self.rpc_client
                .call_typed("crypto.key_management", params)
                .await
        }
    }

    /// Sign data using Security via unix socket
    pub async fn sign(&self, data: &[u8]) -> ToadStoolResult<SignatureResponse> {
        #[cfg(not(unix))]
        {
            let _ = data;
            return Self::unix_unavailable();
        }
        #[cfg(unix)]
        {
            let request = SignatureRequest {
                request_id: uuid::Uuid::new_v4(),
                data: data.to_vec(),
                key_id: None,
                algorithm: None,
            };

            let params = serde_json::to_value(&request).map_err(|e| {
                ToadStoolError::network(format!("Failed to serialize request: {e}"))
            })?;

            self.rpc_client.call_typed("crypto.sign", params).await
        }
    }

    /// Verify signature using Security via unix socket
    pub async fn verify(
        &self,
        data: &[u8],
        signature: &[u8],
        public_key_id: &str,
    ) -> ToadStoolResult<bool> {
        #[cfg(not(unix))]
        {
            let _ = (data, signature, public_key_id);
            return Self::unix_unavailable();
        }
        #[cfg(unix)]
        {
            let request = VerificationRequest {
                request_id: uuid::Uuid::new_v4(),
                data: data.to_vec(),
                signature: signature.to_vec(),
                public_key_id: public_key_id.to_string(),
            };

            let params = serde_json::to_value(&request).map_err(|e| {
                ToadStoolError::network(format!("Failed to serialize request: {e}"))
            })?;

            let result: VerificationResponse =
                self.rpc_client.call_typed("crypto.verify", params).await?;

            Ok(result.valid)
        }
    }

    /// Create permission using Security via unix socket
    pub async fn create_permission(
        &self,
        request: &crate::security_provider::PermissionRequest,
    ) -> ToadStoolResult<PermissionResponse> {
        #[cfg(not(unix))]
        {
            let _ = request;
            return Self::unix_unavailable();
        }
        #[cfg(unix)]
        {
            let params = serde_json::to_value(request).map_err(|e| {
                ToadStoolError::network(format!("Failed to serialize request: {e}"))
            })?;

            self.rpc_client
                .call_typed("crypto.create_permission", params)
                .await
        }
    }

    /// Validate permission using Security via unix socket
    pub async fn validate_permission(
        &self,
        permission: &crate::security_provider::SecurityPermission,
    ) -> ToadStoolResult<bool> {
        #[cfg(not(unix))]
        {
            let _ = permission;
            return Self::unix_unavailable();
        }
        #[cfg(unix)]
        {
            let params = serde_json::to_value(permission).map_err(|e| {
                ToadStoolError::network(format!("Failed to serialize request: {e}"))
            })?;

            let result: ValidationResponse = self
                .rpc_client
                .call_typed("crypto.validate_permission", params)
                .await?;

            Ok(result.valid)
        }
    }

    /// Revoke permission using Security via unix socket
    pub async fn revoke_permission(
        &self,
        permission_id: &uuid::Uuid,
        reason: &str,
    ) -> ToadStoolResult<()> {
        #[cfg(not(unix))]
        {
            let _ = (permission_id, reason);
            return Self::unix_unavailable();
        }
        #[cfg(unix)]
        {
            let request = RevocationRequest {
                reason: reason.to_string(),
            };

            let mut params = serde_json::to_value(&request).map_err(|e| {
                ToadStoolError::network(format!("Failed to serialize request: {e}"))
            })?;

            if let Some(obj) = params.as_object_mut() {
                obj.insert(
                    "permission_id".to_string(),
                    serde_json::json!(permission_id.to_string()),
                );
            }

            let _: serde_json::Value = self
                .rpc_client
                .call("crypto.revoke_permission", params)
                .await?;

            Ok(())
        }
    }

    /// Retrieve a purpose key from the crypto provider secrets store.
    ///
    /// The key name follows the NUCLEUS Two-Tier Crypto Model convention:
    /// `"nucleus:{family}:purpose:{purpose}"` (e.g. `"nucleus:abc123:purpose:compute"`).
    /// If `family` is `None`, the value is read from `TOADSTOOL_FAMILY_ID`.
    pub async fn retrieve_purpose_key(
        &self,
        purpose: &str,
        family: Option<&str>,
    ) -> ToadStoolResult<toadstool::encryption::EncryptionKey> {
        #[cfg(not(unix))]
        {
            let _ = (purpose, family);
            return Self::unix_unavailable();
        }
        #[cfg(unix)]
        {
            let family_id = match family {
                Some(f) => f.to_string(),
                None => std::env::var(
                    toadstool_common::interned_strings::socket_env::TOADSTOOL_FAMILY_ID,
                )
                .or_else(|_| {
                    std::env::var(toadstool_common::interned_strings::socket_env::TOADSTOOL_FAMILY)
                })
                .or_else(|_| {
                    std::env::var(toadstool_common::interned_strings::socket_env::BIOMEOS_FAMILY_ID)
                })
                .map_err(|_| {
                    ToadStoolError::configuration(
                        "TOADSTOOL_FAMILY_ID not set — cannot derive purpose key name",
                    )
                })?,
            };

            let key_name = format!("nucleus:{family_id}:purpose:{purpose}");

            let params = serde_json::json!({ "name": key_name });
            let response: serde_json::Value = self
                .rpc_client
                .call_typed("secrets.retrieve", params)
                .await
                .map_err(|e| {
                    ToadStoolError::network(format!("secrets.retrieve(\"{key_name}\") failed: {e}"))
                })?;

            let key_material_b64 = response["key"]
                .as_str()
                .or_else(|| response["value"].as_str())
                .or_else(|| response.as_str())
                .ok_or_else(|| {
                    ToadStoolError::runtime(format!(
                        "secrets.retrieve(\"{key_name}\") returned no key material"
                    ))
                })?;

            let key_material = base64::engine::general_purpose::STANDARD
                .decode(key_material_b64)
                .map_err(|e| {
                    ToadStoolError::runtime(format!("purpose key base64 decode failed: {e}"))
                })?;

            let algorithm = response["algorithm"]
                .as_str()
                .unwrap_or("chacha20-poly1305")
                .to_string();

            Ok(toadstool::encryption::EncryptionKey::new(
                key_name,
                key_material,
                algorithm,
                toadstool::encryption::SecurityLevel::High,
            ))
        }
    }

    /// Check health of security services
    pub async fn health_check(&self) -> ToadStoolResult<Vec<SecurityEndpoint>> {
        #[cfg(not(unix))]
        {
            return Self::unix_unavailable();
        }
        #[cfg(unix)]
        {
            let endpoints = self.discovery.discover().await?;

            let mut checked_endpoints = Vec::with_capacity(endpoints.len());

            for mut endpoint in endpoints {
                let start = std::time::Instant::now();

                let health_result: Result<serde_json::Value, _> = self
                    .rpc_client
                    .call_typed("crypto.health", serde_json::json!({}))
                    .await;

                let latency_ms = start.elapsed().as_millis() as u64;

                match health_result {
                    Ok(_response) => {
                        endpoint.healthy = true;
                        endpoint.latency_ms = Some(latency_ms);
                    }
                    Err(_) => {
                        endpoint.healthy = false;
                        endpoint.latency_ms = Some(latency_ms);
                    }
                }

                checked_endpoints.push(endpoint);
            }

            Ok(checked_endpoints)
        }
    }
}

#[cfg(test)]
mod tests;
