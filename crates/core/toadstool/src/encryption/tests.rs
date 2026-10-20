// SPDX-License-Identifier: AGPL-3.0-or-later

use uuid::Uuid;

use super::provider::NoopCryptoProvider;

use super::*;

#[test]
fn test_encryption_context_builder() {
    let ctx = EncryptionContextBuilder::<NoopCryptoProvider>::new(Uuid::new_v4())
        .required(true)
        .encrypt_results(true)
        .security_level(SecurityLevel::High)
        .build();

    assert!(ctx.is_required());
    assert!(!ctx.is_available()); // No provider discovered yet
}

#[test]
fn test_default_config() {
    let config = EncryptionConfig::default();
    assert!(!config.required);
    assert!(!config.encrypt_results);
    assert_eq!(config.min_security_level, SecurityLevel::Standard);
}

#[test]
fn test_security_level_ordering() {
    assert!(SecurityLevel::Standard < SecurityLevel::High);
    assert!(SecurityLevel::High < SecurityLevel::Maximum);
}

#[test]
fn test_builder_key_id() {
    let ctx = EncryptionContextBuilder::<NoopCryptoProvider>::new(Uuid::new_v4())
        .key_id("my-key-123")
        .build();
    assert!(!ctx.is_available());
}

#[test]
fn test_builder_algorithms() {
    let algorithms = vec!["aes-256-gcm".to_string(), "xsalsa20".to_string()];
    let ctx = EncryptionContextBuilder::<NoopCryptoProvider>::new(Uuid::new_v4())
        .algorithms(algorithms)
        .build();
    assert!(!ctx.is_available());
}

#[test]
fn test_builder_all_options() {
    let ctx = EncryptionContextBuilder::<NoopCryptoProvider>::new(Uuid::new_v4())
        .required(true)
        .encrypt_results(true)
        .security_level(SecurityLevel::Maximum)
        .key_id("full-config-key")
        .algorithms(vec!["aes-256-gcm".to_string()])
        .build();
    assert!(ctx.is_required());
    assert!(!ctx.is_available());
}

#[test]
fn test_encryption_context_new() {
    let config = EncryptionConfig {
        required: true,
        preferred_algorithms: vec!["test-alg".to_string()],
        key_id: Some("new-key".to_string()),
        encrypt_results: true,
        min_security_level: SecurityLevel::High,
    };
    let ctx = EncryptionContext::<super::provider::NoopCryptoProvider>::new(Uuid::new_v4(), config);
    assert!(ctx.is_required());
    assert!(!ctx.is_available());
}

#[test]
fn test_context_not_available_without_provider() {
    let ctx = EncryptionContextBuilder::<NoopCryptoProvider>::new(Uuid::new_v4()).build();
    assert!(!ctx.is_available());
}

#[test]
fn test_context_required_reflects_config() {
    let ctx_required = EncryptionContextBuilder::<NoopCryptoProvider>::new(Uuid::new_v4())
        .required(true)
        .build();
    let ctx_optional = EncryptionContextBuilder::<NoopCryptoProvider>::new(Uuid::new_v4())
        .required(false)
        .build();
    assert!(ctx_required.is_required());
    assert!(!ctx_optional.is_required());
}

#[test]
fn test_security_level_equality() {
    assert_eq!(SecurityLevel::Standard, SecurityLevel::Standard);
    assert_eq!(SecurityLevel::High, SecurityLevel::High);
    assert_eq!(
        SecurityLevel::Maximum,
        SecurityLevel::Maximum
    );
}

#[test]
fn test_security_level_all_orderings() {
    use std::cmp::Ordering;
    assert_eq!(
        SecurityLevel::Standard.cmp(&SecurityLevel::High),
        Ordering::Less
    );
    assert_eq!(
        SecurityLevel::Standard.cmp(&SecurityLevel::Maximum),
        Ordering::Less
    );
    assert_eq!(
        SecurityLevel::High.cmp(&SecurityLevel::Maximum),
        Ordering::Less
    );
    assert_eq!(
        SecurityLevel::High.cmp(&SecurityLevel::Standard),
        Ordering::Greater
    );
    assert_eq!(
        SecurityLevel::Maximum.cmp(&SecurityLevel::Standard),
        Ordering::Greater
    );
    assert_eq!(
        SecurityLevel::Maximum.cmp(&SecurityLevel::High),
        Ordering::Greater
    );
}

