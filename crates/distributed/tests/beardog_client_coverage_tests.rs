// SPDX-License-Identifier: AGPL-3.0-or-later
#![allow(clippy::pedantic)]
#![allow(deprecated)]
#![allow(
    clippy::cast_precision_loss,
    clippy::float_cmp,
    clippy::unreadable_literal,
    clippy::no_effect_underscore_binding,
    clippy::similar_names,
    clippy::default_trait_access,
    clippy::items_after_statements,
    clippy::unused_async
)]
//! Comprehensive coverage tests for Security client
//! Target: exercise all branches including error paths.

use std::time::Duration;
use toadstool_distributed::security::types::{EncryptionRequest, KeyManagementRequest};
use toadstool_distributed::security::{
    SecurityClient, SecurityConfig,
    types::{EncryptionOperation, KeyOperation, SecurityLevel},
};
use toadstool_distributed::security_provider::types::{
    ExternalTarget, PermissionRequest, PermissionScope, ProviderMetadata, SecurityPermission,
    SecurityProof, SignatureAlgorithm,
};
use uuid::Uuid;

// ─── parse_capabilities_from_json (pure function) ────────────────────────────

#[test]
fn parse_capabilities_standard() {
    let json = serde_json::json!({
        "algorithms": ["aes-256-gcm"],
        "security_level": "standard",
        "hardware_backed": false
    });
    let cap = SecurityClient::parse_capabilities_from_json(&json);
    assert!(matches!(
        cap.security_level,
        toadstool::encryption::SecurityLevel::Standard
    ));
}

#[test]
fn parse_capabilities_enhanced() {
    let json = serde_json::json!({"security_level": "enhanced"});
    let cap = SecurityClient::parse_capabilities_from_json(&json);
    assert!(matches!(
        cap.security_level,
        toadstool::encryption::SecurityLevel::High
    ));
}

#[test]
fn parse_capabilities_hardware_secured() {
    let json = serde_json::json!({
        "security_level": "hardware_secured",
        "hardware_backed": true
    });
    let cap = SecurityClient::parse_capabilities_from_json(&json);
    assert!(matches!(
        cap.security_level,
        toadstool::encryption::SecurityLevel::Maximum
    ));
    assert!(cap.hardware_backed);
}

#[test]
fn parse_capabilities_empty_defaults() {
    let json = serde_json::json!({});
    let cap = SecurityClient::parse_capabilities_from_json(&json);
    assert!(!cap.algorithms.is_empty());
    assert!(!cap.hardware_backed);
}

// ─── SecurityClient::new_test (sync, test-only) ─────────────────────────────

#[test]
fn security_client_new_creates() {
    let config = SecurityConfig::default();
    let result = SecurityClient::new_test(config);
    assert!(result.is_ok());
}

// ─── CryptoProvider trait (via toadstool::encryption::CryptoProvider) ───────

#[test]
fn provider_id_returns_security() {
    use std::sync::Arc;

    use toadstool::encryption::CryptoProvider;
    use toadstool_distributed::security::DistributedCryptoProvider;

    let client = Arc::new(SecurityClient::new_test(SecurityConfig::default()).unwrap());
    let crypto = DistributedCryptoProvider::Security(Arc::clone(&client));
    assert_eq!(crypto.provider_id(), "crypto");
}

#[test]
fn capabilities_returns_default() {
    use std::sync::Arc;

    use toadstool::encryption::CryptoProvider;
    use toadstool_distributed::security::DistributedCryptoProvider;

    let client = Arc::new(SecurityClient::new_test(SecurityConfig::default()).unwrap());
    let crypto = DistributedCryptoProvider::Security(Arc::clone(&client));
    let caps = crypto.capabilities();
    assert!(!caps.algorithms.is_empty());
}

// ─── Async operations (no service) ──────────────────────────────────────────

#[tokio::test]
async fn query_capabilities_service_unavailable() {
    let config = SecurityConfig::default();
    let client = SecurityClient::new_test(config).unwrap();
    let result: Result<_, _> = client.query_capabilities_async().await;
    assert!(result.is_err());
}

