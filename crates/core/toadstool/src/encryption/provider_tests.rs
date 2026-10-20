// SPDX-License-Identifier: AGPL-3.0-or-later
use std::sync::Arc;

use crate::ToadStoolResult;

use super::*;

#[tokio::test]
async fn noop_crypto_provider_returns_no_provider_registered() {
    let provider = NoopCryptoProvider;
    let key = EncryptionKey::default();
    let data = b"test";

    let encrypt = provider.encrypt(data, &key).await;
    assert!(encrypt.is_err());
    assert!(
        encrypt
            .unwrap_err()
            .to_string()
            .contains("no crypto provider registered")
    );

    let decrypt = provider
        .decrypt(
            &EncryptedPayload::default(),
            &key,
            &EncryptionMetadata::default(),
        )
        .await;
    assert!(decrypt.is_err());

    let gen_key = provider.generate_key(SecurityLevel::Standard).await;
    assert!(gen_key.is_err());

    let get = provider.get_key("key-id").await;
    assert!(get.is_err());

    let health = provider.health_check().await.unwrap();
    assert!(!health.available);
    assert!(health.error.is_some());
}

// Mock provider for testing
struct MockProvider {
    id: String,
    capability: CryptoCapability,
}

impl CryptoProvider for MockProvider {
    fn provider_id(&self) -> &str {
        &self.id
    }

    fn capabilities(&self) -> &CryptoCapability {
        &self.capability
    }

    fn encrypt<'a>(
        &'a self,
        _data: &'a [u8],
        _key: &'a EncryptionKey,
    ) -> impl std::future::Future<Output = ToadStoolResult<(EncryptedPayload, EncryptionMetadata)>>
    + Send
    + 'a {
        async { Ok((EncryptedPayload::default(), EncryptionMetadata::default())) }
    }

    fn decrypt<'a>(
        &'a self,
        _encrypted: &'a EncryptedPayload,
        _key: &'a EncryptionKey,
        _metadata: &'a EncryptionMetadata,
    ) -> impl std::future::Future<Output = ToadStoolResult<Vec<u8>>> + Send + 'a {
        async { Ok(vec![]) }
    }

    fn generate_key(
        &self,
        _security_level: SecurityLevel,
    ) -> impl std::future::Future<Output = ToadStoolResult<EncryptionKey>> + Send + '_ {
        async { Ok(EncryptionKey::default()) }
    }

    fn get_key<'a>(
        &'a self,
        _key_id: &'a str,
    ) -> impl std::future::Future<Output = ToadStoolResult<EncryptionKey>> + Send + 'a {
        async { Ok(EncryptionKey::default()) }
    }

    fn health_check(
        &self,
    ) -> impl std::future::Future<Output = ToadStoolResult<ProviderHealth>> + Send + '_ {
        async { Ok(ProviderHealth::healthy(10)) }
    }
}

#[tokio::test]
async fn test_registry_register() {
    let registry = CryptoProviderRegistry::<MockProvider>::new();
    let provider = Arc::new(MockProvider {
        id: "test".to_string(),
        capability: CryptoCapability {
            algorithms: vec!["chacha20poly1305".to_string()],
            security_level: SecurityLevel::Standard,
            hardware_backed: false,
        },
    });

    assert!(registry.register(provider).is_ok());

    let providers = registry.list_providers();
    assert_eq!(providers.len(), 1);
    assert!(providers.contains(&"test".to_string()));
}

#[tokio::test]
async fn test_registry_find_provider() {
    let registry = CryptoProviderRegistry::<MockProvider>::new();
    let provider = Arc::new(MockProvider {
        id: "test".to_string(),
        capability: CryptoCapability {
            algorithms: vec!["chacha20poly1305".to_string()],
            security_level: SecurityLevel::High,
            hardware_backed: false,
        },
    });

    registry.register(provider).unwrap();

    let required = CryptoCapability {
        algorithms: vec!["chacha20poly1305".to_string()],
        security_level: SecurityLevel::Standard,
        hardware_backed: false,
    };

    let found = registry.find_provider(&required).unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().provider_id(), "test");
}

#[test]
#[expect(
    clippy::float_cmp,
    reason = "exact comparison intended in this context"
)] // literal just set by healthy()/unhealthy()
fn test_provider_health_healthy() {
    let health = ProviderHealth::healthy(25);
    assert!(health.available);
    assert_eq!(health.latency_ms, 25);
    assert_eq!(health.load, 0.0);
    assert!(health.error.is_none());
}

#[test]
#[expect(
    clippy::float_cmp,
    reason = "exact comparison intended in this context"
)] // literal just set by unhealthy()
fn test_provider_health_unhealthy() {
    let health = ProviderHealth::unhealthy("connection refused");
    assert!(!health.available);
    assert_eq!(health.latency_ms, 0);
    assert_eq!(health.load, 0.0);
    assert_eq!(health.error.as_deref(), Some("connection refused"));
}

#[test]
fn test_provider_health_unhealthy_from_string() {
    let msg = "timeout".to_string();
    let health = ProviderHealth::unhealthy(msg);
    assert!(!health.available);
    assert_eq!(health.error.as_deref(), Some("timeout"));
}

#[test]
fn test_provider_health_debug_clone() {
    let health = ProviderHealth::healthy(10);
    let cloned = health.clone();
    assert_eq!(health.available, cloned.available);
    assert_eq!(health.latency_ms, cloned.latency_ms);
}

