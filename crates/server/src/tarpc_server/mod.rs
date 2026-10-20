// SPDX-License-Identifier: AGPL-3.0-or-later
//! # ToadStool tarpc Server Implementation
//!
//! High-performance binary RPC server for primal-to-primal communication.
//! Follows the coordination service's architecture pattern.

mod connection;
mod dispatch;
mod executor;

#[cfg(unix)]
pub(crate) use connection::serve_on_tarpc_channel;

pub use dispatch::WorkloadExecutorDispatch;
#[cfg(test)]
pub use executor::TestWorkloadDouble;
pub use executor::{StandaloneExecutor, WorkloadExecutor};

#[cfg(test)]
pub use executor::TestExecutor;

#[cfg(unix)]
use std::io::ErrorKind;
use std::sync::Arc;
use std::sync::RwLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;
use tarpc::context::Context;
use tracing::info;
#[cfg(unix)]
use tracing::warn;

use crate::errors::{ServerError, ServerResult};

#[cfg(unix)]
use toadstool_common::constants::platform_paths::procfs;

// Deep debt solution: Use pure RPC types from local module
#[cfg(test)]
use crate::rpc_types::{AvailableResources, ComputeUnit, ExecutionMetrics};
use crate::rpc_types::{
    ComputeCapabilities, HealthStatus, ServiceError, ToadStoolComputeRpc, WorkloadResult,
    WorkloadStatus, WorkloadSubmission,
};

/// tarpc server state
pub struct ToadStoolTarpcServer {
    /// Service start time
    start_time: Instant,
    /// Service version (`Arc<str>` avoids allocation on spawn-per-connection clone)
    version: Arc<str>,
    /// Active workloads (`Arc<str>` key = zero-copy clone for entry/insert)
    workloads: Arc<RwLock<std::collections::HashMap<Arc<str>, WorkloadResult>>>,
    /// Workload executor (real implementation, not mock)
    executor: Arc<WorkloadExecutorDispatch>,
    /// Error count for monitoring
    error_count: Arc<AtomicU64>,
}

impl ToadStoolTarpcServer {
    /// Calculate current resource utilization
    ///
    /// **Deep Debt Compliance**:
    /// - Queries real system state (no hardcoding)
    /// - Returns 0.0-1.0 utilization percentage
    /// - Graceful degradation on query failure
    async fn calculate_resource_utilization(&self) -> f32 {
        let active_count = self
            .workloads
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len();
        let max_capacity =
            std::thread::available_parallelism().map_or(4, std::num::NonZero::get) * 4; // ~4 workloads per core

        #[expect(
            clippy::cast_precision_loss,
            reason = "precision loss acceptable for this conversion"
        )]
        let base_utilization = if max_capacity > 0 {
            (active_count as f32) / (max_capacity as f32)
        } else {
            0.0
        };

        // Factor in system load (Deep Debt: runtime query)
        let system_load = Self::query_system_load();
        let load_factor = system_load.unwrap_or(0.5);

        // Combined utilization (capped at 1.0)
        (base_utilization * 0.7 + load_factor * 0.3).min(1.0)
    }

    /// Query system load average (runtime discovery)
    fn query_system_load() -> Option<f32> {
        #[cfg(unix)]
        {
            if let Ok(loadavg) = std::fs::read_to_string(procfs::LOADAVG)
                && let Some(first) = loadavg.split_whitespace().next()
                && let Ok(load) = first.parse::<f32>()
            {
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "precision loss acceptable for this conversion"
                )]
                let cpu_count =
                    std::thread::available_parallelism().map_or(4.0, |n| n.get() as f32);
                return Some((load / cpu_count).min(1.0));
            }
        }
        None
    }
}

impl ToadStoolTarpcServer {
    /// Create new tarpc server with real executor
    ///
    /// Pass `error_count` to share the counter with JSON-RPC server for unified monitoring.
    pub fn new(
        version: impl AsRef<str>,
        executor: Arc<WorkloadExecutorDispatch>,
        error_count: Option<Arc<AtomicU64>>,
    ) -> Self {
        Self {
            start_time: Instant::now(),
            version: Arc::from(version.as_ref()),
            workloads: Arc::new(RwLock::new(std::collections::HashMap::new())),
            executor,
            error_count: error_count.unwrap_or_else(|| Arc::new(AtomicU64::new(0))),
        }
    }