#[test]
fn test_default_config_algorithms() {
    let config = EncryptionConfig::default();
    assert_eq!(
        config.preferred_algorithms,
        vec!["chacha20poly1305".to_string(), "aes-256-gcm".to_string()]
    );
}

#[test]
fn test_default_config_key_id_is_none() {
    let config = EncryptionConfig::default();
    assert!(config.key_id.is_none());
}

#[test]
fn test_encryption_context_debug() {
    let ctx = EncryptionContextBuilder::<NoopCryptoProvider>::new(Uuid::new_v4()).build();
    let _ = format!("{ctx:?}");
}

#[test]
fn test_encrypted_input_serialization() {
    let input = EncryptedInput {
        payload: EncryptedPayload::new(vec![1, 2, 3, 4, 5]),
        key_id: "test-key".to_string(),
        metadata: EncryptionMetadata {
            algorithm: "chacha20poly1305".to_string(),
            nonce: vec![10, 20, 30],
            aad: None,
            kdf_info: None,
            encrypted_at: 1_234_567_890,
        },
        security_level: SecurityLevel::Standard,
    };
    let json = serde_json::to_string(&input).unwrap();
    let deserialized: EncryptedInput = serde_json::from_str(&json).unwrap();
    assert_eq!(input.payload.ciphertext, deserialized.payload.ciphertext);
    assert_eq!(input.key_id, deserialized.key_id);
    assert_eq!(input.metadata.algorithm, deserialized.metadata.algorithm);
    assert_eq!(input.security_level, deserialized.security_level);
}

#[test]
fn test_encrypted_output_serialization() {
    let output = EncryptedOutput {
        payload: EncryptedPayload::new(vec![6, 7, 8, 9, 10]),
        key_id: "output-key".to_string(),
        metadata: EncryptionMetadata {
            algorithm: "aes-256-gcm".to_string(),
            nonce: vec![1, 2, 3],
            aad: Some(vec![4, 5, 6]),
            kdf_info: None,
            encrypted_at: 9_876_543_210,
        },
        security_level: SecurityLevel::High,
    };
    let json = serde_json::to_string(&output).unwrap();
    let deserialized: EncryptedOutput = serde_json::from_str(&json).unwrap();
    assert_eq!(output.payload.ciphertext, deserialized.payload.ciphertext);
    assert_eq!(output.key_id, deserialized.key_id);
    assert_eq!(output.metadata.algorithm, deserialized.metadata.algorithm);
    assert_eq!(output.security_level, deserialized.security_level);
}

#[test]
fn test_ecosystem_config_serialization() {
    let config = EncryptionConfig {
        required: true,
        preferred_algorithms: vec!["aes-256-gcm".to_string()],
        key_id: Some("serial-key".to_string()),
        encrypt_results: true,
        min_security_level: SecurityLevel::Maximum,
    };
    let json = serde_json::to_string(&config).unwrap();
    let deserialized: EncryptionConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(config.required, deserialized.required);
    assert_eq!(
        config.preferred_algorithms,
        deserialized.preferred_algorithms
    );
    assert_eq!(config.key_id, deserialized.key_id);
    assert_eq!(config.encrypt_results, deserialized.encrypt_results);
    assert_eq!(config.min_security_level, deserialized.min_security_level);
}

#[test]
fn test_builder_default_values() {
    let ctx = EncryptionContextBuilder::<NoopCryptoProvider>::new(Uuid::new_v4()).build();
    assert!(!ctx.is_required());
    assert!(!ctx.is_available());
}

#[tokio::test]
async fn test_discover_provider_empty_registry_sets_none() {
    use super::provider::CryptoProviderRegistry;

    let mut ctx = EncryptionContext::<super::provider::NoopCryptoProvider>::new(
        Uuid::new_v4(),
        EncryptionConfig::default(),
    );
    let registry = CryptoProviderRegistry::<super::provider::NoopCryptoProvider>::new();

    let result = ctx.discover_provider(&registry);
    assert!(result.is_ok());
    assert!(!ctx.is_available());
}

