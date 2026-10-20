// SPDX-License-Identifier: AGPL-3.0-or-later
//! Hardware learning JSON-RPC handlers.
//!
//! Exposes the hw-learn pipeline (observe → distill → apply → share) as
//! JSON-RPC methods under the `compute.hardware.*` domain, matching the
//! biomeOS v2.30 capability registration.

mod apply;
mod auto_init;
mod helpers;
mod observe_distill;
mod share_recipe;
mod status;
mod telemetry;
mod vfio;

use crate::pure_jsonrpc::types::{JsonRpcError, JsonRpcErrorExt};
use std::path::PathBuf;

pub use helpers::check_thermal_for_bdf_pub;

/// Handler for `compute.hardware.*` JSON-RPC methods.
///
/// Wraps the hw-learn pipeline and nvpmu firmware inventory.
/// All methods are stateless — the `KnowledgeStore` persists recipes to disk.
pub struct HwLearnHandler {
    store_dir: PathBuf,
}

impl Default for HwLearnHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl HwLearnHandler {
    /// Creates a new hw-learn handler with default store directory.
    #[must_use]
    pub fn new() -> Self {
        let store_dir = helpers::dirs_for_store();
        Self { store_dir }
    }

    fn open_store(&self) -> Result<hw_learn::knowledge::KnowledgeStore, JsonRpcError> {
        hw_learn::knowledge::KnowledgeStore::open(&self.store_dir)
            .map_err(|e| JsonRpcError::internal_error(format!("Failed to open recipe store: {e}")))
    }

    /// Expose store_dir for auto_init_all's spawn_blocking closures.
    fn store_dir(&self) -> PathBuf {
        self.store_dir.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn vfio_devices_returns_valid_json() {
        let handler = HwLearnHandler::new();
        let result = handler.hw_learn_vfio_devices(None).await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert!(value.get("devices").is_some());
        assert!(value.get("count").is_some());
    }
}