    /// Start tarpc server on Unix socket (OPTIONAL transport - for binary RPC when needed)
    /// Per wateringHole standard: JSON-RPC 2.0 is PRIMARY, tarpc is OPTIONAL
    ///
    /// Deep debt principle: No TCP hardcoding, use Unix sockets for multi-instance support
    ///
    /// # Errors
    ///
    /// Returns [`ServerError`] if directory creation, socket bind, permission setting, or accept fails.
    #[cfg(unix)]
    pub async fn serve_unix(self, socket_path: impl AsRef<std::path::Path>) -> ServerResult<()> {
        use tokio::net::UnixListener;

        let socket_path = socket_path.as_ref();

        // Ensure parent directory exists (biomeOS requirement for custom socket paths)
        if let Some(parent) = socket_path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                warn!(
                    "Failed to create socket directory {parent:?}: {e}; continuing without creating parent"
                );
            } else {
                info!("Ensured socket directory exists: {:?}", parent);
            }
        }

        // Clean up old socket if present
        match std::fs::remove_file(socket_path) {
            Ok(()) => info!("Removed old socket file: {:?}", socket_path),
            Err(e) if e.kind() == ErrorKind::NotFound => {}
            Err(e) => return Err(ServerError::Network(e.to_string())),
        }

        info!("tarpc server binding to Unix socket: {:?}", socket_path);
        let listener =
            UnixListener::bind(socket_path).map_err(|e| ServerError::Network(e.to_string()))?;

        #[cfg(unix)]
        {
            let mode = std::env::var(
                toadstool_common::interned_strings::socket_env::TOADSTOOL_SOCKET_MODE,
            )
            .ok()
            .and_then(|s| {
                u32::from_str_radix(s.trim_start_matches("0o").trim_start_matches('0'), 8).ok()
            })
            .unwrap_or(0o660);
            let access = toadstool_common::platform::PlatformAccess::Custom(mode);
            toadstool_common::platform::set_access(socket_path, access)
                .map_err(|e| ServerError::Internal(e.to_string()))?;
            info!("Set tarpc socket permissions to {mode:04o}");
        }

        info!(
            "✅ tarpc server listening on Unix socket: {:?}",
            socket_path
        );

        let env = toadstool_common::primal_sockets::SocketPathEnv::from_env();
        let btsp_required = toadstool_common::primal_sockets::is_btsp_required(&env);
        if btsp_required {
            info!("🔒 tarpc Unix: BTSP handshake required (FAMILY_ID set)");
        }

        loop {
            let (stream, _addr) = listener
                .accept()
                .await
                .map_err(|e| ServerError::Network(e.to_string()))?;
            let server = self.clone();

            tokio::spawn(async move {
                let Ok(stream) =
                    connection::unix_maybe_btsp_before_tarpc(stream, btsp_required).await
                else {
                    return;
                };

                connection::serve_on_tarpc_channel(server, stream).await;
            });
        }
    }

    /// Start tarpc server on TCP listener (isomorphic fallback)
    ///
    /// **ISOMORPHIC MODE**: Automatic fallback for platforms without Unix sockets.
    ///
    /// This method is used only when Unix sockets fail due to platform constraints
    /// (SELinux, Android, etc.). The listener is pre-bound to 127.0.0.1:0 for security.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError`] if getting local address or accept fails.
    pub async fn serve_tcp(self, listener: tokio::net::TcpListener) -> ServerResult<()> {
        let local_addr = listener
            .local_addr()
            .map_err(|e| ServerError::Network(e.to_string()))?;
        info!("✅ tarpc server listening on TCP: {}", local_addr);

        let idle_secs = std::env::var(
            toadstool_common::interned_strings::socket_env::TOADSTOOL_TCP_IDLE_TIMEOUT_SECS,
        )
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(toadstool_config::defaults::network::TCP_IDLE_TIMEOUT_SECS);
        let idle_timeout = std::time::Duration::from_secs(idle_secs);

        loop {
            let (stream, addr) = listener
                .accept()
                .await
                .map_err(|e| ServerError::Network(e.to_string()))?;
            let _ = stream.set_nodelay(true);
            let server = self.clone();

            tokio::spawn(async move {
                connection::serve_on_tarpc_channel_with_idle_timeout(server, stream, idle_timeout)
                    .await;
                tracing::debug!("tarpc TCP connection from {addr} closed");
            });
        }
    }
}

