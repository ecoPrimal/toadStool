// SPDX-License-Identifier: AGPL-3.0-or-later
//! Crypto integration types - Protocol-level, vendor-agnostic
//!
//! **Design Philosophy**:
//! - Protocol-agnostic: Works with any crypto provider's API
//! - Version-agnostic: Handle API evolution gracefully
//! - Type-safe: Strong types, no stringly-typed data
//! - Zero vendor lock-in: These types work with ANY crypto service

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Crypto request (encryption, decryption, signing, etc.)
///
/// **Design**: Vendor-agnostic protocol types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CryptoRequest {
    /// Request identifier
    pub request_id: Uuid,

    /// Operation type
    pub operation: CryptoOperation,

    /// Data to process
    pub data: Vec<u8>,

    /// Key identifier (if using existing key)
    pub key_id: Option<String>,

    /// Algorithm preference
    pub algorithm: Option<EncryptionAlgorithm>,

    /// Security level required
    pub security_level: SecurityLevel,

    /// Additional metadata
    pub metadata: serde_json::Value,
}

/// Crypto response from service
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CryptoResponse {
    /// Request identifier
    pub request_id: Uuid,

    /// Result data
    pub data: Vec<u8>,

    /// Key used
    pub key_id: String,

    /// Algorithm used
    pub algorithm: String,

    /// Metadata (nonce, tag, IV, etc.)
    pub metadata: serde_json::Value,
}

/// Crypto operation type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CryptoOperation {
    /// Encrypt data
    Encrypt,

    /// Decrypt data
    Decrypt,

    /// Sign data
    Sign,

    /// Verify signature
    Verify,

    /// Hash data
    Hash,

    /// Generate key
    GenerateKey {
        /// Desired key material type and size.
        key_type: KeyType,
    },

    /// Rotate key
    RotateKey {
        /// Key id to retire and replace.
        old_key_id: String,
    },

    /// Export key (for backup/migration)
    ExportKey {
        /// Key id to export.
        key_id: String,
    },

    /// Import key
    ImportKey {
        /// Encrypted or raw key blob from backup.
        key_data: Vec<u8>,
    },
}

/// Encryption algorithm (extensible list)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum EncryptionAlgorithm {
    /// AES-256-GCM (recommended)
    Aes256Gcm,

    /// AES-128-GCM
    Aes128Gcm,

    /// ChaCha20-Poly1305
    ChaCha20Poly1305,

    /// RSA with OAEP
    RsaOaep {
        /// RSA modulus size in bits.
        bits: u16,
    },

    /// Elliptic curve (ECDSA, ECDH)
    EllipticCurve {
        /// Named curve (e.g. P-256).
        curve: String,
    },

    /// Custom/provider-specific
    Custom(String),
}

/// Key type for generation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum KeyType {
    /// Symmetric key (AES, ChaCha)
    Symmetric {
        /// Key length in bits.
        bits: u16,
    },

    /// Asymmetric keypair (RSA, EC)
    Asymmetric {
        /// Algorithm name or family.
        algorithm: String,
        /// Modulus or key size in bits.
        bits: u16,
    },

    /// Signing key
    Signing {
        /// Signing algorithm name.
        algorithm: String,
    },

    /// Key encryption key
    Kek,
}

/// Map a wire-format algorithm name (purpose keys, legacy security clients) to [`EncryptionAlgorithm`].
pub fn encryption_algorithm_from_wire(name: &str) -> EncryptionAlgorithm {
    match name.to_lowercase().as_str() {
        "aes-256-gcm" | "aes256gcm" => EncryptionAlgorithm::Aes256Gcm,
        "aes-128-gcm" | "aes128gcm" => EncryptionAlgorithm::Aes128Gcm,
        "chacha20-poly1305" | "chacha20poly1305" | "chacha20" => {
            EncryptionAlgorithm::ChaCha20Poly1305
        }
        other => EncryptionAlgorithm::Custom(other.to_string()),
    }
}

