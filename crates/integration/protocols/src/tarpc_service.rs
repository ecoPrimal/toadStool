// SPDX-License-Identifier: AGPL-3.0-or-later
//! # ToadStool Compute RPC Service (tarpc)
//!
//! High-performance, type-safe binary RPC protocol for primal-to-primal communication.
//! Following Coordination's proven pattern for ecosystem integration.
//!
//! ## Design Principles (from Coordination)
//!
//! - **Pure Rust**: No C++ dependencies (no gRPC/protobuf)
//! - **Type-Safe**: Full Rust type checking at compile time
//! - **Async Native**: Built on tokio
//! - **Low Latency**: Binary encoding, direct communication
//! - **High Throughput**: Optimized for high-volume operations
//!
//! ## Architecture (wateringHole Standard)
//!
//! ```text
//! ┌─────────────────────────────────────────┐
//! │      ToadStool Compute Service          │
//! ├─────────────────────────────────────────┤
//! │                                          │
//! │  JSON-RPC 2.0 (PRIMARY)  tarpc (OPTIONAL)│
//! │  Universal RPC           Binary RPC      │
//! │  ↓                       ↓               │
//! │  All Primals             Performance     │
//! │  • Security               Critical Paths  │
//! │  • Coordination                              │
//! │  • Storage                              │
//! │  • External Clients                      │
//! │    (Python, JS, etc.)                    │
//! │                                          │
//! │  HTTP (DEPRECATED - via Coordination)       │
//! └─────────────────────────────────────────┘
//! ```

use bytes::Bytes;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashMap;
use std::sync::Arc;

fn serialize_arc_str<S>(v: &Arc<str>, s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    s.serialize_str(v)
}

fn deserialize_arc_str<'de, D>(d: D) -> Result<Arc<str>, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(d)?;
    Ok(Arc::from(s))
}

/// Workload submission for compute execution.
///
/// `data` uses [`bytes::Bytes`] — an `Arc<[u8]>` — so passing a submission
/// through multiple handler layers or threads costs a single refcount bump
/// rather than copying potentially megabyte-sized payloads.
///
/// `workload_id` and `workload_type` use `Arc<str>` per wateringHole zero-copy
/// guidelines: clone is a refcount bump, not allocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkloadSubmission {
    /// Unique workload identifier
    #[serde(
        serialize_with = "serialize_arc_str",
        deserialize_with = "deserialize_arc_str"
    )]
    pub workload_id: Arc<str>,
    /// Workload type (gpu_compute, cpu_compute, wasm, etc.)
    #[serde(
        serialize_with = "serialize_arc_str",
        deserialize_with = "deserialize_arc_str"
    )]
    pub workload_type: Arc<str>,
    /// Binary workload data (zero-copy: clone bumps refcount, not a memcpy)
    pub data: Bytes,
    /// Workload metadata
    pub metadata: HashMap<String, String>,
    /// Priority level
    pub priority: WorkloadPriority,
    /// Resource requirements
    pub requirements: ResourceRequirements,
}

/// Workload priority levels for scheduling
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum WorkloadPriority {
    /// Low priority
    Low,
    /// Normal priority
    Normal,
    /// High priority
    High,
    /// Critical priority
    Critical,
}

/// Resource requirements for workload execution
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResourceRequirements {
    /// CPU cores required
    pub cpu_cores: Option<u32>,
    /// Memory required (bytes)
    pub memory_bytes: Option<u64>,
    /// GPU memory required (bytes)
    pub gpu_memory_bytes: Option<u64>,
    /// Execution timeout (seconds)
    pub timeout_secs: Option<u64>,
}

/// Workload execution result.
///
/// `data` uses [`bytes::Bytes`] so result payloads can be shared across
/// multiple consumers (e.g. cache + caller) without copying.
///
/// `workload_id` uses `Arc<str>` per wateringHole zero-copy guidelines.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkloadResult {
    /// Workload identifier
    #[serde(
        serialize_with = "serialize_arc_str",
        deserialize_with = "deserialize_arc_str"
    )]
    pub workload_id: Arc<str>,
    /// Execution status
    pub status: WorkloadStatus,
    /// Result data (if successful; zero-copy clone)
    pub data: Option<Bytes>,
    /// Error message (if failed)
    pub error: Option<String>,
    /// Execution metrics
    pub metrics: ExecutionMetrics,
}

