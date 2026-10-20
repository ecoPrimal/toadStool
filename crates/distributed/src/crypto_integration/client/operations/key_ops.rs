// SPDX-License-Identifier: AGPL-3.0-or-later
//! Key management JSON-RPC operations for [`CryptoServiceClient`](super::CryptoServiceClient).

#[cfg(unix)]
use base64::Engine;
#[cfg(unix)]
use toadstool_common::{NetworkError, ToadStoolError, ToadStoolResult};
#[cfg(not(unix))]
use toadstool_common::{ToadStoolError, ToadStoolResult};

use crate::crypto_integration::types::{KeyManagementRequest, KeyManagementResponse};

use super::CryptoServiceClient;

impl CryptoServiceClient {
    /// Manage keys (generate, rotate, delete) via unix socket
    ///
    /// **Pure Rust**: JSON-RPC over unix socket (no HTTP, no ring!)
    pub async fn manage_key(
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
                ToadStoolError::Network(NetworkError::IoError {
                    reason: format!("Failed to serialize request: {e}"),
                })
            })?;

            tokio::time::timeout(
                self.timeout,
                self.rpc_client.call_typed("crypto.manage_key", params),
            )
            .await
            .map_err(|_| {
                ToadStoolError::Network(NetworkError::IoError {
                    reason: format!("Key management timed out after {:?}", self.timeout),
                })
            })?
            .map_err(|e| {
                ToadStoolError::Network(NetworkError::IoError {
                    reason: format!("Key management failed: {e}"),
                })
            })
        }
    }

    /// Retrieve a purpose key from the crypto provider secrets store.
    ///
    /// Key name: `"nucleus:{family}:purpose:{purpose}"`. When `family` is `None`,
    /// reads `TOADSTOOL_FAMILY_ID` (or related env vars).
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
                    ToadStoolError::Network(NetworkError::IoError {
                        reason: format!("secrets.retrieve(\"{key_name}\") failed: {e}"),
                    })
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
}
