// SPDX-License-Identifier: AGPL-3.0-or-later
//! JSON-RPC request handler and method router
//!
//! Routes JSON-RPC 2.0 requests to the appropriate executor or job queue.
//! Semantic method names are resolved through `SemanticMethodRegistry`
//! before dispatch, enabling both legacy `toadstool.*` names and the
//! standard `{domain}.{operation}` naming convention.

mod auth;
mod core;
mod dispatch;
#[cfg(target_os = "linux")]
mod ember;
#[cfg(all(test, target_os = "linux"))]
mod ember_tests;
#[cfg(target_os = "linux")]
mod hw_learn;
mod job;
pub mod method_gate;
#[cfg(target_os = "linux")]
mod mmio;
#[cfg(target_os = "linux")]
mod mmio_ember;
#[cfg(target_os = "linux")]
mod mmio_falcon;
mod resources;
mod router;
mod silicon;
#[cfg(target_os = "linux")]
mod sovereign;
#[cfg(target_os = "linux")]
#[cfg(all(target_os = "linux", feature = "display"))]
mod transport;
mod workload;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use toadstool::semantic_methods::SemanticMethodRegistry;
use toadstool_common::interned_strings::socket_env;
use tracing::{debug, error, info};

pub use method_gate::{
    CallerContext, ConnectionTransport, ConnectionTrustHints, DispatchTrustLevel,
};

use super::types::{JSONRPC_VERSION, JsonRpcError, JsonRpcErrorExt, JsonRpcResponse, ZeroCopyJsonRpcRequest};

use dispatch::DispatchHandler;
#[cfg(target_os = "linux")]
pub use hw_learn::HwLearnHandler;
use job::JobHandler;
use resources::ResourceHandler;
use silicon::SiliconHandler;
#[cfg(all(target_os = "linux", feature = "display"))]
use transport::TransportHandler;
use workload::WorkloadHandler;

#[cfg(target_os = "linux")]
use crate::glowplug_client::{self, SharedGlowPlugClient};

#[cfg(target_os = "linux")]
pub(crate) use crate::background::silicon_discovery::SharedSiliconRegistry;

/// Pure Rust JSON-RPC Handler
///
/// Thin coordinator that delegates to specialized handlers.
/// Routes requests to appropriate methods. Supports both legacy `toadstool.*`
/// names and semantic `{domain}.{operation}` names via the registry.
pub struct JsonRpcHandler {
    version: Arc<str>,
    start_time: std::time::Instant,
    error_count: Arc<AtomicU64>,
    /// PG-62: set to `true` once the server is fully initialized.
    /// `health.readiness` returns `"starting"` until this is set.
    pub(super) ready: Arc<AtomicBool>,
    /// Drain flag: set by `health.drain` to reject new dispatches.
    pub(super) draining: Arc<AtomicBool>,
    /// JH-0 pre-dispatch capability gate. Ships permissive (all calls allowed).
    gate: method_gate::MethodGate,
    semantic_registry: SemanticMethodRegistry,
    pub(super) dispatch: DispatchHandler,
    /// Shared anchor store for warm keepalive — leaked on SIGTERM.
    #[cfg(target_os = "linux")]
    anchor_store: dispatch::AnchorStore,
    #[cfg(target_os = "linux")]
    hw_learn: HwLearnHandler,
    job: JobHandler,
    workload: WorkloadHandler,
    resources: ResourceHandler,
    #[cfg(all(target_os = "linux", feature = "display"))]
    transport: TransportHandler,
    silicon: SiliconHandler,
    #[cfg(target_os = "linux")]
    pub(super) glowplug: SharedGlowPlugClient,
    /// Actual bound JSON-RPC UDS path (set at server startup).
    bound_socket_path: Option<Arc<PathBuf>>,
    /// Cached at init — avoids re-reading env vars on every request.
    pub(super) local_gate_id: Option<Arc<str>>,
    /// Shared silicon registry — populated by background `shader.compile.capabilities` query.
    #[cfg(target_os = "linux")]
    pub(crate) silicon_registry: SharedSiliconRegistry,
}

