// SPDX-License-Identifier: AGPL-3.0-or-later

use super::*;
use std::path::Path;
use crate::interned_strings::socket_env;

/// Isolate localhost fallback from the developer machine: empty runtime dir and no socket env overrides.
fn with_clean_localhost_fallback_env<F: FnOnce() -> R, R>(runtime: &Path, f: F) -> R {
    let rt = runtime.to_str().expect("temp runtime path utf-8");
    temp_env::with_vars(
        [
            (socket_env::XDG_RUNTIME_DIR, Some(rt)),
            ("TOADSTOOL_LOCAL_PORT", None::<&str>),
            (socket_env::BIOMEOS_CRYPTO_SOCKET, None::<&str>),
            (socket_env::TOADSTOOL_SECURITY_SOCKET, None::<&str>),
            (socket_env::LEGACY_BEARDOG_SOCKET_ENV, None::<&str>),
            (socket_env::TOADSTOOL_SECURITY_ENDPOINT, None::<&str>),
            (socket_env::SECURITY_URL, None::<&str>),
            (socket_env::SECURITY_ENDPOINT, None::<&str>),
            (socket_env::LEGACY_BEARDOG_URL, None::<&str>),
            (socket_env::LEGACY_BEARDOG_ENDPOINT, None::<&str>),
            (socket_env::TOADSTOOL_SECURITY_URL, None::<&str>),
            ("SECURITY_PROVIDER_SOCKET", None::<&str>),
            ("CRYPTO_PROVIDER_SOCKET", None::<&str>),
            ("SECURITY_SOCKET", None::<&str>),
        ],
        f,
    )
}

#[test]
fn test_discovery_config_default() {
    let config = DiscoveryConfig::default();
    assert_eq!(config.timeout_secs, 300);
    // Default methods include mdns
    assert!(!config.methods.is_empty());
}

#[test]
fn test_discovery_config_custom() {
    let config = DiscoveryConfig {
        timeout_secs: 10,
        methods: vec!["mdns".into()],
        ..DiscoveryConfig::default()
    };
    assert_eq!(config.timeout_secs, 10);
    assert!(config.methods.contains(&"mdns".to_string()));
}

#[test]
fn test_discovery_method_copy() {
    let method1 = DiscoveryMethod::Auto;
    let method2 = method1; // Copy
    assert!(matches!(method1, DiscoveryMethod::Auto));
    assert!(matches!(method2, DiscoveryMethod::Auto));
}

#[test]
fn test_discovery_method_variants() {
    let auto = DiscoveryMethod::Auto;
    let mdns = DiscoveryMethod::Mdns;
    let env = DiscoveryMethod::Environment;

    assert!(matches!(auto, DiscoveryMethod::Auto));
    assert!(matches!(mdns, DiscoveryMethod::Mdns));
    assert!(matches!(env, DiscoveryMethod::Environment));
}

#[test]
fn test_discovery_error_timeout() {
    let err = DiscoveryError::Timeout;
    assert_eq!(err.to_string(), "Discovery timeout");
}

#[test]
fn test_discovery_error_no_services() {
    let err = DiscoveryError::NoServicesFound("test_capability".to_string());
    assert!(err.to_string().contains("test_capability"));
}

#[test]
fn test_discovery_error_failed() {
    let err = DiscoveryError::DiscoveryFailed("network error".to_string());
    assert!(err.to_string().contains("network error"));
}

#[test]
fn test_discovery_error_invalid_config() {
    let err = DiscoveryError::InvalidConfig("bad config".to_string());
    assert!(err.to_string().contains("bad config"));
}

#[test]
fn test_discovery_config_production_env() {
    temp_env::with_var("TOADSTOOL_ENV", Some("production"), || {
        let config = DiscoveryConfig::default();
        assert!(config.fallbacks.is_empty());
    });
}

#[test]
fn test_discovery_config_development_env() {
    temp_env::with_var_unset("TOADSTOOL_ENV", || {
        let config = DiscoveryConfig::default();
        // Default has no fallbacks (added per-environment)
        assert!(config.fallbacks.is_empty());
    });
}

