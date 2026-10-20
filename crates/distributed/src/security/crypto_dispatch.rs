// SPDX-License-Identifier: AGPL-3.0-or-later
//! `CryptoProvider` dispatch for security backends (IPC client, etc.).

use std::future::Future;
use std::sync::Arc;

use toadstool_common::interned_strings::capabilities;
use toadstool_common::{ToadStoolError, ToadStoolResult};

use super::client::SecurityClient;
use super::types::{
    EncryptionRequest, KeyManagementRequest, KeyManagementResponse, KeyOperationResult,
};

/// Crypto provider backed by a concrete security integration.
#[derive(Clone)]
pub enum DistributedCryptoProvider {
    /// Unix-socket JSON-RPC security primal (`SecurityClient`).
    Security(Arc<SecurityClient>),
}

impl toadstool::encryption::CryptoProvider for DistributedCryptoProvider {
    fn provider_id(&self) -> &str {
        match self {
            Self::Security(_) => capabilities::CRYPTO,
        }
    }

    fn capabilities(&self) -> &toadstool::encryption::CryptoCapability {
        static CAPABILITIES: std::sync::OnceLock<toadstool::encryption::CryptoCapability> =
            std::sync::OnceLock::new();

        CAPABILITIES.get_or_init(|| toadstool::encryption::CryptoCapability {
            algorithms: vec!["chacha20poly1305".to_string(), "aes-256-gcm".to_string()],
            security_level: toadstool::encryption::SecurityLevel::High,
            hardware_backed: false,
        })
    }