impl JsonRpcHandler {
    /// Create new handler with executor.
    ///
    /// Pass `error_count` to share the counter with other servers for unified monitoring.
    /// Pass `ready` to share the readiness flag — `health.readiness` returns `"starting"`
    /// until this flag is set to `true` (PG-62).
    pub fn new(
        executor: Arc<crate::tarpc_server::WorkloadExecutorDispatch>,
        version: impl Into<Arc<str>>,
        error_count: Option<Arc<AtomicU64>>,
        ready: Arc<AtomicBool>,
        bound_socket_path: Option<Arc<PathBuf>>,
    ) -> Self {
        let local_gate_id: Arc<str> = std::env::var(socket_env::TOADSTOOL_GATE_ID)
            .or_else(|_| std::env::var(socket_env::HOSTNAME))
            .or_else(|_| toadstool_sysmon::system::hostname().ok_or(std::env::VarError::NotPresent))
            .unwrap_or_else(|_| String::from("local"))
            .into();
        let gate_ownership = Arc::new(crate::cross_gate::GateOwnership::new(&local_gate_id));

        let gate = match std::env::var(socket_env::TOADSTOOL_AUTH_MODE)
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str()
        {
            "enforcing" | "enforced" => {
                info!("MethodGate mode: enforcing (via TOADSTOOL_AUTH_MODE)");
                method_gate::MethodGate::new(method_gate::GateMode::Enforcing)
            }
            _ => method_gate::MethodGate::permissive(),
        };

        let shader_service = crate::visualization_client::create_visualization_client();

        // Spawn ipc.watch background poller — watches the communication provider for shader
        // capability registrations and invalidates the visualization client
        // cache so dispatch can discover the shader compiler at any time (GAP-HS-119).
        #[cfg(target_os = "linux")]
        let silicon_registry;
        #[cfg(target_os = "linux")]
        {
            let watch_client = Arc::clone(&shader_service);
            tokio::spawn(async move {
                crate::background::ipc_watch::run(watch_client).await;
            });

            silicon_registry = crate::background::silicon_discovery::create_silicon_registry();
            let discovery_client = Arc::clone(&shader_service);
            let discovery_registry = Arc::clone(&silicon_registry);
            tokio::spawn(async move {
                crate::background::silicon_discovery::run(discovery_client, discovery_registry)
                    .await;
            });
        }

        let mut dispatch = DispatchHandler::new(shader_service, Self::try_connect_crypto_client());
        #[cfg(target_os = "linux")]
        {
            tracing::info!("Phase D: local cylinder device factory registered");
            dispatch.set_local_device_factory(dispatch::create_cylinder_device_factory());
        }

        // Build resource orchestrator for multi-tenant GPU scheduling.
        // Deployment model from TOADSTOOL_DEPLOYMENT_MODEL env:
        //   "multi" → LocalMulti, "rental" → CloudRental, else LocalDirect (no enforcement)
        let deployment_model =
            match std::env::var(socket_env::TOADSTOOL_DEPLOYMENT_MODEL).as_deref() {
                Ok("multi") => toadstool_runtime_orchestration::DeploymentModel::LocalMulti,
                Ok("rental") => toadstool_runtime_orchestration::DeploymentModel::CloudRental,
                _ => toadstool_runtime_orchestration::DeploymentModel::LocalDirect,
            };

        if deployment_model != toadstool_runtime_orchestration::DeploymentModel::LocalDirect {
            let gpus = toadstool_sysmon::discover_gpus();
            let devices: Vec<toadstool_runtime_orchestration::AvailableDevice> = gpus
                .iter()
                .map(|gpu| toadstool_runtime_orchestration::AvailableDevice {
                    index: gpu.card_index,
                    total_vram_bytes: gpu.telemetry().vram_total_bytes.unwrap_or(0),
                    allocated_vram_bytes: 0,
                    current_tenant: None,
                })
                .collect();
            let orchestrator = toadstool_runtime_orchestration::ResourceOrchestrator::new(
                deployment_model,
                devices,
            );
            dispatch.set_resource_orchestrator(Arc::new(orchestrator));
            tracing::info!(
                model = ?deployment_model,
                gpu_count = gpus.len(),
                "Resource orchestrator initialized for multi-tenant dispatch"
            );
        }

        dispatch.set_gate_ownership(Arc::clone(&gate_ownership));

        #[cfg(target_os = "linux")]
        let anchor_store = dispatch.anchor_store();

        Self {
            version: version.into(),
            start_time: std::time::Instant::now(),
            error_count: error_count.unwrap_or_else(|| Arc::new(AtomicU64::new(0))),
            ready,
            draining: Arc::new(AtomicBool::new(false)),
            gate,
            semantic_registry: SemanticMethodRegistry::new(),
            dispatch,
            #[cfg(target_os = "linux")]
            anchor_store,
            #[cfg(target_os = "linux")]
            hw_learn: HwLearnHandler::new(),
            job: JobHandler::new(Arc::clone(&gate_ownership)),
            workload: WorkloadHandler::new(executor),
            resources: ResourceHandler::new(),
            #[cfg(all(target_os = "linux", feature = "display"))]
            transport: TransportHandler::new(),
            silicon: SiliconHandler::new(),
            #[cfg(target_os = "linux")]
            glowplug: glowplug_client::create_glowplug_client(),
            bound_socket_path,
            local_gate_id: Some(local_gate_id),
            #[cfg(target_os = "linux")]
            silicon_registry,
        }
    }

