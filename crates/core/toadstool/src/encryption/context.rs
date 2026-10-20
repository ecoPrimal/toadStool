// SPDX-License-Identifier: AGPL-3.0-or-later

use std::marker::PhantomData;
use std::sync::Arc;
use uuid::Uuid;

use crate::{ToadStoolError, ToadStoolResult};

use super::EncryptionKey;
use super::SecurityLevel;
use super::capability::CryptoCapability;
use super::provider::{CryptoProvider, CryptoProviderRegistry, NoopCryptoProvider};
use super::{EncryptedInput, EncryptedOutput, EncryptionConfig};

/// Encryption context for an execution
///
/// **Design**: Holds encryption state for an execution lifecycle
pub struct EncryptionContext<P: CryptoProvider = NoopCryptoProvider> {
    /// Execution identifier
    execution_id: Uuid,

    /// Encryption configuration
    config: EncryptionConfig,

    /// Discovered crypto provider (runtime discovery)
    provider: Option<Arc<P>>,

    /// Active encryption key
    active_key: Option<EncryptionKey>,

    _marker: PhantomData<P>,
}

impl<P: CryptoProvider> EncryptionContext<P> {
    /// Create new encryption context
    ///
    /// **Design**: No provider passed in - discovered at runtime
    pub fn new(execution_id: Uuid, config: EncryptionConfig) -> Self {
        Self {
            execution_id,
            config,
            provider: None,
            active_key: None,
            _marker: PhantomData,
        }
    }

    /// Discover and set crypto provider
    ///
    /// **Capability-based**: Query registry for crypto capability
    ///
    /// # Errors
    ///
    /// Returns error if provider lookup fails.
    pub fn discover_provider(
        &mut self,
        registry: &CryptoProviderRegistry<P>,
    ) -> ToadStoolResult<()> {
        let capability = CryptoCapability {
            algorithms: self.config.preferred_algorithms.clone(),
            security_level: self.config.min_security_level,
            hardware_backed: matches!(
                self.config.min_security_level,
                SecurityLevel::Maximum
            ),
        };

        self.provider = registry.find_provider(&capability)?;
        Ok(())
    }

    /// Decrypt input data
    ///
    /// **Design**: Transparent decryption, returns raw bytes
    ///
    /// # Errors
    ///
    /// Returns error if security level is too low, no provider is configured, key fetch fails, or decryption fails.
    pub async fn decrypt_input(&mut self, encrypted: &EncryptedInput) -> ToadStoolResult<Vec<u8>> {
        if encrypted.security_level < self.config.min_security_level {
            return Err(ToadStoolError::security(format!(
                "Encryption security level {:?} below minimum {:?}",
                encrypted.security_level, self.config.min_security_level
            )));
        }

        let provider = self
            .provider
            .as_ref()
            .ok_or_else(|| {
                ToadStoolError::configuration("No crypto provider available for decryption")
            })?
            .clone();

        let key = self
            .get_or_fetch_key(&encrypted.key_id, provider.as_ref())
            .await?;

        provider
            .decrypt(&encrypted.payload, &key, &encrypted.metadata)
            .await
    }

    /// Encrypt output data
    ///
    /// **Design**: Symmetric encryption using same key
    ///
    /// # Errors
    ///
    /// Returns error if result encryption is disabled, no provider is configured, key generation fails, or encryption fails.
    pub async fn encrypt_output(&mut self, data: &[u8]) -> ToadStoolResult<EncryptedOutput> {
        if !self.config.encrypt_results {
            return Err(ToadStoolError::configuration(
                "Result encryption not enabled in config",
            ));
        }

        let provider = self
            .provider
            .as_ref()
            .ok_or_else(|| {
                ToadStoolError::configuration("No crypto provider available for encryption")
            })?
            .clone();

        let key = if let Some(k) = &self.active_key {
            k.clone()
        } else {
            let new_key = provider
                .generate_key(self.config.min_security_level)
                .await?;
            self.active_key = Some(new_key.clone());
            new_key
        };

        let (payload, metadata) = provider.encrypt(data, &key).await?;

        Ok(EncryptedOutput {
            payload,
            key_id: key.id.clone(),
            metadata,
            security_level: key.security_level,
        })
    }

    async fn get_or_fetch_key(
        &mut self,
        key_id: &str,
        provider: &P,
    ) -> ToadStoolResult<EncryptionKey> {
        if let Some(ref key) = self.active_key
            && key.id == key_id
        {
            return Ok(key.clone());
        }

        let key = provider.get_key(key_id).await?;
        self.active_key = Some(key.clone());
        Ok(key)
    }

    /// Check if encryption is available
    pub fn is_available(&self) -> bool {
        self.provider.is_some()
    }

    /// Check if encryption is required
    pub const fn is_required(&self) -> bool {
        self.config.required
    }
}

impl<P: CryptoProvider> std::fmt::Debug for EncryptionContext<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EncryptionContext")
            .field("execution_id", &self.execution_id)
            .field("config", &self.config)
            .field("provider", &self.provider.as_ref().map(|p| p.provider_id()))
            .field("active_key", &self.active_key.as_ref().map(|k| &k.id))
            .finish()
    }
}