#[test]
fn test_discovery_config_builder_pattern() {
    let config = DiscoveryConfig {
        timeout_secs: 0,
        methods: vec!["mdns".into(), "env".into()],
        fallbacks: std::collections::HashMap::from([("localhost".into(), "127.0.0.1:8080".into())]),
        ..DiscoveryConfig::default()
    };
    assert_eq!(config.timeout_secs, 0);
    assert!(config.fallbacks.contains_key("localhost"));
    assert_eq!(config.methods.len(), 2);
}

#[test]
fn test_discovery_config_clone() {
    let config1 = DiscoveryConfig::default();
    let config2 = config1.clone();
    assert_eq!(config1.timeout_secs, config2.timeout_secs);
    assert_eq!(config1.fallbacks, config2.fallbacks);
}

#[tokio::test]
async fn test_capability_discovery_new_async_from_spawned_task() {
    let result = tokio::spawn(async { CapabilityDiscovery::new_async().await })
        .await
        .expect("join");
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_capability_discovery_with_config_async() {
    let config = DiscoveryConfig {
        timeout_secs: 0,
        methods: vec!["env".into()],
        ..DiscoveryConfig::default()
    };
    let result = CapabilityDiscovery::with_config_async(&config).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_find_by_capability_no_services_in_separate_thread() {
    use crate::primal_identity::{Capability, CryptoCapability};

    let config = DiscoveryConfig {
        timeout_secs: 0,
        methods: vec!["env".into()],
        ..DiscoveryConfig::default()
    };
    let discovery = CapabilityDiscovery::with_config_async(&config)
        .await
        .expect("discovery");
    let result = discovery
        .find_by_capability(Capability::Crypto(CryptoCapability::Encryption))
        .await;

    // In test env with no services, we expect NoServicesFound, Timeout, DiscoveryFailed, or InvalidConfig
    match &result {
        Err(
            DiscoveryError::NoServicesFound(_)
            | DiscoveryError::Timeout
            | DiscoveryError::DiscoveryFailed(_)
            | DiscoveryError::InvalidConfig(_),
        ) => {}
        Ok(services) => assert!(
            services.is_empty(),
            "expected no services in test env, got {}",
            services.len()
        ),
    }
}

#[tokio::test]
async fn test_find_by_capability_with_localhost_fallback() {
    use crate::primal_identity::{Capability, CryptoCapability};

    let dir = tempfile::tempdir().expect("tempdir");
    temp_env::async_with_vars(
        [
            (
                socket_env::XDG_RUNTIME_DIR,
                Some(dir.path().to_str().expect("utf-8")),
            ),
            ("TOADSTOOL_LOCAL_PORT", None::<&str>),
            (socket_env::BIOMEOS_CRYPTO_SOCKET, None::<&str>),
            (socket_env::TOADSTOOL_SECURITY_SOCKET, None::<&str>),
            (socket_env::LEGACY_BEARDOG_SOCKET_ENV, None::<&str>),
            (socket_env::TOADSTOOL_SECURITY_ENDPOINT, None::<&str>),
            (socket_env::SECURITY_URL, None::<&str>),
            (socket_env::SECURITY_ENDPOINT, None::<&str>),
            (socket_env::LEGACY_BEARDOG_URL, None::<&str>),
            (socket_env::LEGACY_BEARDOG_ENDPOINT, None::<&str>),
            (socket_env::TOADSTOOL_SECURITY_URL, None::<&str>),
            ("SECURITY_PROVIDER_SOCKET", None::<&str>),
            ("CRYPTO_PROVIDER_SOCKET", None::<&str>),
            ("SECURITY_SOCKET", None::<&str>),
        ],
        async {
            let config = DiscoveryConfig {
                timeout_secs: 0,
                methods: vec!["env".into()],
                fallbacks: std::collections::HashMap::from([("localhost".into(), "127.0.0.1:8080".into())]),
                ..DiscoveryConfig::default()
            };
            let discovery = CapabilityDiscovery::with_config_async(&config)
                .await
                .expect("discovery");
            let result = discovery
                .find_by_capability(Capability::Crypto(CryptoCapability::Encryption))
                .await;

            // With fallback enabled, empty discovery returns Ok(vec![]) from try_localhost_fallback
            match &result {
                Ok(services) => assert!(services.is_empty()),
                Err(e) => assert!(
                    matches!(
                        e,
                        DiscoveryError::NoServicesFound(_)
                            | DiscoveryError::Timeout
                            | DiscoveryError::DiscoveryFailed(_)
                    ),
                    "unexpected error: {e}"
                ),
            }
        },
    )
    .await;
}

#[test]
fn test_discovery_error_display_all_variants() {
    let timeout_err = DiscoveryError::Timeout;
    assert_eq!(timeout_err.to_string(), "Discovery timeout");

    let no_services = DiscoveryError::NoServicesFound("Capability::Crypto(Encryption)".to_string());
    assert!(no_services.to_string().contains("Crypto"));
    assert!(no_services.to_string().contains("Encryption"));

    let failed = DiscoveryError::DiscoveryFailed("network down".to_string());
    assert!(failed.to_string().contains("network down"));

    let invalid = DiscoveryError::InvalidConfig("bad".to_string());
    assert!(invalid.to_string().contains("bad"));
}

#[test]
fn test_discovery_error_is_std_error() {
    use std::error::Error;
    let err = DiscoveryError::Timeout;
    assert!(err.source().is_none());
    let _ = format!("{err:?}");
}

#[test]
fn test_discovery_method_derive_clone() {
    let m = DiscoveryMethod::Mdns;
    let m2 = m;
    assert!(matches!(m2, DiscoveryMethod::Mdns));
}

// ═══════════════════════════════════════════════════════════════════
// Additional tests for capability discovery logic and error paths
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_find_best_empty_services_returns_error() {
    use crate::primal_identity::{Capability, CryptoCapability};

    let config = DiscoveryConfig {
        timeout_secs: 0,
        methods: vec!["env".into()],
        ..DiscoveryConfig::default()
    };
    let discovery = CapabilityDiscovery::with_config_async(&config)
        .await
        .expect("discovery");
    let result = discovery
        .find_best(Capability::Crypto(CryptoCapability::Encryption))
        .await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(
            err,
            DiscoveryError::NoServicesFound(_)
                | DiscoveryError::Timeout
                | DiscoveryError::DiscoveryFailed(_)
        ),
        "unexpected error: {err:?}"
    );
}

#[test]
fn test_discovery_config_default_methods() {
    let config = DiscoveryConfig::default();
    assert!(!config.methods.is_empty());
    assert!(config.methods.contains(&"mdns".to_string()));
}

#[test]
fn test_discovery_error_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<DiscoveryError>();
}

