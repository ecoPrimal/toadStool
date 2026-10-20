// SPDX-License-Identifier: AGPL-3.0-or-later
//! Service Discovery Fallback Defaults
//!
//! This module provides fallback configuration for service discovery.
//! These values are ONLY used when:
//! 1. Automatic discovery fails
//! 2. Running in development/test mode
//! 3. Explicitly enabled via environment variables
//!
//! ## Infant Discovery Pattern
//!
//! ToadStool starts with zero knowledge and discovers services at runtime.
//! These fallbacks allow graceful degradation in development environments.
//!
//! ## Production Behavior
//!
//! In production, discovery should always succeed. If it doesn't, fail fast
//! rather than falling back to localhost defaults.
//!
//! ## Evolution History
//!
//! - **Feb 14, 2026**: Removed hardcoded localhost URLs. All fallbacks now require
//!   explicit environment variable configuration (self-knowledge principle).
//! - **Mar 31, 2026**: `ServiceDiscovery` fallbacks prefer
//!   `$XDG_RUNTIME_DIR/ecoPrimals/{capability}.sock` (wateringHole); TCP `*_URL` fallbacks
//!   remain last resort and are logged as deprecated.
//! - **Phase 3/4 (Complete)**: mDNS/DNS-SD discovery fully deployed via
//!   `MdnsDiscoveryService`, `primal_discovery_mdns`, and `discovery_engine`.
//!   This module retained for development/test graceful degradation only.

// EVOLVED (Feb 14, 2026): Removed unused imports after localhost fallback removal
// - DEFAULT_HTTP_PORT, primals, runtime_ports no longer needed
use std::env;

use crate::interned_strings::socket_env;

const _DEFAULT_DISCOVERY_TIMEOUT_SECS: u64 = 5;
const _DEFAULT_CACHE_TTL_SECS: u64 = 60;
const PROD_DISCOVERY_TIMEOUT_SECS: u64 = 10;
const PROD_CACHE_TTL_SECS: u64 = 300;
const DEV_DISCOVERY_TIMEOUT_SECS: u64 = 2;
const DEV_CACHE_TTL_SECS: u64 = 10;
const TEST_DISCOVERY_TIMEOUT_MS: u64 = 100;
const TEST_CACHE_TTL_SECS: u64 = 1;

// Canonical DiscoveryConfig from cellmembrane-types.
// Local struct removed — was {enable_localhost_fallback, timeout, max_retries, cache_ttl, allow_insecure}.
// Canonical: {methods, interval_secs, timeout_secs, max_services, cache_enabled, cache_ttl_secs, retries, enable_ipv6, fallbacks}.
pub use cellmembrane_types::DiscoveryConfig;

/// Environment-specific discovery config constructors (toadStool extensions).
pub trait DiscoveryConfigExt {
    /// Production configuration (strict, no fallbacks).
    fn production() -> DiscoveryConfig;
    /// Development configuration (permissive, with fallbacks).
    fn development() -> DiscoveryConfig;
    /// Test configuration (fast, local only).
    fn test_config() -> DiscoveryConfig;
}

impl DiscoveryConfigExt for DiscoveryConfig {
    fn production() -> DiscoveryConfig {
        DiscoveryConfig {
            timeout_secs: PROD_DISCOVERY_TIMEOUT_SECS,
            cache_ttl_secs: PROD_CACHE_TTL_SECS,
            retries: 5,
            ..DiscoveryConfig::default()
        }
    }

    fn development() -> DiscoveryConfig {
        DiscoveryConfig {
            timeout_secs: DEV_DISCOVERY_TIMEOUT_SECS,
            cache_ttl_secs: DEV_CACHE_TTL_SECS,
            retries: 1,
            ..DiscoveryConfig::default()
        }
    }

    fn test_config() -> DiscoveryConfig {
        DiscoveryConfig {
            timeout_secs: TEST_DISCOVERY_TIMEOUT_MS / 1000,
            cache_ttl_secs: TEST_CACHE_TTL_SECS,
            retries: 0,
            ..DiscoveryConfig::default()
        }
    }
}

/// Localhost fallback defaults for development
///
/// **Note**: Production environments should use capability-based discovery
/// via mDNS/DNS-SD (fully deployed as of Phase 4, Feb 2026).
/// These fallbacks exist only for development/test graceful degradation.
#[derive(Debug, Clone)]
pub struct LocalhostFallbacks {
    /// Enable fallbacks (should be false in production)
    pub enabled: bool,
}

impl Default for LocalhostFallbacks {
    fn default() -> Self {
        Self {
            enabled: env::var(socket_env::TOADSTOOL_ENV)
                .unwrap_or_else(|_| "development".to_string())
                != "production",
        }
    }
}

