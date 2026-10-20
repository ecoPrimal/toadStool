// SPDX-License-Identifier: AGPL-3.0-or-later
//! Core types and data structures for protocol integration

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Protocol integration errors
#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    /// Connection to service failed
    #[error("Connection failed: {0}")]
    Connection(String),

    /// Authentication failed
    #[error("Authentication failed: {0}")]
    Authentication(String),

    /// Authorization denied
    #[error("Authorization failed: {0}")]
    Authorization(String),

    /// Protocol negotiation failed
    #[error("Protocol negotiation failed: {0}")]
    Negotiation(String),

    /// Message serialization failed
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// Transport layer error
    #[error("Transport error: {0}")]
    Transport(String),

    /// HTTP transport is not available in this deployment (use JSON-RPC over Unix socket to Coordination).
    #[error("HTTP transport not available: use JSON-RPC over Unix socket to Coordination for IPC")]
    HttpTransportNotAvailable,

    /// tRPC transport is not wired; use JSON-RPC for IPC until Phase 3.
    #[error("tRPC transport not available: use JSON-RPC via pure_jsonrpc for IPC")]
    TRpcTransportNotAvailable,

    /// Service discovery failed
    #[error("Service discovery error: {0}")]
    Discovery(String),

    /// Message routing failed
    #[error("Message routing error: {0}")]
    Routing(String),

    /// Operation timed out
    #[error("Timeout: {0}")]
    Timeout(String),

    /// Network error
    #[error("Network error: {0}")]
    Network(String),

    /// JSON serialization error
    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    /// I/O error
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Internal error
    #[error("Internal error: {0}")]
    Internal(String),
}

/// Result type for protocol operations
pub type ProtocolResult<T> = Result<T, ProtocolError>;

/// Message formats supported by the protocol system
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MessageFormat {
    /// JSON format (human-readable)
    Json,
    /// MessagePack (binary, compact)
    MessagePack,
    /// CBOR (binary, extensible)
    Cbor,
    /// Custom format
    Custom(String),
}

/// Transport types for message delivery
/// WebSocket removed — use JSON-RPC 2.0 (biomeOS/coordination)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum TransportType {
    /// HTTP/HTTPS transport
    Http,
    /// Pure Rust tRPC transport
    TRpc,
    /// Binary-framed primal IPC (MessagePack + length-delimited) after `TSB1` handshake
    #[cfg(all(feature = "tarpc-transport", feature = "binary-transport"))]
    Binary,
    /// TCP socket transport
    Tcp,
    /// UDP socket transport
    Udp,
    /// Custom transport
    Custom(String),
}

/// Authentication types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuthType {
    /// No authentication
    None,
    /// Bearer token authentication
    Bearer,
    /// API key authentication
    ApiKey,
    /// Mutual TLS authentication
    MutualTls,
    /// JWT authentication
    Jwt,
    /// Custom authentication
    Custom(String),
}

/// Message priority levels
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum MessagePriority {
    /// Low priority message
    Low,
    /// Normal priority message
    #[default]
    Normal,
    /// High priority message
    High,
    /// Critical priority message
    Critical,
    /// Emergency priority message
    Emergency,
}

// Canonical HealthStatus from cellmembrane-types (struct, not enum).
// Local enum removed — was (Healthy, Degraded, Unhealthy, Unknown).
// Use cellmembrane_types::HealthStatus { healthy: bool, message, checks, details, .. }.
// For enum-like status, see cellmembrane_types::ServiceStatus.
pub use cellmembrane_types::HealthStatus;

/// Protocol message structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolMessage {
    /// Message ID
    pub id: Uuid,

    /// Message type (`Arc<str>` = zero-copy clone)
    pub message_type: Arc<str>,

    /// Source service ID (`Arc<str>` = zero-copy clone)
    pub source: Arc<str>,

    /// Destination service ID (`Arc<str>` = zero-copy clone)
    pub destination: Option<Arc<str>>,

    /// Message payload
    pub payload: serde_json::Value,

    /// Message headers
    pub headers: HashMap<String, String>,

    /// Message timestamp
    #[serde(with = "toadstool_common::system_time_serde")]
    pub timestamp: std::time::SystemTime,

    /// Message format
    pub format: MessageFormat,

    /// Correlation ID for request-response patterns
    pub correlation_id: Option<Uuid>,

    /// Reply-to address for responses (`Arc<str>` = zero-copy clone)
    pub reply_to: Option<Arc<str>>,

    /// Message TTL
    pub ttl: Option<Duration>,

    /// Message priority
    pub priority: MessagePriority,
}

// Canonical ServiceInfo from cellmembrane-types.
// Local struct removed — was {id: Arc<str>, name: Arc<str>, version, endpoints: Vec<ServiceEndpoint>, metadata, health_status, last_seen, capabilities}.
// Canonical uses String (not Arc<str>), endpoints: Vec<String>, status: ServiceStatus, metadata: HashMap<String,String>.
// Extra fields (last_seen, health_status as HealthStatus struct) go in metadata.
pub use cellmembrane_types::ServiceInfo;

/// Service endpoint information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceEndpoint {
    /// Endpoint ID
    pub id: String,

    /// Transport type
    pub transport: TransportType,

    /// Endpoint address
    pub address: String,

    /// Endpoint port
    pub port: u16,

    /// Endpoint path
    pub path: Option<String>,

    /// TLS enabled
    pub tls_enabled: bool,

    /// Endpoint health status
    pub health_status: HealthStatus,
}

/// Protocol events for monitoring and notifications
#[derive(Debug, Clone)]
pub enum ProtocolEvent {
    /// Service registered in discovery
    ServiceRegistered {
        /// Registered service info
        service: ServiceInfo,
    },

    /// Service deregistered
    ServiceDeregistered {
        /// Service identifier
        service_id: String,
    },

    /// Service health status changed
    ServiceHealthChanged {
        /// Service identifier
        service_id: String,
        /// New health status
        status: HealthStatus,
    },

    /// Message sent to destination
    MessageSent {
        /// Message identifier
        message_id: Uuid,
        /// Destination address
        destination: String,
    },

    /// Message received from source
    MessageReceived {
        /// Message identifier
        message_id: Uuid,
        /// Source address
        source: String,
    },

    /// Connection established to service
    ConnectionEstablished {
        /// Service identifier
        service_id: String,
        /// Endpoint address
        endpoint: String,
    },

    /// Connection lost
    ConnectionLost {
        /// Service identifier
        service_id: String,
        /// Error description
        error: String,
    },
}

/// Message handler trait for processing incoming messages
pub trait MessageHandler: Send + Sync {
    /// Handle incoming message
    fn handle_message(
        &self,
        message: ProtocolMessage,
    ) -> Result<Option<ProtocolMessage>, ProtocolError>;
}