/// Workload execution status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WorkloadStatus {
    /// Awaiting submission
    Pending,
    /// Queued for execution
    Queued,
    /// Currently executing
    Running,
    /// Completed successfully
    Completed,
    /// Execution failed
    Failed,
    /// Cancelled by user
    Cancelled,
}

/// Execution metrics for performance tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionMetrics {
    /// Time queued (seconds)
    pub queued_duration_secs: f64,
    /// Time executing (seconds)
    pub execution_duration_secs: f64,
    /// CPU cores used
    pub cpu_cores_used: u32,
    /// Memory used (bytes)
    pub memory_used_bytes: u64,
    /// GPU memory used (bytes)
    pub gpu_memory_used_bytes: Option<u64>,
}

/// Compute capabilities query result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputeCapabilities {
    /// Service identifier
    pub service_id: String,
    /// Available compute units
    pub compute_units: Vec<ComputeUnit>,
    /// Supported workload types
    pub supported_workload_types: Vec<String>,
    /// Total available resources
    pub available_resources: AvailableResources,
    /// Service metadata
    pub metadata: HashMap<String, String>,
}

/// Individual compute unit (CPU, GPU, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputeUnit {
    /// Unit identifier
    pub id: String,
    /// Unit type (cpu, gpu, neuromorphic, etc.)
    pub unit_type: String,
    /// Unit name
    pub name: String,
    /// Available cores/processors
    pub cores: u32,
    /// Available memory (bytes)
    pub memory_bytes: u64,
    /// Compute throughput (TFLOPS)
    pub tflops: Option<f64>,
    /// Current utilization (0.0-1.0)
    pub utilization: f32,
}

/// Available resources across all compute units
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvailableResources {
    /// Total CPU cores
    pub total_cpu_cores: u32,
    /// Available CPU cores
    pub available_cpu_cores: u32,
    /// Total memory (bytes)
    pub total_memory_bytes: u64,
    /// Available memory (bytes)
    pub available_memory_bytes: u64,
    /// Total GPU memory (bytes)
    pub total_gpu_memory_bytes: Option<u64>,
    /// Available GPU memory (bytes)
    pub available_gpu_memory_bytes: Option<u64>,
    /// Current CPU utilization (0.0-1.0)
    pub cpu_utilization: f32,
    /// Current memory utilization (0.0-1.0)
    pub memory_utilization: f32,
    /// Current GPU utilization (0.0-1.0)
    pub gpu_utilization: Option<f32>,
}