impl LocalhostFallbacks {
    /// Get fallback URL for a service from environment only
    ///
    /// Returns None if:
    /// - Fallbacks are disabled (production mode)
    /// - No environment variable is set for the service
    ///
    /// **EVOLVED (Feb 14, 2026)**: Removed hardcoded localhost URLs.
    /// Primal self-knowledge principle: a primal cannot assume localhost endpoints exist.
    /// All fallback URLs must come from explicit environment configuration.
    ///
    /// Environment variable format: `{SERVICE_TYPE}_URL` (e.g., `REDIS_URL`, `TOADSTOOL_URL`)
    #[must_use]
    pub fn get_fallback_url(&self, service_type: &str) -> Option<String> {
        if !self.enabled {
            return None;
        }

        // EVOLVED: Only read from environment variables
        // No hardcoded localhost - violates self-knowledge principle
        env::var(format!("{}_URL", service_type.to_uppercase())).ok()
    }

    /// Check if fallbacks should be used
    #[must_use]
    pub const fn should_use_fallback(&self) -> bool {
        self.enabled
    }
}

/// Discovery error handling strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscoveryErrorStrategy {
    /// Fail fast - no fallbacks, error immediately
    FailFast,

    /// Try fallback - use localhost defaults if discovery fails
    TryFallback,

    /// Silent fallback - use fallback without logging errors
    SilentFallback,
}

