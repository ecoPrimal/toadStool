// SPDX-License-Identifier: AGPL-3.0-or-later
use std::sync::Arc;

use super::*;
use crate::security::DistributedCryptoProvider;
use toadstool::encryption::CryptoProvider;
use toadstool_common::interned_strings::capabilities;

#[test]
fn test_security_client_new_creates_client() {
    let config = SecurityConfig::default();
    let result = SecurityClient::new_test(config);
    assert!(result.is_ok());
}

#[test]
fn test_provider_id_returns_security() {
    let client = Arc::new(SecurityClient::new_test(SecurityConfig::default()).unwrap());
    let crypto = DistributedCryptoProvider::Security(Arc::clone(&client));
    assert_eq!(crypto.provider_id(), capabilities::CRYPTO);
}

#[test]
fn test_capabilities_returns_default() {
    let client = Arc::new(SecurityClient::new_test(SecurityConfig::default()).unwrap());
    let crypto = DistributedCryptoProvider::Security(Arc::clone(&client));
    let caps = crypto.capabilities();
    assert!(!caps.algorithms.is_empty());
    assert!(
        caps.algorithms.contains(&"chacha20poly1305".to_string())
            || caps.algorithms.contains(&"aes-256-gcm".to_string())
    );
}

#[test]
fn test_parse_capabilities_security_level_standard() {
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
fn test_parse_capabilities_security_level_enhanced() {
    let json = serde_json::json!({
        "security_level": "enhanced"
    });
    let cap = SecurityClient::parse_capabilities_from_json(&json);
    assert!(matches!(
        cap.security_level,
        toadstool::encryption::SecurityLevel::High
    ));
}

#[test]
fn test_parse_capabilities_missing_algorithms_uses_default() {
    let json = serde_json::json!({"security_level": "standard"});
    let cap = SecurityClient::parse_capabilities_from_json(&json);
    assert_eq!(
        cap.algorithms,
        vec!["chacha20poly1305".to_string(), "aes-256-gcm".to_string()]
    );
}

#[test]
fn test_parse_capabilities_array_with_mixed_types() {
    let json = serde_json::json!({
        "algorithms": ["aes", 42, "gcm", null],
        "security_level": "standard"
    });
    let cap = SecurityClient::parse_capabilities_from_json(&json);
    assert_eq!(cap.algorithms, vec!["aes", "gcm"]);
}

#[test]
fn test_parse_capabilities_hardware_secured() {
    let json = serde_json::json!({
        "algorithms": ["aes-256-gcm"],
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
fn test_parse_capabilities_hardware_variant() {
    let json = serde_json::json!({
        "security_level": "hardware"
    });
    let cap = SecurityClient::parse_capabilities_from_json(&json);
    assert!(matches!(
        cap.security_level,
        toadstool::encryption::SecurityLevel::Maximum
    ));
}

#[test]
fn test_parse_capabilities_empty_response_uses_defaults() {
    let json = serde_json::json!({});
    let cap = SecurityClient::parse_capabilities_from_json(&json);
    assert_eq!(
        cap.algorithms,
        vec!["chacha20poly1305".to_string(), "aes-256-gcm".to_string()]
    );
    assert!(matches!(
        cap.security_level,
        toadstool::encryption::SecurityLevel::High
    ));
    assert!(!cap.hardware_backed);
}

#[test]
fn test_parse_capabilities_unknown_security_level_defaults_to_enhanced() {
    let json = serde_json::json!({
        "security_level": "unknown_level"
    });
    let cap = SecurityClient::parse_capabilities_from_json(&json);
    assert!(matches!(
        cap.security_level,
        toadstool::encryption::SecurityLevel::High
    ));
}

#[test]
fn test_parse_capabilities_custom_algorithms() {
    let json = serde_json::json!({
        "algorithms": ["custom-algo-1", "custom-algo-2"],
        "security_level": "standard"
    });
    let cap = SecurityClient::parse_capabilities_from_json(&json);
    assert_eq!(cap.algorithms.len(), 2);
    assert!(cap.algorithms.contains(&"custom-algo-1".to_string()));
    assert!(cap.algorithms.contains(&"custom-algo-2".to_string()));
}

#[tokio::test]
async fn test_query_capabilities_service_unavailable() {
    let config = SecurityConfig::default();
    let nonexistent = std::path::PathBuf::from("/tmp/nonexistent-security-test-12345.sock");
    let client = SecurityClient::new_with_socket_path(config, nonexistent).unwrap();
    let result = client.query_capabilities_async().await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("capabilities")
            || msg.contains("socket")
            || msg.contains("Connection")
            || msg.contains("unavailable"),
        "expected security/capabilities-related error, got: {msg}"
    );
}

#[tokio::test]
async fn test_encrypt_service_unavailable() {
    use crate::security::types::{EncryptionOperation, SecurityLevel};
    let config = SecurityConfig::default();
    let nonexistent = std::path::PathBuf::from("/tmp/nonexistent-security-encrypt.sock");
    let client = SecurityClient::new_with_socket_path(config, nonexistent).unwrap();
    let request = EncryptionRequest {
        request_id: uuid::Uuid::new_v4(),
        operation: EncryptionOperation::Encrypt,
        data: b"secret".to_vec(),
        key_id: None,
        algorithm: None,
        security_level: SecurityLevel::Standard,
    };
    let result = client.encrypt(request).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_decrypt_service_unavailable() {
    use crate::security::types::{EncryptionOperation, SecurityLevel};
    let config = SecurityConfig::default();
    let nonexistent = std::path::PathBuf::from("/tmp/nonexistent-security-decrypt.sock");
    let client = SecurityClient::new_with_socket_path(config, nonexistent).unwrap();
    let request = EncryptionRequest {
        request_id: uuid::Uuid::new_v4(),
        operation: EncryptionOperation::Decrypt,
        data: vec![],
        key_id: None,
        algorithm: None,
        security_level: SecurityLevel::Standard,
    };
    let result = client.decrypt(request).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_health_check_service_unavailable() {
    let config = SecurityConfig::default();
    let nonexistent = std::path::PathBuf::from("/tmp/nonexistent-security-health.sock");
    let client = SecurityClient::new_with_socket_path(config, nonexistent).unwrap();
    let result = client.health_check().await;
    assert!(result.is_ok());
    let endpoints = result.unwrap();
    assert!(endpoints.is_empty() || endpoints.iter().all(|e| !e.healthy));
}

#[tokio::test]
async fn test_sign_service_unavailable() {
    let config = SecurityConfig::default();
    let nonexistent = std::path::PathBuf::from("/tmp/nonexistent-security-sign.sock");
    let client = SecurityClient::new_with_socket_path(config, nonexistent).unwrap();
    let result = client.sign(b"data to sign").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_verify_service_unavailable() {
    let config = SecurityConfig::default();
    let nonexistent = std::path::PathBuf::from("/tmp/nonexistent-security-verify.sock");
    let client = SecurityClient::new_with_socket_path(config, nonexistent).unwrap();
    let result = client.verify(b"data", b"sig", "key-1").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_key_management_service_unavailable() {
    use crate::security::types::KeyOperation;
    let config = SecurityConfig::default();
    let nonexistent = std::path::PathBuf::from("/tmp/nonexistent-security-km.sock");
    let client = SecurityClient::new_with_socket_path(config, nonexistent).unwrap();
    let request = KeyManagementRequest {
        request_id: uuid::Uuid::new_v4(),
        operation: KeyOperation::Generate,
        key_id: None,
        security_level: None,
    };
    let result = client.key_management(request).await;
    assert!(result.is_err());
}

// ═══════════════════════════════════════════════════════════
// retrieve_purpose_key tests (Phase 55)
// ═══════════════════════════════════════════════════════════

#[tokio::test]
async fn test_retrieve_purpose_key_service_unavailable() {
    let config = SecurityConfig::default();
    let nonexistent = std::path::PathBuf::from("/tmp/nonexistent-security-secrets.sock");
    let client = SecurityClient::new_with_socket_path(config, nonexistent).unwrap();
    let result = client
        .retrieve_purpose_key("compute", Some("test-family"))
        .await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("secrets.retrieve") || err.contains("failed"),
        "error should mention secrets.retrieve: {err}"
    );
}

#[tokio::test]
async fn test_retrieve_purpose_key_builds_correct_key_name() {
    let config = SecurityConfig::default();
    let nonexistent = std::path::PathBuf::from("/tmp/nonexistent-security-keyname.sock");
    let client = SecurityClient::new_with_socket_path(config, nonexistent).unwrap();
    let result = client
        .retrieve_purpose_key("compute", Some("my-family-id"))
        .await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("nucleus:my-family-id:purpose:compute"),
        "error should contain the full key name: {err}"
    );
}