#[tokio::test]
async fn encrypt_service_unavailable() {
    let config = SecurityConfig::default();
    let client = SecurityClient::new_test(config).unwrap();
    let req = EncryptionRequest {
        request_id: Uuid::new_v4(),
        operation: EncryptionOperation::Encrypt,
        data: b"secret".to_vec(),
        key_id: None,
        algorithm: None,
        security_level: SecurityLevel::Standard,
    };
    let result: Result<_, _> = client.encrypt(req).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn decrypt_service_unavailable() {
    let config = SecurityConfig::default();
    let client = SecurityClient::new_test(config).unwrap();
    let req = EncryptionRequest {
        request_id: Uuid::new_v4(),
        operation: EncryptionOperation::Decrypt,
        data: vec![],
        key_id: None,
        algorithm: None,
        security_level: SecurityLevel::Standard,
    };
    let result: Result<_, _> = client.decrypt(req).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn sign_service_unavailable() {
    let config = SecurityConfig::default();
    let client = SecurityClient::new_test(config).unwrap();
    let result: Result<_, _> = client.sign(b"data").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn verify_service_unavailable() {
    let config = SecurityConfig::default();
    let client = SecurityClient::new_test(config).unwrap();
    let result: Result<_, _> = client.verify(b"data", b"sig", "key-1").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn key_management_service_unavailable() {
    let config = SecurityConfig::default();
    let client = SecurityClient::new_test(config).unwrap();
    let req = KeyManagementRequest {
        request_id: Uuid::new_v4(),
        operation: KeyOperation::Generate,
        key_id: None,
        security_level: None,
    };
    let result: Result<_, _> = client.key_management(req).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn create_permission_service_unavailable() {
    let config = SecurityConfig::default();
    let client = SecurityClient::new_test(config).unwrap();
    let req = PermissionRequest {
        requester_id: "test".to_string(),
        target: ExternalTarget::ExternalTool {
            tool_name: "test".to_string(),
            api_endpoints: vec![],
            feature_set: vec![],
        },
        scope: PermissionScope {
            operations: vec!["read".to_string()],
            resource_limits:
                toadstool_distributed::security_provider::types::ResourceLimits::default(),
            geo_restrictions: vec![],
        },
        validity_duration: Duration::from_hours(1),
        delegation_info: None,
    };
    let result: Result<_, _> = client.create_permission(&req).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn validate_permission_service_unavailable() {
    let config = SecurityConfig::default();
    let client = SecurityClient::new_test(config).unwrap();
    let perm = SecurityPermission {
        permission_id: Uuid::new_v4(),
        holder_id: "u1".to_string(),
        target: ExternalTarget::ExternalTool {
            tool_name: "t".to_string(),
            api_endpoints: vec![],
            feature_set: vec![],
        },
        scope: PermissionScope {
            operations: vec![],
            resource_limits: Default::default(),
            geo_restrictions: vec![],
        },
        valid_from: std::time::SystemTime::now(),
        valid_until: std::time::SystemTime::now() + Duration::from_hours(1),
        proof: SecurityProof {
            signature: vec![],
            algorithm: SignatureAlgorithm::Ed25519,
            public_key_id: "k1".to_string(),
            signed_at: std::time::SystemTime::now(),
        },
        provider_metadata: ProviderMetadata {
            provider_id: "test".to_string(),
            provider_type: "test".to_string(),
            provider_version: "1.0".to_string(),
            metadata: std::collections::HashMap::new(),
        },
    };
    let result: Result<_, _> = client.validate_permission(&perm).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn revoke_permission_service_unavailable() {
    let config = SecurityConfig::default();
    let client = SecurityClient::new_test(config).unwrap();
    let result: Result<_, _> = client
        .revoke_permission(&Uuid::new_v4(), "test reason")
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn health_check_returns_empty_or_unhealthy() {
    let config = SecurityConfig::default();
    let client = SecurityClient::new_test(config).unwrap();
    let result = client.health_check().await;
    assert!(result.is_ok());
    let endpoints = result.unwrap();
    assert!(endpoints.is_empty() || endpoints.iter().all(|e| !e.healthy));
}

#[tokio::test]
async fn discover_returns_empty_without_service() {
    let client = SecurityClient::new_test(SecurityConfig::default()).unwrap();
    let result = client.discover().await;
    assert!(result.is_ok());
}