impl Default for DiscoveryErrorStrategy {
    fn default() -> Self {
        let is_production = env::var(socket_env::TOADSTOOL_ENV)
            .unwrap_or_else(|_| "development".to_string())
            == "production";

        if is_production {
            Self::FailFast
        } else {
            Self::TryFallback
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discovery_config_default() {
        let config = DiscoveryConfig::default();
        assert!(config.cache_enabled);
        assert_eq!(config.retries, 3);
    }

    #[test]
    fn test_discovery_config_production() {
        let config = DiscoveryConfig::production();
        assert_eq!(config.retries, 5);
        assert_eq!(config.timeout_secs, PROD_DISCOVERY_TIMEOUT_SECS);
    }

    #[test]
    fn test_discovery_config_development() {
        let config = DiscoveryConfig::development();
        assert_eq!(config.retries, 1);
        assert_eq!(config.timeout_secs, DEV_DISCOVERY_TIMEOUT_SECS);
    }

    #[test]
    fn test_discovery_config_test() {
        let config = DiscoveryConfig::test_config();
        assert_eq!(config.retries, 0);
    }

    #[test]
    fn test_localhost_fallbacks_default() {
        let fallbacks = LocalhostFallbacks::default();
        // Should be enabled in non-production
        assert!(fallbacks.enabled);
    }

    #[test]
    fn test_localhost_fallback_urls() {
        // EVOLVED (Feb 14, 2026): Fallbacks now require explicit env vars
        // No hardcoded localhost - self-knowledge principle
        let fallbacks = LocalhostFallbacks { enabled: true };

        // Without env vars set, fallback returns None
        // (unless env var is set by another test)
        let unknown_url = fallbacks.get_fallback_url("unknown_service_xyz");
        assert_eq!(unknown_url, None);
    }

    #[test]
    fn test_localhost_fallbacks_disabled() {
        let fallbacks = LocalhostFallbacks { enabled: false };

        // Should return None when disabled
        assert_eq!(fallbacks.get_fallback_url("toadstool"), None);
        assert!(!fallbacks.should_use_fallback());
    }

    #[test]
    fn test_discovery_error_strategy_default() {
        let strategy = DiscoveryErrorStrategy::default();
        // Should try fallback in non-production (default)
        assert_eq!(strategy, DiscoveryErrorStrategy::TryFallback);
    }

    #[test]
    fn test_discovery_config_default_production_mode() {
        temp_env::with_var("TOADSTOOL_ENV", Some("production"), || {
            let config = DiscoveryConfig::default();
            assert!(config.cache_enabled);
        });
    }

    #[test]
    fn test_localhost_fallbacks_default_production_mode() {
        temp_env::with_var("TOADSTOOL_ENV", Some("production"), || {
            let fallbacks = LocalhostFallbacks::default();
            assert!(!fallbacks.enabled);
        });
    }

    #[test]
    fn test_discovery_error_strategy_default_production_mode() {
        temp_env::with_var("TOADSTOOL_ENV", Some("production"), || {
            let strategy = DiscoveryErrorStrategy::default();
            assert_eq!(strategy, DiscoveryErrorStrategy::FailFast);
        });
    }

    #[test]
    fn test_get_fallback_url_env_override() {
        temp_env::with_var("REDIS_URL", Some("redis://custom.example.com:6380"), || {
            let fallbacks = LocalhostFallbacks { enabled: true };
            let url = fallbacks.get_fallback_url("redis");
            assert_eq!(url.as_deref(), Some("redis://custom.example.com:6380"));
        });
    }

    #[test]
    fn test_get_fallback_url_toadstool_env_override() {
        temp_env::with_var("TOADSTOOL_URL", Some("http://toadstool.local:9999"), || {
            let fallbacks = LocalhostFallbacks { enabled: true };
            let url = fallbacks.get_fallback_url("toadstool");
            assert_eq!(url.as_deref(), Some("http://toadstool.local:9999"));
        });
    }

    #[test]
    fn test_get_fallback_url_postgres_env_override() {
        temp_env::with_var(
            "POSTGRES_URL",
            Some("postgresql://db.example.com:5433"),
            || {
                let fallbacks = LocalhostFallbacks { enabled: true };
                let url = fallbacks.get_fallback_url("postgres");
                assert_eq!(url.as_deref(), Some("postgresql://db.example.com:5433"));
            },
        );
    }

    #[test]
    fn test_get_fallback_url_mongodb_env_override() {
        temp_env::with_var(
            "MONGODB_URL",
            Some("mongodb://mongo.example.com:27018"),
            || {
                let fallbacks = LocalhostFallbacks { enabled: true };
                let url = fallbacks.get_fallback_url("mongodb");
                assert_eq!(url.as_deref(), Some("mongodb://mongo.example.com:27018"));
            },
        );
    }

    #[test]
    fn test_discovery_config_timeouts() {
        let config = DiscoveryConfig::production();
        assert_eq!(config.timeout_secs, PROD_DISCOVERY_TIMEOUT_SECS);
        assert_eq!(config.retries, 5);
        assert_eq!(config.cache_ttl_secs, PROD_CACHE_TTL_SECS);

        let config = DiscoveryConfig::development();
        assert_eq!(config.timeout_secs, DEV_DISCOVERY_TIMEOUT_SECS);
        assert_eq!(config.retries, 1);
        assert_eq!(config.cache_ttl_secs, DEV_CACHE_TTL_SECS);

        let config = DiscoveryConfig::test_config();
        assert_eq!(config.retries, 0);
        assert_eq!(config.cache_ttl_secs, TEST_CACHE_TTL_SECS);
    }

    #[test]
    fn test_should_use_fallback_when_enabled() {
        let fallbacks = LocalhostFallbacks { enabled: true };
        assert!(fallbacks.should_use_fallback());
    }

    #[test]
    fn test_discovery_error_strategy_variants() {
        assert_eq!(
            DiscoveryErrorStrategy::FailFast,
            DiscoveryErrorStrategy::FailFast
        );
        assert_eq!(
            DiscoveryErrorStrategy::TryFallback,
            DiscoveryErrorStrategy::TryFallback
        );
        assert_eq!(
            DiscoveryErrorStrategy::SilentFallback,
            DiscoveryErrorStrategy::SilentFallback
        );
        assert_ne!(
            DiscoveryErrorStrategy::FailFast,
            DiscoveryErrorStrategy::TryFallback
        );
    }

    #[test]
    fn test_discovery_config_default_staging_env() {
        temp_env::with_var("TOADSTOOL_ENV", Some("staging"), || {
            let config = DiscoveryConfig::default();
            assert!(config.cache_enabled);
        });
    }

    #[test]
    fn test_discovery_config_default_empty_env() {
        temp_env::with_var_unset("TOADSTOOL_ENV", || {
            let config = DiscoveryConfig::default();
            assert!(config.cache_enabled);
        });
    }

    #[test]
    fn test_get_fallback_url_service_type_case_insensitive() {
        temp_env::with_var("REDIS_URL", Some("redis://custom:6379"), || {
            let fallbacks = LocalhostFallbacks { enabled: true };
            assert_eq!(
                fallbacks.get_fallback_url("redis").as_deref(),
                Some("redis://custom:6379")
            );
            assert_eq!(
                fallbacks.get_fallback_url("Redis").as_deref(),
                Some("redis://custom:6379")
            );
        });
    }

    #[test]
    fn test_discovery_config_debug() {
        let config = DiscoveryConfig::production();
        let debug_str = format!("{config:?}");
        assert!(debug_str.contains("DiscoveryConfig"));
        assert!(debug_str.contains("enable_localhost_fallback"));
    }

    #[test]
    fn test_localhost_fallbacks_debug() {
        let fallbacks = LocalhostFallbacks { enabled: true };
        let debug_str = format!("{fallbacks:?}");
        assert!(debug_str.contains("LocalhostFallbacks"));
        assert!(debug_str.contains("enabled"));
    }

    #[test]
    fn test_discovery_config_clone() {
        let config = DiscoveryConfig::production();
        let cloned = config.clone();
        assert_eq!(config.retries, cloned.retries);
        assert_eq!(config.timeout_secs, cloned.timeout_secs);
    }
}
