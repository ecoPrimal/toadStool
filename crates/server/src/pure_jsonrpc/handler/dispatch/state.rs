// SPDX-License-Identifier: AGPL-3.0-or-later
//! Dispatch handler construction, configuration, and anchor-store lifecycle.

use super::DispatchHandler;
use crate::visualization_client::SharedVisualizationClient;
use std::collections::HashMap;
#[cfg(target_os = "linux")]
use std::os::fd::AsFd;
use std::sync::Arc;

const DEFAULT_ESTIMATED_DURATION: std::time::Duration = std::time::Duration::from_secs(60);

impl DispatchHandler {
    pub fn new(
        shader_service: SharedVisualizationClient,
        crypto_client: Option<Arc<toadstool_distributed::crypto_integration::CryptoServiceClient>>,
    ) -> Self {
        Self {
            shader_service,
            crypto_client,
            cached_purpose_key: Arc::new(std::sync::RwLock::new(None)),
            jobs: Arc::new(std::sync::RwLock::new(HashMap::new())),
            pipelines: Arc::new(std::sync::RwLock::new(HashMap::new())),
            dispatch_count: std::sync::atomic::AtomicU64::new(0),
            #[cfg(target_os = "linux")]
            device_pool: Arc::new(std::sync::RwLock::new(HashMap::new())),
            #[cfg(target_os = "linux")]
            local_device_factory: None,
            #[cfg(target_os = "linux")]
            cached_devices: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            #[cfg(target_os = "linux")]
            anchor_store: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            resource_orchestrator: None,
            gate_ownership: None,
        }
    }

    pub fn set_gate_ownership(&mut self, ownership: Arc<crate::cross_gate::GateOwnership>) {
        self.gate_ownership = Some(ownership);
    }

    /// Get a clone of the anchor store for use in the SIGTERM handler.
    #[cfg(target_os = "linux")]
    pub fn anchor_store(&self) -> super::AnchorStore {
        Arc::clone(&self.anchor_store)
    }

    /// Set the local device factory for Phase D sovereign dispatch.
    #[cfg(target_os = "linux")]
    pub fn set_local_device_factory(&mut self, factory: super::LocalDeviceFactory) {
        self.local_device_factory = Some(factory);
    }

    pub fn set_resource_orchestrator(
        &mut self,
        orchestrator: Arc<toadstool_runtime_orchestration::ResourceOrchestrator>,
    ) {
        self.resource_orchestrator = Some(orchestrator);
    }

    /// Pre-dispatch resource check via the orchestrator (no-op when unset).
    pub(super) async fn pre_dispatch_resource_check(
        &self,
        bdf: &str,
        ctx: Option<&super::super::method_gate::CallerContext>,
        params: Option<&serde_json::Value>,
    ) -> Result<
        Option<toadstool_runtime_orchestration::ResourceAllocation>,
        crate::pure_jsonrpc::types::JsonRpcError,
    > {
        use crate::pure_jsonrpc::types::{JsonRpcError, JsonRpcErrorExt};
        use toadstool_common::constants::jsonrpc::error_codes;

        let Some(orchestrator) = self.resource_orchestrator.as_ref() else {
            return Ok(None);
        };

        let preferred_devices = toadstool_sysmon::discover_gpus()
            .into_iter()
            .find(|gpu| gpu.pci_slot == bdf)
            .map(|gpu| vec![gpu.card_index])
            .unwrap_or_default();

        let caller_gate_id = resolve_caller_gate_id(ctx, params);
        let hardware_owner_gate_id = if let Some(ownership) = self.gate_ownership.as_ref() {
            Some(
                ownership
                    .hardware_owner_gate_id()
                    .await
                    .as_ref()
                    .to_string(),
            )
        } else {
            None
        };

        let request = toadstool_runtime_orchestration::ResourceRequest {
            tenant_id: String::from("anonymous"),
            priority: 3,
            preferred_devices,
            min_vram_bytes: 0,
            estimated_duration: DEFAULT_ESTIMATED_DURATION,
            caller_gate_id,
            hardware_owner_gate_id,
        };

        match orchestrator.allocate(&request) {
            Ok(allocation) => Ok(Some(allocation)),
            Err(toadstool_runtime_orchestration::OrchestrationError::GuestLoadExceeded(msg)) => {
                Err(JsonRpcError::server_error(
                    error_codes::CAPABILITY_NOT_AVAILABLE,
                    msg,
                ))
            }
            Err(toadstool_runtime_orchestration::OrchestrationError::QuotaExceeded(msg)) => Err(
                JsonRpcError::server_error(error_codes::RESOURCE_EXHAUSTED, msg),
            ),
            Err(err) => Err(JsonRpcError::internal_error(err.to_string())),
        }
    }
}