// Clone implementation for spawning tasks
impl Clone for ToadStoolTarpcServer {
    fn clone(&self) -> Self {
        Self {
            start_time: self.start_time,
            version: Arc::clone(&self.version),
            workloads: Arc::clone(&self.workloads),
            executor: Arc::clone(&self.executor),
            error_count: Arc::clone(&self.error_count),
        }
    }
}

impl ToadStoolComputeRpc for ToadStoolTarpcServer {
    async fn submit_workload(
        self,
        _context: Context,
        submission: WorkloadSubmission,
    ) -> Result<WorkloadResult, ServiceError> {
        info!("Submitting workload: {}", submission.workload_id.as_ref());

        // Execute via real executor (not mock)
        let result = self.executor.execute(submission).await.inspect_err(|_| {
            self.error_count.fetch_add(1, Ordering::Relaxed);
        })?;

        // Store result
        // ✅ OPTIMIZED: Use Entry API; Arc::clone(workload_id) and result.clone() are cheap
        // (Bytes clone = refcount bump, Arc<str> clone = refcount bump)
        {
            let mut workloads = self
                .workloads
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            workloads
                .entry(Arc::clone(&result.workload_id))
                .or_insert_with(|| result.clone());
        }

        Ok(result)
    }

    async fn query_status(
        self,
        _context: Context,
        workload_id: String,
    ) -> Result<WorkloadResult, ServiceError> {
        let workloads = self
            .workloads
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        workloads.get(workload_id.as_str()).cloned().ok_or_else(|| {
            self.error_count.fetch_add(1, Ordering::Relaxed);
            ServiceError::WorkloadNotFound { workload_id }
        })
    }

    async fn cancel_workload(
        self,
        _context: Context,
        workload_id: String,
    ) -> Result<(), ServiceError> {
        self.executor.cancel(&workload_id).await.inspect_err(|_| {
            self.error_count.fetch_add(1, Ordering::Relaxed);
        })?;

        // Update status
        let mut workloads = self
            .workloads
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(result) = workloads.get_mut(workload_id.as_str()) {
            result.status = WorkloadStatus::Cancelled;
        }

        Ok(())
    }

    async fn list_workloads(
        self,
        _context: Context,
        _filter: Option<std::collections::HashMap<String, String>>,
    ) -> Result<Vec<WorkloadResult>, ServiceError> {
        let workloads = self
            .workloads
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        Ok(workloads.values().cloned().collect())
    }

    /// Query capabilities - SELF-KNOWLEDGE ONLY
    ///
    /// Following the principle: "Primal code only has self knowledge
    /// and discovers other primals at runtime"
    async fn query_capabilities(
        self,
        _context: Context,
    ) -> Result<ComputeCapabilities, ServiceError> {
        // Query OUR capabilities only (not other primals)
        self.executor.query_capabilities().await.inspect_err(|_| {
            self.error_count.fetch_add(1, Ordering::Relaxed);
        })
    }

    async fn health_check(self, _context: Context) -> Result<HealthStatus, ServiceError> {
        let uptime = self.start_time.elapsed();
        let (active_count, queued_count) = {
            let workloads = self
                .workloads
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let active_count = workloads
                .values()
                .filter(|w| matches!(w.status, WorkloadStatus::Running | WorkloadStatus::Queued))
                .count();
            let queued_count = workloads
                .values()
                .filter(|w| matches!(w.status, WorkloadStatus::Queued))
                .count();
            (active_count, queued_count)
        };

        #[expect(
            clippy::cast_possible_truncation,
            reason = "truncation acceptable for this conversion"
        )]
        let error_count = self.error_count.load(Ordering::Relaxed) as usize; // display/metrics only

        Ok(HealthStatus::ok()
            .with_uptime(uptime.as_secs())
            .with_detail("version", serde_json::Value::String(self.version.to_string()))
            .with_detail("resource_utilization", serde_json::json!(self.calculate_resource_utilization().await))
            .with_detail("active_workloads", serde_json::json!(active_count))
            .with_detail("queued_workloads", serde_json::json!(queued_count))
            .with_detail("error_count", serde_json::json!(error_count)))
    }
}

#[cfg(test)]
#[path = "../tarpc_server_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../tarpc_server_coverage_expansion_tests.rs"]
mod coverage_expansion_tests;