    fn encrypt<'a>(
        &'a self,
        data: &'a [u8],
        key: &'a toadstool::encryption::EncryptionKey,
    ) -> impl Future<
        Output = ToadStoolResult<(
            toadstool::encryption::EncryptedPayload,
            toadstool::encryption::EncryptionMetadata,
        )>,
    > + Send
    + 'a {
        let Self::Security(client) = self;
        let client = Arc::clone(client);
        async move {
            let request = EncryptionRequest {
                request_id: uuid::Uuid::new_v4(),
                operation: super::types::EncryptionOperation::Encrypt,
                data: data.to_vec(),
                key_id: Some(key.id.clone()),
                algorithm: Some(key.algorithm.clone()),
                security_level: match key.security_level {
                    toadstool::encryption::SecurityLevel::Standard => {
                        super::types::SecurityLevel::Standard
                    }
                    toadstool::encryption::SecurityLevel::High => {
                        super::types::SecurityLevel::High
                    }
                    toadstool::encryption::SecurityLevel::Maximum => {
                        super::types::SecurityLevel::Maximum
                    }
                },
            };

            let response = client.encrypt(request).await?;

            let payload = toadstool::encryption::EncryptedPayload::new(response.data);
            let metadata = toadstool::encryption::EncryptionMetadata {
                algorithm: response.algorithm,
                nonce: Vec::new(),
                aad: None,
                kdf_info: None,
                encrypted_at: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64,
            };

            Ok((payload, metadata))
        }
    }

    fn decrypt<'a>(
        &'a self,
        encrypted: &'a toadstool::encryption::EncryptedPayload,
        key: &'a toadstool::encryption::EncryptionKey,
        _metadata: &'a toadstool::encryption::EncryptionMetadata,
    ) -> impl Future<Output = ToadStoolResult<Vec<u8>>> + Send + 'a {
        let Self::Security(client) = self;
        let client = Arc::clone(client);
        let ciphertext = encrypted.ciphertext.clone();
        async move {
            let request = EncryptionRequest {
                request_id: uuid::Uuid::new_v4(),
                operation: super::types::EncryptionOperation::Decrypt,
                data: ciphertext,
                key_id: Some(key.id.clone()),
                algorithm: Some(key.algorithm.clone()),
                security_level: match key.security_level {
                    toadstool::encryption::SecurityLevel::Standard => {
                        super::types::SecurityLevel::Standard
                    }
                    toadstool::encryption::SecurityLevel::High => {
                        super::types::SecurityLevel::High
                    }
                    toadstool::encryption::SecurityLevel::Maximum => {
                        super::types::SecurityLevel::Maximum
                    }
                },
            };

            let response = client.decrypt(request).await?;
            Ok(response.data)
        }
    }

    fn generate_key(
        &self,
        security_level: toadstool::encryption::SecurityLevel,
    ) -> impl Future<Output = ToadStoolResult<toadstool::encryption::EncryptionKey>> + Send + '_
    {
        let Self::Security(client) = self;
        let client = Arc::clone(client);
        async move {
            let request = KeyManagementRequest {
                request_id: uuid::Uuid::new_v4(),
                operation: super::types::KeyOperation::Generate,
                key_id: None,
                security_level: Some(match security_level {
                    toadstool::encryption::SecurityLevel::Standard => {
                        super::types::SecurityLevel::Standard
                    }
                    toadstool::encryption::SecurityLevel::High => {
                        super::types::SecurityLevel::High
                    }
                    toadstool::encryption::SecurityLevel::Maximum => {
                        super::types::SecurityLevel::Maximum
                    }
                }),
            };

            let response = client.key_management(request).await?;

            match response.result {
                KeyOperationResult::Generated { key_id, algorithm } => {
                    Ok(toadstool::encryption::EncryptionKey::new(
                        key_id,
                        Vec::new(),
                        algorithm,
                        security_level,
                    ))
                }
                KeyOperationResult::Error { message } => Err(ToadStoolError::runtime(format!(
                    "security/crypto service key generation failed: {message}"
                ))),
                _ => Err(ToadStoolError::runtime(
                    "Unexpected response from security/crypto service",
                )),
            }
        }
    }

    fn get_key<'a>(
        &'a self,
        key_id: &'a str,
    ) -> impl Future<Output = ToadStoolResult<toadstool::encryption::EncryptionKey>> + Send + 'a
    {
        let Self::Security(client) = self;
        let client = Arc::clone(client);
        let key_id_owned = key_id.to_string();
        async move {
            let request = KeyManagementRequest {
                request_id: uuid::Uuid::new_v4(),
                operation: super::types::KeyOperation::Get,
                key_id: Some(key_id_owned),
                security_level: None,
            };

            let response: KeyManagementResponse = client.key_management(request).await?;

            match response.result {
                KeyOperationResult::Retrieved {
                    key_id,
                    key_material,
                    algorithm,
                } => Ok(toadstool::encryption::EncryptionKey::new(
                    key_id,
                    key_material,
                    algorithm,
                    toadstool::encryption::SecurityLevel::Standard,
                )),
                KeyOperationResult::Error { message } => Err(ToadStoolError::not_found(format!(
                    "security/crypto service key not found: {message}"
                ))),
                _ => Err(ToadStoolError::runtime(
                    "Unexpected response from security/crypto service",
                )),
            }
        }
    }

    fn health_check(
        &self,
    ) -> impl Future<Output = ToadStoolResult<toadstool::encryption::provider::ProviderHealth>> + Send + '_
    {
        let Self::Security(client) = self;
        let client = Arc::clone(client);
        async move {
            let endpoints = client.health_check().await?;

            if endpoints.is_empty() {
                return Ok(toadstool::encryption::provider::ProviderHealth::unhealthy(
                    "No security/crypto endpoints available",
                ));
            }

            let healthy_count = endpoints.iter().filter(|e| e.healthy).count();
            if healthy_count == 0 {
                return Ok(toadstool::encryption::provider::ProviderHealth::unhealthy(
                    "All security/crypto service endpoints unhealthy",
                ));
            }

            let avg_latency =
                endpoints.iter().filter_map(|e| e.latency_ms).sum::<u64>() / healthy_count as u64;

            Ok(toadstool::encryption::provider::ProviderHealth::healthy(
                avg_latency,
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use toadstool::encryption::CryptoProvider;

    fn make_provider() -> DistributedCryptoProvider {
        use crate::security::SecurityConfig;
        let config = SecurityConfig::default();
        let client = SecurityClient::new_test(config).unwrap();
        DistributedCryptoProvider::Security(Arc::new(client))
    }

    #[test]
    fn provider_id_is_crypto_capability() {
        let p = make_provider();
        assert_eq!(p.provider_id(), capabilities::CRYPTO);
    }

    #[test]
    fn capabilities_include_expected_algorithms() {
        let p = make_provider();
        let caps = p.capabilities();
        assert!(caps.algorithms.contains(&"chacha20poly1305".to_string()));
        assert!(caps.algorithms.contains(&"aes-256-gcm".to_string()));
        assert!(!caps.hardware_backed);
    }
}
