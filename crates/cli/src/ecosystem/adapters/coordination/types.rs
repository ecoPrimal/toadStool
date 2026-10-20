// SPDX-License-Identifier: AGPL-3.0-or-later
//! Data types for coordination: registration, discovery, and distributed locks.
//!
//! These structs are the stable surface for CLI and integrator code that registers
//! services, interprets discovery results, or holds lock handles from the
//! coordination capability layer.


// Canonical ServiceInfo from cellmembrane-types.
// Local struct removed — was {name, capabilities, endpoint, metadata}.
// Canonical: {id, name, version, endpoints: Vec<String>, capabilities, status, metadata}.
pub use cellmembrane_types::ServiceInfo;

/// Registration token from coordination service
#[derive(Debug, Clone)]
pub struct RegistrationToken {
    /// Opaque token for heartbeat and unregister
    pub token: String,
}

/// Peer service information from discovery
#[derive(Debug, Clone)]
pub struct PeerInfo {
    /// Peer service name
    pub name: String,
    /// Peer endpoint URL
    pub endpoint: String,
    /// Capabilities the peer provides
    pub capabilities: Vec<String>,
    /// Health status (healthy, unhealthy, unknown)
    pub health: String,
}

/// Distributed lock handle for coordination
#[derive(Debug, Clone)]
pub struct LockHandle {
    /// Lock name (resource being locked)
    pub lock_name: String,
    /// Unique lock ID for release
    pub lock_id: String,
}
