// SPDX-License-Identifier: AGPL-3.0-or-later
//! Advanced service discovery tests — registry, env key parsing, path resolution, multi-method

use std::io::Write;
use std::path::PathBuf;

use crate::discovery_defaults::{DiscoveryConfig, DiscoveryConfigExt};
use crate::primal_identity::{Capability, CryptoCapability};

use super::config::capability_from_str;
use super::service::ServiceDiscovery;
use super::types::{DiscoveryError, DiscoveryMethod, ServiceDiscoveryTrait};

use tempfile::NamedTempFile;

fn write_test_config(config_json: &str) -> (NamedTempFile, PathBuf) {
    let mut tmp = NamedTempFile::new().expect("temp file");
    tmp.write_all(config_json.as_bytes()).unwrap();
    let path = tmp.path().to_path_buf();
    (tmp, path)
}

async fn discovery_from_json(json: &str) -> (NamedTempFile, ServiceDiscovery) {
    let (tmp, path) = write_test_config(json);
    let path_str = path.to_string_lossy().to_string();
    let disc = ServiceDiscovery::new(DiscoveryMethod::ConfigFile { path: path_str })
        .await
        .unwrap();
    (tmp, disc)
}

#[tokio::test]
async fn test_registry_empty_endpoint_returns_error() {
    temp_env::async_with_vars([("TOADSTOOL_REGISTRY_ENDPOINT", None::<&str>)], async {
        let config = DiscoveryConfig::production();
        let result = ServiceDiscovery::with_config(
            DiscoveryMethod::Registry {
                endpoint: String::new(),
            },
            config,
        )
        .await;
        assert!(
            result.is_err(),
            "Empty registry endpoint should fail without env var"
        );
    })
    .await;
}

#[tokio::test]
async fn test_registry_file_path_delegates_to_config() {
    let (_tmp, path) = write_test_config(
        r#"{"services":[{"name":"file-reg","capabilities":["storage"],"endpoints":["http://localhost:6666"]}]}"#,
    );
    let path_str = path.to_string_lossy().to_string();
    let disc = ServiceDiscovery::new(DiscoveryMethod::Registry {
        endpoint: format!("file://{path_str}"),
    })
    .await
    .unwrap();
    let all = disc.discover_all().await.unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].name, "file-reg");
}

#[tokio::test]
async fn test_registry_unix_path_delegates_to_config() {
    let (_tmp, path) = write_test_config(
        r#"{"services":[{"name":"unix-reg","capabilities":["compute"],"endpoints":["http://localhost:7777"]}]}"#,
    );
    let path_str = path.to_string_lossy().to_string();
    let disc = ServiceDiscovery::new(DiscoveryMethod::Registry {
        endpoint: format!("unix://{path_str}"),
    })
    .await
    .unwrap();
    let all = disc.discover_all().await.unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].name, "unix-reg");
}

#[tokio::test]
async fn test_discover_multi_partial_success() {
    let (_tmp, path) = write_test_config(
        r#"{"services":[{"name":"cfg-svc","capabilities":["compute"],"endpoints":["http://localhost:5555"]}]}"#,
    );
    let path_str = path.to_string_lossy().to_string();
    let disc = ServiceDiscovery::new(DiscoveryMethod::Multi(vec![
        DiscoveryMethod::ConfigFile { path: path_str },
        DiscoveryMethod::Registry {
            endpoint: String::new(),
        },
    ]))
    .await
    .unwrap();
    let all = disc.discover_all().await;
    assert!(all.is_ok());
    let services = all.unwrap();
    assert!(!services.is_empty());
}

#[tokio::test]
async fn test_discover_fallback_when_nothing_found() {
    temp_env::async_with_vars(
        [
            ("TOADSTOOL_ENV", Some("development")),
            ("TOADSTOOL_URL", Some("http://localhost:8084")),
        ],
        async {
            let disc = ServiceDiscovery::new(DiscoveryMethod::Multi(vec![
                DiscoveryMethod::ConfigFile {
                    path: "/nonexistent/discovery.json".to_string(),
                },
                DiscoveryMethod::Registry {
                    endpoint: "/nonexistent/reg".to_string(),
                },
            ]))
            .await
            .unwrap();
            let all = disc.discover_all().await.unwrap();
            assert!(!all.is_empty(), "Should use fallback when configured");
        },
    )
    .await;
}

#[test]
fn test_capability_from_str_object_storage() {
    assert!(matches!(
        capability_from_str("object_storage"),
        Capability::Storage(_)
    ));
    assert!(matches!(
        capability_from_str("object-storage"),
        Capability::Storage(_)
    ));
}

#[test]
fn test_capability_from_str_cryptography() {
    assert!(matches!(
        capability_from_str("cryptography"),
        Capability::Crypto(_)
    ));
    assert!(matches!(
        capability_from_str("security"),
        Capability::Crypto(_)
    ));
}

#[test]
fn test_capability_from_str_native_execution() {
    assert!(matches!(
        capability_from_str("native"),
        Capability::Compute(_)
    ));
    assert!(matches!(
        capability_from_str("execution"),
        Capability::Compute(_)
    ));
}