// Canonical SecurityLevel from cellmembrane-types.
// Local enum removed — was (Standard, High, Maximum, QuantumResistant).
// Exact match with canonical variants.
pub use cellmembrane_types::SecurityLevel;

/// Key management request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyManagementRequest {
    /// Request identifier
    pub request_id: Uuid,

    /// Key operation
    pub operation: KeyOperation,

    /// Key metadata
    pub metadata: serde_json::Value,
}

/// Key management response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyManagementResponse {
    /// Request identifier
    pub request_id: Uuid,

    /// Key identifier
    pub key_id: String,

    /// Operation result
    pub success: bool,

    /// Metadata
    pub metadata: serde_json::Value,
}

/// Key operation type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum KeyOperation {
    /// Generate new key
    Generate {
        /// Type of key to create.
        key_type: KeyType,
    },

    /// Rotate existing key
    Rotate {
        /// Key id to rotate.
        key_id: String,
    },

    /// Delete key
    Delete {
        /// Key id to remove.
        key_id: String,
    },

    /// Export key for backup
    Export {
        /// Key id to export.
        key_id: String,
    },

    /// Import key from backup
    Import {
        /// Serialized key material.
        key_data: Vec<u8>,
    },

    /// List keys
    List,
}

/// Signature request to a crypto service
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatureRequest {
    /// Request identifier
    pub request_id: Uuid,

    /// Data to sign
    pub data: Vec<u8>,

    /// Key identifier (if using existing key)
    pub key_id: Option<String>,

    /// Algorithm preference
    pub algorithm: Option<String>,
}

/// Signature response from a crypto service
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatureResponse {
    /// Request identifier
    pub request_id: Uuid,

    /// Signature data
    pub signature: Vec<u8>,

    /// Key used
    pub key_id: String,

    /// Algorithm used
    pub algorithm: String,
}

/// Verification request to a crypto service
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationRequest {
    /// Request identifier
    pub request_id: Uuid,

    /// Original data
    pub data: Vec<u8>,

    /// Signature to verify
    pub signature: Vec<u8>,

    /// Public key identifier
    pub public_key_id: String,
}

/// Verification response from a crypto service
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResponse {
    /// Request identifier
    pub request_id: Uuid,

    /// Whether signature is valid
    pub valid: bool,

    /// Verification details (optional)
    pub details: Option<String>,
}

/// Permission response from a crypto service
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionResponse {
    /// Request identifier
    pub request_id: Uuid,

    /// Permission identifier
    pub permission_id: Uuid,

    /// Cryptographic proof
    pub proof: Vec<u8>,

    /// Metadata
    pub metadata: serde_json::Value,
}

/// Validation response from a crypto service
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResponse {
    /// Request identifier
    pub request_id: Uuid,

    /// Whether permission is valid
    pub valid: bool,

    /// Validation details (optional)
    pub details: Option<String>,
}

/// Revocation request to a crypto service
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevocationRequest {
    /// Reason for revocation
    pub reason: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crypto_request_creation() {
        let request = CryptoRequest {
            request_id: Uuid::new_v4(),
            operation: CryptoOperation::Encrypt,
            data: vec![1, 2, 3],
            key_id: None,
            algorithm: Some(EncryptionAlgorithm::Aes256Gcm),
            security_level: SecurityLevel::High,
            metadata: serde_json::Value::Null,
        };

        assert_eq!(request.operation, CryptoOperation::Encrypt);
        assert_eq!(request.security_level, SecurityLevel::High);
    }

    #[test]
    fn test_security_level_ordering() {
        assert!(SecurityLevel::Standard < SecurityLevel::High);
        assert!(SecurityLevel::High < SecurityLevel::Maximum);
        assert!(SecurityLevel::Maximum < SecurityLevel::QuantumResistant);
    }

    #[test]
    fn test_encryption_algorithm_custom() {
        let algo = EncryptionAlgorithm::Custom("XSalsa20".to_string());
        assert!(matches!(algo, EncryptionAlgorithm::Custom(_)));
    }
}