/// Resolve caller gate id from BTSP/unix transport or mesh `_dispatch_trust`.
fn resolve_caller_gate_id(
    ctx: Option<&super::super::method_gate::CallerContext>,
    params: Option<&serde_json::Value>,
) -> Option<String> {
    if let Some(id) = ctx.and_then(|c| c.gate_id.clone()) {
        return Some(id);
    }
    params.and_then(|p| {
        p.get("_dispatch_trust")
            .and_then(|t| t.get("source_gate_id"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
    })
}

#[cfg(target_os = "linux")]
impl DispatchHandler {
    /// Dup VFIO fds from the anchor store into `ReceivedVfioFds` for
    /// device adoption. Returns `None` if no anchor exists for this BDF.
    pub(super) async fn dup_received_fds_from_anchor(
        &self,
        bdf: &str,
    ) -> Option<toadstool_cylinder::vfio::ReceivedVfioFds> {
        let anchors = self.anchor_store.lock().await;
        let anchor = anchors.get(bdf)?;

        let device_fd = anchor.device_fd().try_clone_to_owned().ok()?;

        match anchor.backend_arc() {
            toadstool_ember::vfio_anchor::AnchorBackendRef::Iommufd { iommufd, ioas_id } => {
                let iommufd_dup = iommufd.as_fd().try_clone_to_owned().ok()?;
                Some(toadstool_cylinder::vfio::ReceivedVfioFds::Iommufd {
                    iommufd: iommufd_dup,
                    device: device_fd,
                    ioas_id,
                })
            }
            toadstool_ember::vfio_anchor::AnchorBackendRef::LegacyGroup { container } => {
                let container_dup = container.as_fd().try_clone_to_owned().ok()?;
                let group_fd = anchor.group_fd()?.try_clone_to_owned().ok()?;
                Some(toadstool_cylinder::vfio::ReceivedVfioFds::Legacy {
                    container: container_dup,
                    device: device_fd,
                    group: group_fd,
                })
            }
        }
    }

    /// Try to engage the clutch for a BDF from the anchor store.
    ///
    /// Uses `WarmKeepalive` to streamline fd extraction and DMA construction.
    pub(super) async fn try_engage_clutch(
        &self,
        bdf: &str,
    ) -> Option<toadstool_cylinder::vfio::clutch::ClutchEngaged> {
        let anchors = self.anchor_store.lock().await;
        let anchor = anchors.get(bdf)?;
        let view = toadstool_ember::WarmKeepalive::from_ref(anchor);
        let dma = dma_from_keepalive(&view)?;

        match toadstool_cylinder::vfio::clutch::Clutch::engage(bdf, view.device_fd(), dma.clone()) {
            Ok(engaged) => {
                tracing::info!(bdf, "clutch engaged from keepalive (VFIO BAR0)");
                Some(engaged)
            }
            Err(e) => {
                tracing::warn!(bdf, err = %e, "clutch VFIO engage failed — trying sysfs BAR0");
                toadstool_cylinder::vfio::clutch::Clutch::engage_sysfs(bdf, dma).ok()
            }
        }
    }
}

/// Convert a `WarmKeepaliveRef`'s DMA spec into cylinder's `DmaBackend`.
#[cfg(target_os = "linux")]
fn dma_from_keepalive(
    view: &toadstool_ember::warm_keepalive::WarmKeepaliveRef<'_>,
) -> Option<toadstool_cylinder::vfio::DmaBackend> {
    let spec = view.make_dma_backend();
    if let Some((iommufd, ioas_id)) = spec.as_iommufd() {
        Some(toadstool_cylinder::vfio::clutch::Clutch::dma_backend_from_iommufd(iommufd, ioas_id))
    } else if let Some(container) = spec.as_legacy_container() {
        Some(toadstool_cylinder::vfio::clutch::Clutch::dma_backend_from_legacy(container))
    } else {
        tracing::error!("DmaSpec is neither iommufd nor legacy — cannot create DMA backend");
        None
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::DispatchHandler;
    use crate::cross_gate::GateOwnership;
    use crate::pure_jsonrpc::handler::method_gate::CallerContext;
    use crate::visualization_client::VisualizationClient;

    fn test_handler() -> DispatchHandler {
        DispatchHandler::new(Arc::new(VisualizationClient::unavailable()), None)
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn new_handler_anchor_store_is_empty() {
        let handler = test_handler();
        let store = handler.anchor_store();
        assert!(store.try_lock().is_ok());
    }

    #[test]
    fn set_gate_ownership_accepts_ownership_handle() {
        let mut handler = test_handler();
        handler.set_gate_ownership(Arc::new(GateOwnership::new("owner-gate")));
    }

    #[test]
    fn resolve_caller_gate_id_prefers_caller_context() {
        let ctx = CallerContext {
            gate_id: Some(String::from("ctx-gate")),
            ..CallerContext::anonymous()
        };
        let params = serde_json::json!({
            "_dispatch_trust": { "source_gate_id": "params-gate" }
        });
        assert_eq!(
            super::resolve_caller_gate_id(Some(&ctx), Some(&params)).as_deref(),
            Some("ctx-gate")
        );
    }

    #[test]
    fn resolve_caller_gate_id_falls_back_to_dispatch_trust_params() {
        let params = serde_json::json!({
            "_dispatch_trust": { "source_gate_id": "params-gate" }
        });
        assert_eq!(
            super::resolve_caller_gate_id(None, Some(&params)).as_deref(),
            Some("params-gate")
        );
    }

    #[test]
    fn resolve_caller_gate_id_returns_none_without_provenance() {
        assert!(super::resolve_caller_gate_id(None, None).is_none());
    }

    #[tokio::test]
    async fn pre_dispatch_resource_check_without_orchestrator_is_noop() {
        let handler = test_handler();
        let result = handler
            .pre_dispatch_resource_check("0000:03:00.0", None, None)
            .await
            .expect("noop");
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn dispatch_submit_creates_lookupable_job() {
        let handler = test_handler();
        let params = serde_json::json!({
            "binary": [9u8, 8, 7],
            "bdf": "0000:04:00.0",
            "dispatch_mode": "passthrough",
        });
        let submit = handler
            .dispatch_submit(Some(&params))
            .await
            .expect("submit");
        let job_id = submit["job_id"].as_str().expect("job_id");

        let status = handler
            .dispatch_status(Some(&serde_json::json!({ "job_id": job_id })))
            .await
            .expect("status");
        assert_eq!(status["job_id"], job_id);

        let result = handler
            .dispatch_result(Some(&serde_json::json!({ "job_id": job_id })))
            .await
            .expect("result");
        assert_eq!(result["job_id"], job_id);
    }
}