/// Semantic method name mapping for ToadStool tarpc service
///
/// Maps Rust method names (required by tarpc) to semantic method names
/// following wateringHole SEMANTIC_METHOD_NAMING_STANDARD.md format:
/// `{domain}.{operation}[.{variant}]`
///
/// Domain: `toadstool` (ToadStool compute service)
pub mod semantic_methods {
    /// Get semantic method name for a Rust method name
    ///
    /// # Examples
    ///
    /// ```
    /// use toadstool_integration_protocols::tarpc_service::semantic_methods::get_semantic_name;
    ///
    /// assert_eq!(get_semantic_name("submit_workload"), Some("toadstool.submit_workload"));
    /// assert_eq!(get_semantic_name("health_check"), Some("toadstool.health"));
    /// ```
    pub fn get_semantic_name(rust_method: &str) -> Option<&'static str> {
        METHOD_MAPPING
            .iter()
            .find(|(rust, _)| *rust == rust_method)
            .map(|(_, semantic)| *semantic)
    }

    /// Get Rust method name for a semantic method name
    ///
    /// # Examples
    ///
    /// ```
    /// use toadstool_integration_protocols::tarpc_service::semantic_methods::get_rust_method;
    ///
    /// assert_eq!(get_rust_method("toadstool.submit_workload"), Some("submit_workload"));
    /// assert_eq!(get_rust_method("toadstool.health"), Some("health_check"));
    /// ```
    pub fn get_rust_method(semantic_method: &str) -> Option<&'static str> {
        REVERSE_MAPPING
            .iter()
            .find(|(semantic, _)| *semantic == semantic_method)
            .map(|(_, rust)| *rust)
    }

    /// Check if a method name is a semantic method name
    pub fn is_semantic_method(method: &str) -> bool {
        method.contains('.') && get_rust_method(method).is_some()
    }

    /// Get all semantic method names
    pub fn all_semantic_methods() -> Vec<&'static str> {
        REVERSE_MAPPING
            .iter()
            .map(|(semantic, _)| *semantic)
            .collect()
    }

    /// Mapping: Rust method name → Semantic method name
    const METHOD_MAPPING: &[(&str, &str)] = &[
        ("submit_workload", "toadstool.submit_workload"),
        ("query_status", "toadstool.query_status"),
        ("cancel_workload", "toadstool.cancel_workload"),
        ("list_workloads", "toadstool.list_workloads"),
        ("query_capabilities", "toadstool.query_capabilities"),
        ("health_check", "toadstool.health"),
    ];

    /// Reverse mapping: Semantic method name → Rust method name
    const REVERSE_MAPPING: &[(&str, &str)] = &[
        ("toadstool.submit_workload", "submit_workload"),
        ("toadstool.query_status", "query_status"),
        ("toadstool.cancel_workload", "cancel_workload"),
        ("toadstool.list_workloads", "list_workloads"),
        ("toadstool.query_capabilities", "query_capabilities"),
        ("toadstool.health", "health_check"),
    ];
}

/// tarpc service definition for ToadStool compute operations
///
/// This trait defines the RPC interface following Coordination's pattern:
/// - Binary protocol for performance
/// - Type-safe at compile time
/// - Async throughout
/// - Self-describing (capabilities query)
///
/// ## Semantic Method Names
///
/// Each method has a corresponding semantic name following wateringHole
/// SEMANTIC_METHOD_NAMING_STANDARD.md format: `{domain}.{operation}[.{variant}]`
///
/// | Rust Method | Semantic Name |
/// |-------------|---------------|
/// | `submit_workload` | `toadstool.submit_workload` |
/// | `query_status` | `toadstool.query_status` |
/// | `cancel_workload` | `toadstool.cancel_workload` |
/// | `list_workloads` | `toadstool.list_workloads` |
/// | `query_capabilities` | `toadstool.query_capabilities` |
/// | `health_check` | `toadstool.health` |
///
/// Use `semantic_methods::get_semantic_name()` to convert Rust method names
/// to semantic names for JSON-RPC interop.
/// Errors returned by tarpc RPC service methods.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
pub enum ServiceError {
    /// The requested workload was not found.
    #[error("workload not found: {workload_id}")]
    WorkloadNotFound {
        /// ID of the missing workload.
        workload_id: String,
    },

    /// Workload execution failed.
    #[error("execution failed: {0}")]
    ExecutionFailed(String),

    /// The workload ID was invalid or malformed.
    #[error("invalid workload ID: {workload_id} — {detail}")]
    InvalidWorkloadId {
        /// The invalid workload ID.
        workload_id: String,
        /// Explanation of why the ID is invalid.
        detail: String,
    },

    /// Workload cancellation failed.
    #[error("cancel failed: {0}")]
    CancelFailed(String),

    /// Coordinator-level error.
    #[error("coordinator error: {0}")]
    Coordinator(String),
}

/// Core RPC service for workload execution and management.
#[tarpc::service]
pub trait ToadStoolComputeRpc {
    /// Submit workload for execution
    ///
    /// **Semantic Name**: `toadstool.submit_workload`
    ///
    /// # Arguments
    /// * `submission` - Workload submission details
    ///
    /// # Returns
    /// * `WorkloadResult` - Initial submission result with workload ID
    async fn submit_workload(
        submission: WorkloadSubmission,
    ) -> Result<WorkloadResult, ServiceError>;

    /// Query workload execution status
    ///
    /// **Semantic Name**: `toadstool.query_status`
    ///
    /// # Arguments
    /// * `workload_id` - Workload identifier
    ///
    /// # Returns
    /// * `WorkloadResult` - Current execution status and results
    async fn query_status(workload_id: String) -> Result<WorkloadResult, ServiceError>;

