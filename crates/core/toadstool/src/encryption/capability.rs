// SPDX-License-Identifier: AGPL-3.0-or-later
//! Capability-based crypto service discovery
//!
//! **Design Philosophy**:
//! - No hardcoding: Discover crypto providers by capability, not name
//! - Runtime discovery: Find services dynamically
//! - Self-knowledge: Toadstool knows what it needs, not where to get it

use serde::{Deserialize, Serialize};

use super::SecurityLevel;

/// Crypto capability description
///
/// **Design**: Describes what crypto capabilities are needed,
/// not which specific service provides them
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CryptoCapability {
    /// Supported encryption algorithms
    pub algorithms: Vec<String>,

    /// Required security level
    pub security_level: SecurityLevel,

    /// Whether hardware-backed crypto is required
    pub hardware_backed: bool,
}

impl CryptoCapability {
    /// Check if this capability matches required features
    pub fn matches(&self, required: &Self) -> bool {
        // Check security level
        if self.security_level < required.security_level {
            return false;
        }

        // Check hardware requirement
        if required.hardware_backed && !self.hardware_backed {
            return false;
        }

        // Check if we support at least one required algorithm
        required
            .algorithms
            .iter()
            .any(|req_alg| self.algorithms.iter().any(|our_alg| our_alg == req_alg))
    }

    /// Score this capability match (higher = better match)
    ///
    /// **Design**: Allows prioritizing better matches
    pub fn match_score(&self, required: &Self) -> u32 {
        let mut score = 0u32;

        // Security level match (exact match = higher score)
        if self.security_level == required.security_level {
            score += 100;
        } else if self.security_level > required.security_level {
            score += 50; // Better than required, but not exact
        }

        // Hardware backing
        if self.hardware_backed && required.hardware_backed {
            score += 50;
        }

        // Algorithm matches (more = better)
        let matching_algorithms = required
            .algorithms
            .iter()
            .filter(|req_alg| self.algorithms.contains(req_alg))
            .count();
        score += u32::try_from(matching_algorithms)
            .unwrap_or(u32::MAX)
            .saturating_mul(10);

        score
    }
}

/// Crypto service discovery query
///
/// **Design**: Describes what we're looking for without hardcoding
#[derive(Debug, Clone)]
pub struct CryptoServiceQuery {
    /// Required capability
    pub capability: CryptoCapability,

    /// Preferred service location (None = any)
    pub preferred_location: Option<ServiceLocation>,

    /// Maximum acceptable latency
    pub max_latency_ms: Option<u64>,
}

impl CryptoServiceQuery {
    /// Create query for specific capability
    pub const fn for_capability(capability: CryptoCapability) -> Self {
        Self {
            capability,
            preferred_location: None,
            max_latency_ms: None,
        }
    }

    /// Prefer local services
    pub const fn prefer_local(mut self) -> Self {
        self.preferred_location = Some(ServiceLocation::Local);
        self
    }

    /// Prefer network services (distributed)
    pub const fn prefer_network(mut self) -> Self {
        self.preferred_location = Some(ServiceLocation::Network);
        self
    }

    /// Set maximum acceptable latency
    pub const fn max_latency(mut self, ms: u64) -> Self {
        self.max_latency_ms = Some(ms);
        self
    }
}

