// SPDX-License-Identifier: AGPL-3.0-or-later
//! Security client capability parsing tests — JSON → typed capability

use crate::security::SecurityClient;
use crate::security::types::{
    EncryptionOperation, EncryptionRequest, KeyManagementRequest, KeyOperation, SecurityLevel,
    SignatureRequest, VerificationRequest,
};

#[test]
fn test_parse_capabilities_from_json_full() {
    let json = serde_json::json!({
        "algorithms": ["aes-256-gcm", "chacha20poly1305"],
        "security_level": "enhanced",
        "hardware_backed": true
    });
    let cap = SecurityClient::parse_capabilities_from_json(&json);
    assert_eq!(cap.algorithms, vec!["aes-256-gcm", "chacha20poly1305"]);
    assert!(matches!(
        cap.security_level,
        toadstool::encryption::SecurityLevel::High
    ));
    assert!(cap.hardware_backed);
}

#[test]
fn test_parse_capabilities_from_json_standard_level() {
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
fn test_parse_capabilities_from_json_hardware_secured() {
    let json = serde_json::json!({
        "algorithms": [],
        "security_level": "hardware_secured",
        "hardware_backed": true
    });
    let cap = SecurityClient::parse_capabilities_from_json(&json);
    assert!(matches!(
        cap.security_level,
        toadstool::encryption::SecurityLevel::Maximum
    ));
}

#[test]
fn test_parse_capabilities_from_json_hardware_alias() {
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
fn test_parse_capabilities_from_json_empty_defaults() {
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
fn test_parse_capabilities_from_json_unknown_level_defaults_to_enhanced() {
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
fn test_parse_capabilities_from_json_filters_non_string_algorithms() {
    let json = serde_json::json!({
        "algorithms": ["aes", 123, "chacha", null, "gcm"],
        "security_level": "standard",
        "hardware_backed": false
    });
    let cap = SecurityClient::parse_capabilities_from_json(&json);
    assert_eq!(cap.algorithms, vec!["aes", "chacha", "gcm"]);
}

#[test]
fn test_encryption_request_construction_for_encrypt() {
    let request = EncryptionRequest {
        request_id: uuid::Uuid::new_v4(),
        operation: EncryptionOperation::Encrypt,
        data: vec![0x01, 0x02, 0x03, 0x04, 0x05],
        key_id: Some("enc-key-1".to_string()),
        algorithm: Some("aes-256-gcm".to_string()),
        security_level: SecurityLevel::High,
    };
    let params = serde_json::to_value(&request).unwrap();
    assert!(params.get("operation").is_some());
    assert_eq!(params["data"].as_array().unwrap().len(), 5);
}

#[test]
fn test_encryption_request_construction_for_decrypt() {
    let request = EncryptionRequest {
        request_id: uuid::Uuid::new_v4(),
        operation: EncryptionOperation::Decrypt,
        data: vec![0xAA, 0xBB, 0xCC],
        key_id: Some("dec-key".to_string()),
        algorithm: Some("chacha20poly1305".to_string()),
        security_level: SecurityLevel::Standard,
    };
    let params = serde_json::to_value(&request).unwrap();
    assert!(params.get("key_id").is_some());
}

#[test]
fn test_key_management_request_construction_generate() {
    let request = KeyManagementRequest {
        request_id: uuid::Uuid::new_v4(),
        operation: KeyOperation::Generate,
        key_id: None,
        security_level: Some(SecurityLevel::Maximum),
    };
    let params = serde_json::to_value(&request).unwrap();
    assert!(params.get("operation").is_some());
}

#[test]
fn test_key_management_request_construction_get() {
    let request = KeyManagementRequest {
        request_id: uuid::Uuid::new_v4(),
        operation: KeyOperation::Get,
        key_id: Some("fetch-key-123".to_string()),
        security_level: None,
    };
    let params = serde_json::to_value(&request).unwrap();
    assert_eq!(params["key_id"].as_str(), Some("fetch-key-123"));
}

#[test]
fn test_signature_request_with_key_and_algorithm() {
    let req = SignatureRequest {
        request_id: uuid::Uuid::new_v4(),
        data: vec![1, 2, 3, 4, 5, 6],
        key_id: Some("sig-key".to_string()),
        algorithm: Some("ed25519".to_string()),
    };
    let params = serde_json::to_value(&req).unwrap();
    assert_eq!(params["key_id"].as_str(), Some("sig-key"));
    assert_eq!(params["algorithm"].as_str(), Some("ed25519"));
}

#[test]
fn test_verification_request_construction() {
    let request = VerificationRequest {
        request_id: uuid::Uuid::new_v4(),
        data: vec![1, 2, 3],
        signature: vec![4, 5, 6, 7, 8],
        public_key_id: "pub-key-99".to_string(),
    };
    let params = serde_json::to_value(&request).unwrap();
    assert_eq!(params["public_key_id"].as_str(), Some("pub-key-99"));
}

#[test]
fn test_revocation_request_construction() {
    use crate::security::types::RevocationRequest;
    let request = RevocationRequest {
        reason: "security incident".to_string(),
    };
    let params = serde_json::to_value(&request).unwrap();
    assert_eq!(params["reason"].as_str(), Some("security incident"));
}

#[test]
fn test_encryption_response_parsing_success() {
    use crate::security::types::EncryptionResponse;
    let resp = EncryptionResponse {
        request_id: uuid::Uuid::new_v4(),
        data: vec![0x11, 0x22, 0x33],
        key_id: "key-456".to_string(),
        algorithm: "chacha20".to_string(),
        metadata: serde_json::json!({"iv": "abc123"}),
    };
    let json = serde_json::to_value(&resp).unwrap();
    let parsed: EncryptionResponse = serde_json::from_value(json).unwrap();
    assert_eq!(parsed.data, vec![0x11, 0x22, 0x33]);
    assert_eq!(parsed.algorithm, "chacha20");
}

#[test]
fn test_signature_request_construction_with_algorithm() {
    let request = SignatureRequest {
        request_id: uuid::Uuid::new_v4(),
        data: vec![1, 2, 3, 4, 5],
        key_id: Some("sig-key".to_string()),
        algorithm: Some("ed25519".to_string()),
    };
    let params = serde_json::to_value(&request).unwrap();
    assert_eq!(params["algorithm"].as_str(), Some("ed25519"));
}