#[tokio::test]
async fn test_registry_default() {
    let registry = CryptoProviderRegistry::<MockProvider>::default();
    assert!(registry.list_providers().is_empty());
}

#[tokio::test]
async fn test_registry_register_duplicate_fails() {
    let registry = CryptoProviderRegistry::<MockProvider>::new();
    let provider = Arc::new(MockProvider {
        id: "dup".to_string(),
        capability: CryptoCapability {
            algorithms: vec!["aes-256-gcm".to_string()],
            security_level: SecurityLevel::Standard,
            hardware_backed: false,
        },
    });

    assert!(registry.register(provider.clone()).is_ok());
    let result = registry.register(provider);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("already registered")
    );
}

#[tokio::test]
async fn test_registry_unregister_not_found() {
    let registry = CryptoProviderRegistry::<MockProvider>::new();
    let result = registry.unregister("nonexistent");
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not found"));
}

#[tokio::test]
async fn test_registry_unregister() {
    let registry = CryptoProviderRegistry::<MockProvider>::new();
    let provider = Arc::new(MockProvider {
        id: "unreg".to_string(),
        capability: CryptoCapability {
            algorithms: vec!["chacha20poly1305".to_string()],
            security_level: SecurityLevel::Standard,
            hardware_backed: false,
        },
    });
    registry.register(provider).unwrap();
    assert_eq!(registry.list_providers().len(), 1);

    assert!(registry.unregister("unreg").is_ok());
    assert!(registry.list_providers().is_empty());
}

#[tokio::test]
async fn test_registry_find_provider_no_match() {
    let registry = CryptoProviderRegistry::<MockProvider>::new();
    let provider = Arc::new(MockProvider {
        id: "test".to_string(),
        capability: CryptoCapability {
            algorithms: vec!["aes-256-gcm".to_string()],
            security_level: SecurityLevel::Standard,
            hardware_backed: false,
        },
    });
    registry.register(provider).unwrap();

    let required = CryptoCapability {
        algorithms: vec!["nonexistent-alg".to_string()],
        security_level: SecurityLevel::Standard,
        hardware_backed: false,
    };
    let found = registry.find_provider(&required).unwrap();
    assert!(found.is_none());
}

#[tokio::test]
async fn test_registry_find_all_providers() {
    let registry = CryptoProviderRegistry::<MockProvider>::new();
    let p1 = Arc::new(MockProvider {
        id: "p1".to_string(),
        capability: CryptoCapability {
            algorithms: vec!["chacha20poly1305".to_string()],
            security_level: SecurityLevel::Standard,
            hardware_backed: false,
        },
    });
    let p2 = Arc::new(MockProvider {
        id: "p2".to_string(),
        capability: CryptoCapability {
            algorithms: vec!["chacha20poly1305".to_string(), "aes-256-gcm".to_string()],
            security_level: SecurityLevel::High,
            hardware_backed: false,
        },
    });
    registry.register(p1).unwrap();
    registry.register(p2).unwrap();

    let required = CryptoCapability {
        algorithms: vec!["chacha20poly1305".to_string()],
        security_level: SecurityLevel::Standard,
        hardware_backed: false,
    };
    let found = registry.find_all_providers(&required).unwrap();
    assert_eq!(found.len(), 2);
}

#[tokio::test]
async fn test_registry_get_provider() {
    let registry = CryptoProviderRegistry::<MockProvider>::new();
    let provider = Arc::new(MockProvider {
        id: "get-me".to_string(),
        capability: CryptoCapability {
            algorithms: vec!["chacha20poly1305".to_string()],
            security_level: SecurityLevel::Standard,
            hardware_backed: false,
        },
    });
    registry.register(provider).unwrap();

    let got = registry.get_provider("get-me").unwrap();
    assert_eq!(got.provider_id(), "get-me");

    let result = registry.get_provider("missing");
    assert!(result.is_err());
}

#[tokio::test]
async fn test_registry_health_check_all() {
    let registry = CryptoProviderRegistry::<MockProvider>::new();
    let provider = Arc::new(MockProvider {
        id: "healthy".to_string(),
        capability: CryptoCapability {
            algorithms: vec!["chacha20poly1305".to_string()],
            security_level: SecurityLevel::Standard,
            hardware_backed: false,
        },
    });
    registry.register(provider).unwrap();

    let health_map = registry.health_check_all().await;
    assert_eq!(health_map.len(), 1);
    assert!(health_map.get("healthy").unwrap().available);
    assert_eq!(health_map.get("healthy").unwrap().latency_ms, 10);
}

#[tokio::test]
async fn test_registry_list_providers_empty() {
    let registry = CryptoProviderRegistry::<MockProvider>::new();
    let list = registry.list_providers();
    assert!(list.is_empty());
}

#[tokio::test]
async fn test_registry_list_providers_multiple() {
    let registry = CryptoProviderRegistry::<MockProvider>::new();
    for (id, alg) in [("a", "aes"), ("b", "chacha")] {
        let provider = Arc::new(MockProvider {
            id: id.to_string(),
            capability: CryptoCapability {
                algorithms: vec![alg.to_string()],
                security_level: SecurityLevel::Standard,
                hardware_backed: false,
            },
        });
        registry.register(provider).unwrap();
    }
    let list = registry.list_providers();
    assert_eq!(list.len(), 2);
    assert!(list.contains(&"a".to_string()));
    assert!(list.contains(&"b".to_string()));
}