    /// Get the anchor store for wiring into the SIGTERM leak handler.
    #[cfg(target_os = "linux")]
    pub fn anchor_store(&self) -> dispatch::AnchorStore {
        self.anchor_store.clone()
    }

    /// Attempt to connect to the crypto provider for crypto delegation.
    /// Returns `None` in standalone mode (no crypto capability socket).
    fn try_connect_crypto_client()
    -> Option<Arc<toadstool_distributed::crypto_integration::CryptoServiceClient>> {
        let socket = toadstool_common::primal_sockets::get_socket_path_for_capability("crypto");
        if socket.exists() {
            match toadstool_distributed::crypto_integration::CryptoServiceClient::from_local_socket(
                &socket,
            ) {
                Ok(client) => {
                    info!("Tower crypto client connected — compute payloads will be encrypted");
                    Some(Arc::new(client))
                }
                Err(e) => {
                    debug!(
                        "Tower crypto client unavailable: {e} — standalone mode (plaintext dispatch)"
                    );
                    None
                }
            }
        } else {
            debug!(
                path = %socket.display(),
                "crypto socket absent — standalone mode (plaintext dispatch)"
            );
            None
        }
    }

    /// Handle a JSON-RPC request (main entry point).
    ///
    /// Pattern: parse → validate → resolve → route → execute → respond
    pub async fn handle_request(&self, request: &ZeroCopyJsonRpcRequest<'_>) -> JsonRpcResponse {
        self.handle_request_with_connection(request, ConnectionTrustHints::default())
            .await
    }

    /// Handle a JSON-RPC request with per-connection trust hints.
    pub async fn handle_request_with_connection(
        &self,
        request: &ZeroCopyJsonRpcRequest<'_>,
        conn: ConnectionTrustHints,
    ) -> JsonRpcResponse {
        let id = request.id.clone().unwrap_or(serde_json::Value::Null);

        if request.jsonrpc != JSONRPC_VERSION {
            self.error_count.fetch_add(1, Ordering::Relaxed);
            return JsonRpcResponse {
                jsonrpc: "2.0".into(),
                result: None,
                error: Some(JsonRpcError::invalid_request(
                    "Invalid JSON-RPC version (must be '2.0')",
                )),
                id,
            };
        }

        info!(method = %request.method.as_ref(), "JSON-RPC request");

        match self
            .handle_method(request.method.as_ref(), request.params.as_ref(), conn)
            .await
        {
            Ok(result) => JsonRpcResponse {
                jsonrpc: "2.0".into(),
                result: Some(result),
                error: None,
                id,
            },
            Err(err) => {
                self.error_count.fetch_add(1, Ordering::Relaxed);
                error!(
                    method = %request.method.as_ref(),
                    error = %err.message,
                    "JSON-RPC error",
                );
                JsonRpcResponse {
                    jsonrpc: "2.0".into(),
                    result: None,
                    error: Some(err),
                    id,
                }
            }
        }
    }
}

/// Extract caller provenance from connection-level trust hints.
///
/// Defaults to anonymous. Unix connections without BTSP get
/// [`DispatchTrustLevel::LocalTransport`]. Completed BTSP handshakes set
/// [`DispatchTrustLevel::BtspVerified`] (crypto provider JH-1 will add mutual auth).
///
/// `cached_gate_id` is resolved once at handler init — avoids re-reading
/// env vars on every request.
pub(super) fn extract_caller_context(
    conn: ConnectionTrustHints,
    cached_gate_id: Option<&Arc<str>>,
) -> CallerContext {
    let mut ctx = CallerContext::anonymous();
    let gate_id = cached_gate_id.map(std::string::ToString::to_string);
    if conn.mutually_authenticated {
        ctx.trust_level = DispatchTrustLevel::MutuallyAuthenticated;
        ctx.gate_id = gate_id;
    } else if conn.btsp_verified {
        ctx.trust_level = DispatchTrustLevel::BtspVerified;
        ctx.gate_id = gate_id;
    } else if conn.transport == ConnectionTransport::Unix {
        ctx.trust_level = DispatchTrustLevel::LocalTransport;
        ctx.gate_id = gate_id;
    }
    ctx
}

pub(crate) fn resolve_local_gate_id() -> Option<String> {
    std::env::var(socket_env::TOADSTOOL_GATE_ID)
        .or_else(|_| std::env::var(socket_env::HOSTNAME))
        .or_else(|_| toadstool_sysmon::system::hostname().ok_or(std::env::VarError::NotPresent))
        .ok()
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