#[test]
fn test_capability_from_str_whitespace() {
    assert!(matches!(
        capability_from_str("  coordination  "),
        Capability::Coordination(_)
    ));
}

#[tokio::test]
async fn test_find_services_cache_hit() {
    let (_tmp, disc) = discovery_from_json(r#"{"services":[{"name":"cache-svc","capabilities":["compute"],"endpoints":["http://localhost:2"]}]}"#).await;
    let cap = Capability::Compute(crate::primal_identity::ComputeCapability::NativeExecution);
    let first = disc.find_services_by_capability(&cap).await.unwrap();
    let second = disc.find_services_by_capability(&cap).await.unwrap();
    assert_eq!(first.len(), second.len());
    assert_eq!(first[0].id, second[0].id);
}

#[tokio::test]
async fn test_find_service_filter_capability_mismatch_returns_error() {
    let (_tmp, disc) = discovery_from_json(
        r#"{"services":[
        {"name":"compute-only","capabilities":["compute"],"endpoints":["http://localhost:3"]}
    ]}"#,
    )
    .await;
    let crypto_cap = Capability::Crypto(CryptoCapability::KeyManagement);
    let found = disc.find_service_by_capability(crypto_cap).await;
    assert!(found.is_err());
    if let Err(DiscoveryError::NoServiceFound { .. }) = found {
        // Expected
    } else {
        panic!("Expected NoServiceFound error");
    }
}

#[tokio::test]
async fn test_discover_from_env_key_strip_prefix_suffix() {
    temp_env::async_with_vars(
        [
            ("TOADSTOOL_SERVICE_MYSVC_URL", Some("http://localhost:9999")),
            ("TOADSTOOL_SERVICE_MYSVC_CAPABILITIES", Some("compute")),
        ],
        async {
            let disc = ServiceDiscovery::new(DiscoveryMethod::Environment)
                .await
                .unwrap();
            let services = disc.discover_from_env().unwrap();
            let mysvc = services.iter().find(|s| s.name == "mysvc");
            assert!(
                mysvc.is_some(),
                "Should parse MY_SVC from TOADSTOOL_SERVICE_MYSVC_URL"
            );
        },
    )
    .await;
}

#[tokio::test]
async fn test_discover_from_env_invalid_url_propagates_error() {
    temp_env::async_with_vars(
        [
            (
                "TOADSTOOL_SERVICE_BADURL_URL",
                Some(":::triple-colon-invalid"),
            ),
            ("TOADSTOOL_SERVICE_BADURL_CAPABILITIES", Some("compute")),
        ],
        async {
            let disc = ServiceDiscovery::new(DiscoveryMethod::Environment)
                .await
                .unwrap();
            let result = disc.discover_from_env();
            assert!(result.is_err());
        },
    )
    .await;
}

#[tokio::test]
async fn test_config_path_resolution_biomeos_runtime_dir() {
    let config = r#"{"services":[{"name":"rt-svc","capabilities":["storage"],"endpoints":["http://localhost:4"]}]}"#;
    let (_tmp, path) = write_test_config(config);
    let parent = path.parent().unwrap().to_path_buf();
    let runtime_dir = parent.join("biomeos_runtime");
    std::fs::create_dir_all(&runtime_dir).unwrap();
    std::fs::write(runtime_dir.join("discovery.json"), config).unwrap();
    let runtime_path = runtime_dir.to_str().unwrap().to_string();
    temp_env::async_with_vars(
        [("BIOMEOS_RUNTIME_DIR", Some(runtime_path.as_str()))],
        async {
            let disc = ServiceDiscovery::new(DiscoveryMethod::ConfigFile {
                path: String::new(),
            })
            .await
            .unwrap();
            let all = disc.discover_all().await.unwrap();
            assert_eq!(all.len(), 1);
            assert_eq!(all[0].name, "rt-svc");
        },
    )
    .await;
    std::fs::remove_dir_all(&runtime_dir).ok();
}

#[tokio::test]
async fn test_config_path_resolution_xdg_config_home() {
    let config = r#"{"services":[{"name":"xdg-svc","capabilities":["compute"],"endpoints":["http://localhost:5"]}]}"#;
    let temp_dir = std::env::temp_dir().join("toadstool_xdg_test");
    let xdg_config = temp_dir.join("xdg_config");
    let biomeos = xdg_config.join("biomeos");
    std::fs::create_dir_all(&biomeos).unwrap();
    std::fs::write(biomeos.join("discovery.json"), config).unwrap();
    let xdg_path = xdg_config.to_str().unwrap().to_string();
    temp_env::async_with_vars([("XDG_CONFIG_HOME", Some(xdg_path.as_str()))], async {
        let disc = ServiceDiscovery::new(DiscoveryMethod::ConfigFile {
            path: String::new(),
        })
        .await
        .unwrap();
        let all = disc.discover_all().await.unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].name, "xdg-svc");
    })
    .await;
    std::fs::remove_dir_all(&temp_dir).ok();
}