/// Service location preference
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceLocation {
    /// Local process (same machine)
    Local,
    /// Network service (remote, but same private network)
    Network,
    /// Internet service (public network)
    Internet,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capability_matching() {
        let provided = CryptoCapability {
            algorithms: vec!["chacha20poly1305".to_string(), "aes-256-gcm".to_string()],
            security_level: SecurityLevel::High,
            hardware_backed: false,
        };

        let required = CryptoCapability {
            algorithms: vec!["chacha20poly1305".to_string()],
            security_level: SecurityLevel::Standard,
            hardware_backed: false,
        };

        assert!(provided.matches(&required));
    }

    #[test]
    fn test_capability_no_match_security_level() {
        let provided = CryptoCapability {
            algorithms: vec!["chacha20poly1305".to_string()],
            security_level: SecurityLevel::Standard,
            hardware_backed: false,
        };

        let required = CryptoCapability {
            algorithms: vec!["chacha20poly1305".to_string()],
            security_level: SecurityLevel::High,
            hardware_backed: false,
        };

        assert!(!provided.matches(&required));
    }

    #[test]
    fn test_capability_scoring() {
        let provided = CryptoCapability {
            algorithms: vec!["chacha20poly1305".to_string(), "aes-256-gcm".to_string()],
            security_level: SecurityLevel::High,
            hardware_backed: true,
        };

        let required = CryptoCapability {
            algorithms: vec!["chacha20poly1305".to_string()],
            security_level: SecurityLevel::High,
            hardware_backed: true,
        };

        let score = provided.match_score(&required);
        assert!(score > 100); // Should have high score
    }

    #[test]
    fn test_capability_no_match_hardware_backed() {
        let provided = CryptoCapability {
            algorithms: vec!["chacha20poly1305".to_string()],
            security_level: SecurityLevel::High,
            hardware_backed: false,
        };

        let required = CryptoCapability {
            algorithms: vec!["chacha20poly1305".to_string()],
            security_level: SecurityLevel::Standard,
            hardware_backed: true,
        };

        assert!(!provided.matches(&required));
    }

    #[test]
    fn test_capability_no_match_algorithm() {
        let provided = CryptoCapability {
            algorithms: vec!["aes-256-gcm".to_string()],
            security_level: SecurityLevel::High,
            hardware_backed: false,
        };

        let required = CryptoCapability {
            algorithms: vec!["chacha20poly1305".to_string()],
            security_level: SecurityLevel::Standard,
            hardware_backed: false,
        };

        assert!(!provided.matches(&required));
    }

    #[test]
    fn test_capability_match_multiple_algorithms() {
        let provided = CryptoCapability {
            algorithms: vec![
                "chacha20poly1305".to_string(),
                "aes-256-gcm".to_string(),
                "xsalsa20".to_string(),
            ],
            security_level: SecurityLevel::Maximum,
            hardware_backed: true,
        };

        let required = CryptoCapability {
            algorithms: vec!["xsalsa20".to_string(), "unknown".to_string()],
            security_level: SecurityLevel::Standard,
            hardware_backed: false,
        };

        assert!(provided.matches(&required));
    }

    #[test]
    fn test_capability_match_score_exact_security() {
        let provided = CryptoCapability {
            algorithms: vec!["chacha20poly1305".to_string()],
            security_level: SecurityLevel::Standard,
            hardware_backed: false,
        };

        let required = CryptoCapability {
            algorithms: vec!["chacha20poly1305".to_string()],
            security_level: SecurityLevel::Standard,
            hardware_backed: false,
        };

        let score = provided.match_score(&required);
        assert!(score >= 100, "Exact security match should add 100");
    }

    #[test]
    fn test_capability_match_score_better_security() {
        let provided = CryptoCapability {
            algorithms: vec!["chacha20poly1305".to_string()],
            security_level: SecurityLevel::High,
            hardware_backed: false,
        };

        let required = CryptoCapability {
            algorithms: vec!["chacha20poly1305".to_string()],
            security_level: SecurityLevel::Standard,
            hardware_backed: false,
        };

        let score = provided.match_score(&required);
        assert!(score >= 50, "Better security should add 50");
    }

    #[test]
    fn test_capability_match_score_algorithm_count() {
        let provided = CryptoCapability {
            algorithms: vec!["chacha20poly1305".to_string(), "aes-256-gcm".to_string()],
            security_level: SecurityLevel::Standard,
            hardware_backed: false,
        };

        let required = CryptoCapability {
            algorithms: vec!["chacha20poly1305".to_string(), "aes-256-gcm".to_string()],
            security_level: SecurityLevel::Standard,
            hardware_backed: false,
        };

        let score = provided.match_score(&required);
        assert!(score >= 20, "Two algorithm matches should add 20");
    }

    #[test]
    fn test_crypto_service_query_for_capability() {
        let cap = CryptoCapability {
            algorithms: vec!["aes-256-gcm".to_string()],
            security_level: SecurityLevel::Standard,
            hardware_backed: false,
        };
        let query = CryptoServiceQuery::for_capability(cap.clone());

        assert_eq!(query.capability.algorithms, cap.algorithms);
        assert!(query.preferred_location.is_none());
        assert!(query.max_latency_ms.is_none());
    }

    #[test]
    fn test_crypto_service_query_prefer_local() {
        let cap = CryptoCapability {
            algorithms: vec!["chacha20poly1305".to_string()],
            security_level: SecurityLevel::Standard,
            hardware_backed: false,
        };
        let query = CryptoServiceQuery::for_capability(cap).prefer_local();

        assert_eq!(query.preferred_location, Some(ServiceLocation::Local));
    }

    #[test]
    fn test_crypto_service_query_prefer_network() {
        let cap = CryptoCapability {
            algorithms: vec!["chacha20poly1305".to_string()],
            security_level: SecurityLevel::Standard,
            hardware_backed: false,
        };
        let query = CryptoServiceQuery::for_capability(cap).prefer_network();

        assert_eq!(query.preferred_location, Some(ServiceLocation::Network));
    }

    #[test]
    fn test_crypto_service_query_max_latency() {
        let cap = CryptoCapability {
            algorithms: vec!["aes-256-gcm".to_string()],
            security_level: SecurityLevel::Standard,
            hardware_backed: false,
        };
        let query = CryptoServiceQuery::for_capability(cap).max_latency(100);

        assert_eq!(query.max_latency_ms, Some(100));
    }

    #[test]
    fn test_crypto_service_query_builder_chain() {
        let cap = CryptoCapability {
            algorithms: vec!["chacha20poly1305".to_string()],
            security_level: SecurityLevel::High,
            hardware_backed: true,
        };
        let query = CryptoServiceQuery::for_capability(cap)
            .prefer_local()
            .max_latency(50);

        assert_eq!(query.preferred_location, Some(ServiceLocation::Local));
        assert_eq!(query.max_latency_ms, Some(50));
    }

    #[test]
    fn test_service_location_variants() {
        assert_eq!(ServiceLocation::Local, ServiceLocation::Local);
        assert_eq!(ServiceLocation::Network, ServiceLocation::Network);
        assert_eq!(ServiceLocation::Internet, ServiceLocation::Internet);
        assert_ne!(ServiceLocation::Local, ServiceLocation::Network);
        assert_ne!(ServiceLocation::Local, ServiceLocation::Internet);
    }

    #[test]
    fn test_crypto_capability_serialization_roundtrip() {
        let cap = CryptoCapability {
            algorithms: vec!["chacha20poly1305".to_string(), "aes-256-gcm".to_string()],
            security_level: SecurityLevel::High,
            hardware_backed: true,
        };

        let json = serde_json::to_string(&cap).unwrap();
        let deserialized: CryptoCapability = serde_json::from_str(&json).unwrap();

        assert_eq!(cap.algorithms, deserialized.algorithms);
        assert_eq!(cap.security_level, deserialized.security_level);
        assert_eq!(cap.hardware_backed, deserialized.hardware_backed);
    }
}