    /// Cancel running workload
    ///
    /// **Semantic Name**: `toadstool.cancel_workload`
    ///
    /// # Arguments
    /// * `workload_id` - Workload identifier
    ///
    /// # Returns
    /// * Success or error message
    async fn cancel_workload(workload_id: String) -> Result<(), ServiceError>;

    /// List all workloads for a given filter
    ///
    /// **Semantic Name**: `toadstool.list_workloads`
    ///
    /// # Arguments
    /// * `filter` - Optional filter (status, type, etc.)
    ///
    /// # Returns
    /// * List of workload results
    async fn list_workloads(
        filter: Option<HashMap<String, String>>,
    ) -> Result<Vec<WorkloadResult>, ServiceError>;

    /// Query compute capabilities (self-knowledge pattern)
    ///
    /// **Semantic Name**: `toadstool.query_capabilities`
    ///
    /// This follows the "primal only knows itself" principle:
    /// - Returns only this primal's capabilities
    /// - No knowledge of other primals
    /// - Discovery happens at runtime
    ///
    /// # Returns
    /// * `ComputeCapabilities` - This primal's compute resources
    async fn query_capabilities() -> Result<ComputeCapabilities, ServiceError>;

    /// Health check endpoint
    ///
    /// **Semantic Name**: `toadstool.health`
    ///
    /// # Returns
    /// * Service health status
    async fn health_check() -> Result<HealthStatus, ServiceError>;
}

// Canonical HealthStatus from cellmembrane-types.
// Local struct removed — extra fields (version, resource_utilization, active_workloads,
// queued_workloads, error_count) now stored via the canonical `details` map.
// Use: HealthStatus::ok().with_uptime(secs).with_detail("version", ver) etc.
pub use cellmembrane_types::HealthStatus;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workload_submission_serialization() {
        let submission = WorkloadSubmission {
            workload_id: Arc::from("work-123"),
            workload_type: Arc::from("gpu_compute"),
            data: bytes::Bytes::from(vec![1, 2, 3, 4]),
            metadata: HashMap::new(),
            priority: WorkloadPriority::Normal,
            requirements: ResourceRequirements {
                cpu_cores: Some(4),
                memory_bytes: Some(1024 * 1024 * 1024),
                gpu_memory_bytes: Some(512 * 1024 * 1024),
                timeout_secs: Some(300),
            },
        };

        // Verify serialization works
        let json = serde_json::to_string(&submission).expect("Serialization failed");
        let deserialized: WorkloadSubmission =
            serde_json::from_str(&json).expect("Deserialization failed");

        assert_eq!(
            submission.workload_id.as_ref(),
            deserialized.workload_id.as_ref()
        );
    }

    #[test]
    fn test_compute_capabilities_structure() {
        let capabilities = ComputeCapabilities {
            service_id: "toadstool-1".to_string(),
            compute_units: vec![ComputeUnit {
                id: "cpu-0".to_string(),
                unit_type: "cpu".to_string(),
                name: "AMD Ryzen".to_string(),
                cores: 128,
                memory_bytes: 270 * 1024 * 1024 * 1024,
                tflops: Some(12.8),
                utilization: 0.25,
            }],
            supported_workload_types: vec![
                "cpu_compute".to_string(),
                "gpu_compute".to_string(),
                "wasm".to_string(),
            ],
            available_resources: AvailableResources {
                total_cpu_cores: 128,
                available_cpu_cores: 96,
                total_memory_bytes: 270 * 1024 * 1024 * 1024,
                available_memory_bytes: 200 * 1024 * 1024 * 1024,
                total_gpu_memory_bytes: Some(40 * 1024 * 1024 * 1024),
                available_gpu_memory_bytes: Some(30 * 1024 * 1024 * 1024),
                cpu_utilization: 0.25,
                memory_utilization: 0.26,
                gpu_utilization: Some(0.15),
            },
            metadata: HashMap::new(),
        };

        // Verify structure is valid
        assert_eq!(capabilities.compute_units.len(), 1);
        assert_eq!(capabilities.compute_units[0].cores, 128);
    }
}