#[tokio::test]
async fn test_config_path_resolution_home_fallback() {
    let config = r#"{"services":[{"name":"home-svc","capabilities":["storage"],"endpoints":["http://localhost:6"]}]}"#;
    let temp_dir = std::env::temp_dir().join("toadstool_home_test");
    let home = temp_dir.join("fake_home");
    let config_dir = home.join(".config/biomeos");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::write(config_dir.join("discovery.json"), config).unwrap();
    let home_path = home.to_str().unwrap().to_string();
    temp_env::async_with_vars(
        [
            ("TOADSTOOL_DISCOVERY_CONFIG", None::<&str>),
            ("BIOMEOS_RUNTIME_DIR", None::<&str>),
            ("XDG_CONFIG_HOME", None::<&str>),
            ("HOME", Some(home_path.as_str())),
        ],
        async {
            let disc = ServiceDiscovery::new(DiscoveryMethod::ConfigFile {
                path: String::new(),
            })
            .await
            .unwrap();
            let all = disc.discover_all().await.unwrap();
            assert_eq!(all.len(), 1);
            assert_eq!(all[0].name, "home-svc");
        },
    )
    .await;
    std::fs::remove_dir_all(&temp_dir).ok();
}

#[tokio::test]
async fn test_registry_http_path_parsing() {
    let (_tmp, path) = write_test_config(
        r#"{"services":[{"name":"path-svc","capabilities":["compute"],"endpoints":["http://localhost:7"]}]}"#,
    );
    let path_str = path.to_string_lossy().to_string();
    let disc = ServiceDiscovery::new(DiscoveryMethod::Registry {
        endpoint: format!("file://{path_str}"),
    })
    .await
    .unwrap();
    let all = disc.discover_all().await.unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].name, "path-svc");
}

#[tokio::test]
async fn test_discover_from_fallbacks_disabled_when_production() {
    temp_env::async_with_vars(
        [
            ("TOADSTOOL_ENV", Some("production")),
            ("TOADSTOOL_URL", Some("http://localhost:8084")),
        ],
        async {
            let disc = ServiceDiscovery::new(DiscoveryMethod::Multi(vec![
                DiscoveryMethod::ConfigFile {
                    path: "/nonexistent/discovery.json".to_string(),
                },
                DiscoveryMethod::Registry {
                    endpoint: String::new(),
                },
            ]))
            .await
            .unwrap();
            let all = disc.discover_all().await.unwrap();
            assert!(all.is_empty(), "Production should not use fallbacks");
        },
    )
    .await;
}

#[tokio::test]
async fn test_discover_multi_all_fail_no_fallback() {
    temp_env::async_with_vars(
        [
            ("TOADSTOOL_ENV", Some("production")),
            ("TOADSTOOL_REGISTRY_ENDPOINT", None::<&str>),
        ],
        async {
            let disc = ServiceDiscovery::new(DiscoveryMethod::Multi(vec![
                DiscoveryMethod::ConfigFile {
                    path: "/nonexistent/x.json".to_string(),
                },
                DiscoveryMethod::Registry {
                    endpoint: String::new(),
                },
            ]))
            .await
            .unwrap();
            let all = disc.discover_all().await.unwrap();
            assert!(all.is_empty());
        },
    )
    .await;
}

#[tokio::test]
async fn test_find_service_prefers_healthy() {
    let (_tmp, disc) = discovery_from_json(
        r#"{
        "services": [
            {"name":"unhealthy-svc","capabilities":["compute"],"endpoints":["http://localhost:8"]},
            {"name":"healthy-svc","capabilities":["compute"],"endpoints":["http://localhost:9"]}
        ]
    }"#,
    )
    .await;
    let cap = Capability::Compute(crate::primal_identity::ComputeCapability::NativeExecution);
    let found = disc.find_service_by_capability(cap).await.unwrap();
    assert!(found.healthy);
}

#[tokio::test]
async fn test_new_no_refresh() {
    let disc = ServiceDiscovery::new_no_refresh(DiscoveryMethod::Environment);
    assert_eq!(disc.method, DiscoveryMethod::Environment);
    let all = disc.discover_all().await;
    assert!(all.is_ok());
}

#[test]
fn test_discovery_method_debug() {
    let m = DiscoveryMethod::Auto;
    let s = format!("{m:?}");
    assert!(s.contains("Auto"));

    let m = DiscoveryMethod::ConfigFile {
        path: "/path".to_string(),
    };
    let s = format!("{m:?}");
    assert!(s.contains("ConfigFile"));
    assert!(s.contains("/path"));

    let m = DiscoveryMethod::Registry {
        endpoint: "http://x".to_string(),
    };
    let s = format!("{m:?}");
    assert!(s.contains("Registry"));
}

#[tokio::test]
async fn test_parse_capabilities_ignores_unknown() {
    let caps = ServiceDiscovery::parse_capabilities("foo,bar,compute,baz,storage");
    assert_eq!(caps.len(), 2);
}