#[tokio::test]
async fn test_discover_provider_with_registered_provider() {
    use std::sync::Arc;

    use super::capability::CryptoCapability;
    use super::provider::{CryptoProvider, CryptoProviderRegistry, ProviderHealth};

    struct TestProvider;
    impl CryptoProvider for TestProvider {
        fn provider_id(&self) -> &'static str {
            "test-crypto"
        }
        fn capabilities(&self) -> &CryptoCapability {
            static CAP: std::sync::OnceLock<CryptoCapability> = std::sync::OnceLock::new();
            CAP.get_or_init(|| CryptoCapability {
                algorithms: vec!["chacha20poly1305".to_string()],
                security_level: SecurityLevel::Standard,
                hardware_backed: false,
            })
        }
        fn encrypt<'a>(
            &'a self,
            data: &'a [u8],
            _key: &'a super::types::EncryptionKey,
        ) -> impl std::future::Future<
            Output = crate::ToadStoolResult<(
                super::types::EncryptedPayload,
                super::types::EncryptionMetadata,
            )>,
        > + Send
        + 'a {
            async move {
                Ok((
                    super::types::EncryptedPayload::new(data.to_vec()),
                    super::types::EncryptionMetadata::default(),
                ))
            }
        }
        fn decrypt<'a>(
            &'a self,
            encrypted: &'a super::types::EncryptedPayload,
            _key: &'a super::types::EncryptionKey,
            _metadata: &'a super::types::EncryptionMetadata,
        ) -> impl std::future::Future<Output = crate::ToadStoolResult<Vec<u8>>> + Send + 'a
        {
            async move { Ok(encrypted.ciphertext.clone()) }
        }
        fn generate_key(
            &self,
            level: SecurityLevel,
        ) -> impl std::future::Future<
            Output = crate::ToadStoolResult<super::types::EncryptionKey>,
        > + Send
        + '_ {
            async move {
                Ok(super::types::EncryptionKey::new(
                    "gen-key".to_string(),
                    vec![1u8; 32],
                    "chacha20poly1305".to_string(),
                    level,
                ))
            }
        }
        fn get_key<'a>(
            &'a self,
            key_id: &'a str,
        ) -> impl std::future::Future<
            Output = crate::ToadStoolResult<super::types::EncryptionKey>,
        > + Send
        + 'a {
            async move {
                Ok(super::types::EncryptionKey::new(
                    key_id.to_string(),
                    vec![1u8; 32],
                    "chacha20poly1305".to_string(),
                    SecurityLevel::Standard,
                ))
            }
        }
        fn health_check(
            &self,
        ) -> impl std::future::Future<Output = crate::ToadStoolResult<ProviderHealth>> + Send + '_
        {
            async { Ok(ProviderHealth::healthy(1)) }
        }
    }

    let mut ctx = EncryptionContextBuilder::<TestProvider>::new(Uuid::new_v4())
        .encrypt_results(true)
        .build();
    let registry = CryptoProviderRegistry::<TestProvider>::new();
    registry.register(Arc::new(TestProvider)).expect("register");

    let result = ctx.discover_provider(&registry);
    assert!(result.is_ok());
    assert!(ctx.is_available());
}

#[tokio::test]
async fn test_decrypt_input_without_provider_returns_error() {
    let mut ctx = EncryptionContext::<super::provider::NoopCryptoProvider>::new(
        Uuid::new_v4(),
        EncryptionConfig::default(),
    );
    let encrypted = EncryptedInput {
        payload: EncryptedPayload::new(vec![1, 2, 3]),
        key_id: "key-1".to_string(),
        metadata: EncryptionMetadata::default(),
        security_level: SecurityLevel::Standard,
    };

    let result = ctx.decrypt_input(&encrypted).await;
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("No crypto provider")
    );
}

#[tokio::test]
async fn test_decrypt_input_security_level_below_minimum_returns_error() {
    let mut ctx = EncryptionContextBuilder::<NoopCryptoProvider>::new(Uuid::new_v4())
        .security_level(SecurityLevel::Maximum)
        .build();
    let encrypted = EncryptedInput {
        payload: EncryptedPayload::new(vec![1, 2, 3]),
        key_id: "key-1".to_string(),
        metadata: EncryptionMetadata::default(),
        security_level: SecurityLevel::Standard,
    };

    let result = ctx.decrypt_input(&encrypted).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("security level"));
}

#[tokio::test]
async fn test_encrypt_output_without_encrypt_results_returns_error() {
    let mut ctx = EncryptionContextBuilder::<NoopCryptoProvider>::new(Uuid::new_v4())
        .encrypt_results(false)
        .build();

    let result = ctx.encrypt_output(b"hello").await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not enabled"));
}

#[tokio::test]
async fn test_encrypt_output_without_provider_returns_error() {
    let mut ctx = EncryptionContextBuilder::<NoopCryptoProvider>::new(Uuid::new_v4())
        .encrypt_results(true)
        .build();

    let result = ctx.encrypt_output(b"hello").await;
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("No crypto provider")
    );
}