#[test]
fn test_discovery_config_debug() {
    let config = DiscoveryConfig::default();
    let debug_str = format!("{config:?}");
    assert!(debug_str.contains("DiscoveryConfig"));
}

#[test]
fn test_discovery_method_debug() {
    let m = DiscoveryMethod::Auto;
    let debug_str = format!("{m:?}");
    assert!(!debug_str.is_empty());
}

#[tokio::test]
async fn test_capability_discovery_new_async_creates_valid_instance() {
    let discovery = CapabilityDiscovery::new_async().await.expect("discovery");
    assert!(std::mem::size_of_val(&discovery) > 0);
}

#[tokio::test]
async fn test_capability_discovery_with_config_async_succeeds() {
    let config = DiscoveryConfig {
        timeout_secs: 30,
        fallbacks: std::collections::HashMap::from([("localhost".into(), "127.0.0.1:8080".into())]),
        ..DiscoveryConfig::default()
    };
    let discovery = CapabilityDiscovery::with_config_async(&config).await;
    assert!(discovery.is_ok());
}

#[test]
fn test_try_localhost_fallback_returns_empty() {
    use crate::primal_identity::{Capability, CryptoCapability};

    let dir = tempfile::tempdir().expect("tempdir");
    with_clean_localhost_fallback_env(dir.path(), || {
        let fallback = CapabilityDiscovery::try_localhost_fallback(&Capability::Crypto(
            CryptoCapability::Encryption,
        ));
        assert!(fallback.is_empty());
    });
}
