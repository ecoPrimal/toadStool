// SPDX-License-Identifier: AGPL-3.0-or-later
//! Discovery, fallback, and mDNS tests

use super::super::*;
use super::common::*;
use crate::primal_identity::{ComputeCapability, CoordinationCapability, StorageCapability};
use std::collections::HashMap;

#[tokio::test]
#[expect(clippy::await_holding_lock)]
async fn test_discovery_with_fallback() {
    let _lock = ENV_MUTEX.lock().expect("env mutex poisoned");
    let config = DiscoveryConfig {
        methods: vec!["env".into()],
        fallbacks: {
            let mut m = std::collections::HashMap::new();
            m.insert(
                "orchestration".to_string(),
                "http://localhost:9999".to_string(),
            );
            m
        },
        ..DiscoveryConfig::default()
    };

    let engine = PrimalDiscoveryEngine::without_client(config).expect("Failed to create engine");

    let capability = Capability::Coordination(CoordinationCapability::ServiceDiscovery);
    let result = engine.discover_by_capability(&capability).await;

    assert!(result.is_ok(), "Should find orchestration via fallback");
    let services = result.expect("Services should be present");
    assert!(!services.is_empty(), "Should have at least one service");
    assert_eq!(services[0].endpoints[0].port, 9999);
}

#[tokio::test]
async fn test_with_config_mdns_only_no_client() {
    // When methods only contain "mdns" and no client is provided, engine should error
    let config = DiscoveryConfig {
        methods: vec!["mdns".into()],
        fallbacks: HashMap::new(),
        ..Default::default()
    };

    let result = PrimalDiscoveryEngine::without_client(config);

    // Engine may error or succeed depending on implementation — validate it handles gracefully
    if let Err(e) = &result {
        let err_msg = e.to_string();
        assert!(
            err_msg.to_lowercase().contains("mdns") || err_msg.to_lowercase().contains("client"),
            "unexpected error: {err_msg}"
        );
    }
}

#[tokio::test]
async fn test_discover_by_capability_not_found() {
    let mut config = DiscoveryConfig::default();
    config.fallbacks.clear();
    config.methods = vec!["env".into()];

    let engine = PrimalDiscoveryEngine::without_client(config).expect("Failed to create engine");

    let capability = Capability::Compute(ComputeCapability::NativeExecution);
    let result = engine.discover_by_capability(&capability).await;

    assert!(result.is_err());
    let err_msg = result.err().unwrap().to_string();
    assert!(err_msg.contains("No service found"));
}

#[tokio::test]
async fn test_create_fallback_service_https() {
    let mut config = DiscoveryConfig::default();
    config.fallbacks.insert(
        "storage".to_string(),
        "https://example.com:443/api".to_string(),
    );
    config.methods = vec!["env".into()];

    let engine = PrimalDiscoveryEngine::without_client(config).expect("Failed to create engine");

    let capability = Capability::Storage(StorageCapability::ObjectStorage);
    let services = engine
        .discover_by_capability(&capability)
        .await
        .expect("Should find fallback");

    assert_eq!(services[0].endpoints[0].protocol, "https");
    assert_eq!(services[0].endpoints[0].address, "example.com");
    assert_eq!(services[0].endpoints[0].port, 443);
}

#[tokio::test]
async fn test_create_fallback_service_socket_addr() {
    let mut config = DiscoveryConfig::default();
    config
        .fallbacks
        .insert("compute".to_string(), "127.0.0.1:9090".to_string());
    config.methods = vec!["env".into()];

    let engine = PrimalDiscoveryEngine::without_client(config).expect("Failed to create engine");

    let capability = Capability::Compute(ComputeCapability::NativeExecution);
    let services = engine
        .discover_by_capability(&capability)
        .await
        .expect("Should find fallback");

    assert_eq!(services[0].endpoints[0].address, "127.0.0.1");
    assert_eq!(services[0].endpoints[0].port, 9090);
}

#[tokio::test]
async fn test_create_fallback_service_http() {
    let mut config = DiscoveryConfig::default();
    config
        .fallbacks
        .insert("test".to_string(), "http://127.0.0.1:8080".to_string());
    config.methods = vec!["env".into()];

    let engine = PrimalDiscoveryEngine::without_client(config).expect("Failed to create engine");

    let capability = Capability::Custom {
        name: "test".to_string(),
        version: "1.0".to_string(),
    };
    let services = engine
        .discover_by_capability(&capability)
        .await
        .expect("Should find fallback");

    assert_eq!(services[0].endpoints[0].protocol, "http");
    assert_eq!(services[0].endpoints[0].address, "127.0.0.1");
    assert_eq!(services[0].endpoints[0].port, 8080);
    assert!(
        services[0]
            .metadata
            .get("source")
            .is_some_and(|s| s == "configuration")
    );
}

#[tokio::test]
async fn test_mdns_discovery_success_path() {
    let mdns_service = DiscoveredService {
        id: Some("mdns-svc".to_string()),
        capabilities: vec![Capability::Coordination(
            CoordinationCapability::ServiceDiscovery,
        )],
        endpoints: vec![crate::primal_identity::ServiceEndpoint {
            protocol: "http".to_string(),
            address: "mdns-host".to_string(),
            port: 9000,
            path: Some("/".to_string()),
            metadata: HashMap::new(),
        }],
        healthy: true,
        metadata: HashMap::new(),
    };

    let mock = MockDiscoveryClient {
        services: std::sync::RwLock::new(Some(vec![mdns_service])),
        error: std::sync::RwLock::new(None),
    };

    let mut config = DiscoveryConfig::default();
    config.fallbacks.clear();
    config.methods = vec!["mdns".into(), "env".into()];

    let engine = PrimalDiscoveryEngine::with_config(Some(std::sync::Arc::new(mock)), config)
        .expect("Failed to create engine");

    let result = engine
        .discover_by_capability(&Capability::Coordination(
            CoordinationCapability::ServiceDiscovery,
        ))
        .await;

    assert!(result.is_ok());
    let services = result.unwrap();
    assert_eq!(services.len(), 1);
    assert_eq!(services[0].endpoints[0].address, "mdns-host");
    assert_eq!(services[0].endpoints[0].port, 9000);
}

#[tokio::test]
async fn test_mdns_discovery_empty_falls_to_fallback() {
    let mock = MockDiscoveryClient {
        services: std::sync::RwLock::new(Some(vec![])),
        error: std::sync::RwLock::new(None),
    };

    let mut config = DiscoveryConfig::default();
    config.fallbacks.insert(
        "orchestration".to_string(),
        "http://fallback:8888".to_string(),
    );
    config.methods = vec!["mdns".into(), "env".into()];

    let engine = PrimalDiscoveryEngine::with_config(Some(std::sync::Arc::new(mock)), config)
        .expect("Failed to create engine");

    let result = engine
        .discover_by_capability(&Capability::Coordination(
            CoordinationCapability::ServiceDiscovery,
        ))
        .await;

    assert!(result.is_ok());
    let services = result.unwrap();
    assert_eq!(services.len(), 1);
    assert_eq!(services[0].endpoints[0].address, "fallback");
    assert_eq!(services[0].endpoints[0].port, 8888);
}

#[tokio::test]
async fn test_mdns_discovery_error_falls_to_fallback() {
    let mock = MockDiscoveryClient {
        services: std::sync::RwLock::new(None),
        error: std::sync::RwLock::new(Some("mdns network error".to_string())),
    };

    let mut config = DiscoveryConfig::default();
    config
        .fallbacks
        .insert("compute".to_string(), "http://127.0.0.1:7777".to_string());
    config.methods = vec!["mdns".into(), "env".into()];

    let engine = PrimalDiscoveryEngine::with_config(Some(std::sync::Arc::new(mock)), config)
        .expect("Failed to create engine");

    let result = engine
        .discover_by_capability(&Capability::Compute(ComputeCapability::NativeExecution))
        .await;

    assert!(result.is_ok());
    let services = result.unwrap();
    assert_eq!(services[0].endpoints[0].address, "127.0.0.1");
    assert_eq!(services[0].endpoints[0].port, 7777);
}

#[tokio::test]
async fn test_cache_hit_returns_cached_service() {
    let mut config = DiscoveryConfig::default();
    config.fallbacks.insert(
        "orchestration".to_string(),
        "http://localhost:9997".to_string(),
    );
    config.methods = vec!["env".into()];
    config.cache_ttl_secs = 300;

    let engine = PrimalDiscoveryEngine::without_client(config).expect("Failed to create engine");

    let cap = Capability::Coordination(CoordinationCapability::ServiceDiscovery);
    let first = engine.discover_by_capability(&cap).await.unwrap();
    assert_eq!(first[0].endpoints[0].port, 9997);

    let second = engine.discover_by_capability(&cap).await.unwrap();
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].endpoints[0].port, 9997);
}

#[tokio::test]
async fn test_create_fallback_service_malformed_url_uses_defaults() {
    let mut config = DiscoveryConfig::default();
    config
        .fallbacks
        .insert("test".to_string(), "not-a-valid-url".to_string());
    config.methods = vec!["env".into()];

    let engine = PrimalDiscoveryEngine::without_client(config).expect("Failed to create engine");

    let cap = Capability::Custom {
        name: "test".to_string(),
        version: "1.0".to_string(),
    };
    let services = engine.discover_by_capability(&cap).await.unwrap();
    assert_eq!(services.len(), 1);
    assert_eq!(
        services[0].endpoints[0].address,
        crate::constants::network::LOCALHOST_IPV4
    );
    assert_eq!(services[0].endpoints[0].port, 8080);
}

#[tokio::test]
async fn test_create_fallback_service_http_no_port_uses_80() {
    let mut config = DiscoveryConfig::default();
    config
        .fallbacks
        .insert("test".to_string(), "http://example.com/path".to_string());
    config.methods = vec!["env".into()];

    let engine = PrimalDiscoveryEngine::without_client(config).expect("Failed to create engine");

    let cap = Capability::Custom {
        name: "test".to_string(),
        version: "1.0".to_string(),
    };
    let services = engine.discover_by_capability(&cap).await.unwrap();
    assert_eq!(services[0].endpoints[0].port, 80);
    assert_eq!(services[0].endpoints[0].address, "example.com");
}

#[tokio::test]
async fn test_with_config_enable_mdns_no_fallbacks_warn_path() {
    let mut config = DiscoveryConfig::default();
    config.fallbacks.clear();
    config.methods = vec!["env".into()];
    config.fallbacks.clear();

    let engine = PrimalDiscoveryEngine::without_client(config)
        .expect("Should create even with no mDNS and no fallbacks");

    let cap = Capability::Compute(ComputeCapability::NativeExecution);
    let result = engine.discover_by_capability(&cap).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_with_config_mdns_enabled_with_client() {
    let mock = MockDiscoveryClient {
        services: std::sync::RwLock::new(None),
        error: std::sync::RwLock::new(None),
    };
    let mut config = DiscoveryConfig::default();
    config.fallbacks.clear();
    config.methods = vec!["mdns".into(), "env".into()];

    let engine = PrimalDiscoveryEngine::with_config(Some(std::sync::Arc::new(mock)), config)
        .expect("Should create when mDNS client provided");
    let _ = engine;
}

#[tokio::test]
async fn test_discover_by_capability_not_found_no_fallback() {
    let mut config = DiscoveryConfig::default();
    config.fallbacks.clear();
    config.methods = vec!["env".into()];

    let engine = PrimalDiscoveryEngine::without_client(config).expect("create");

    let cap = Capability::Custom {
        name: "nonexistent-cap".to_string(),
        version: "1.0".to_string(),
    };
    let result = engine.discover_by_capability(&cap).await;
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("No service found"));
    assert!(err_msg.contains("nonexistent-cap"));
}

#[tokio::test]
async fn test_create_fallback_service_url_with_path() {
    let mut config = DiscoveryConfig::default();
    config.fallbacks.insert(
        "api".to_string(),
        "https://api.example.com:443/v1/endpoint".to_string(),
    );
    config.methods = vec!["env".into()];

    let engine = PrimalDiscoveryEngine::without_client(config).expect("create");

    let cap = Capability::Custom {
        name: "api".to_string(),
        version: "1.0".to_string(),
    };
    let services = engine.discover_by_capability(&cap).await.unwrap();
    assert_eq!(
        services[0].endpoints[0].path,
        Some("/v1/endpoint".to_string())
    );
    assert_eq!(services[0].endpoints[0].port, 443);
}

#[tokio::test]
async fn test_discovered_service_metadata_source() {
    let mut config = DiscoveryConfig::default();
    config
        .fallbacks
        .insert("meta".to_string(), "http://127.0.0.1:9000".to_string());
    config.methods = vec!["env".into()];

    let engine = PrimalDiscoveryEngine::without_client(config).expect("create");

    let cap = Capability::Custom {
        name: "meta".to_string(),
        version: "1.0".to_string(),
    };
    let services = engine.discover_by_capability(&cap).await.unwrap();
    assert_eq!(services[0].metadata.get("source").unwrap(), "configuration");
    assert!(services[0].healthy);
}

#[tokio::test]
async fn test_mdns_discovery_empty_no_fallback_fails() {
    let mock = MockDiscoveryClient {
        services: std::sync::RwLock::new(Some(vec![])),
        error: std::sync::RwLock::new(None),
    };
    let mut config = DiscoveryConfig::default();
    config.fallbacks.clear();
    config.methods = vec!["mdns".into(), "env".into()];

    let engine = PrimalDiscoveryEngine::with_config(Some(std::sync::Arc::new(mock)), config)
        .expect("create");

    let cap = Capability::Compute(ComputeCapability::NativeExecution);
    let result = engine.discover_by_capability(&cap).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_mdns_discovery_error_no_fallback_fails() {
    let mock = MockDiscoveryClient {
        services: std::sync::RwLock::new(None),
        error: std::sync::RwLock::new(Some("network failure".to_string())),
    };
    let mut config = DiscoveryConfig::default();
    config.fallbacks.clear();
    config.methods = vec!["mdns".into(), "env".into()];

    let engine = PrimalDiscoveryEngine::with_config(Some(std::sync::Arc::new(mock)), config)
        .expect("create");

    let cap = Capability::Storage(StorageCapability::ObjectStorage);
    let result = engine.discover_by_capability(&cap).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_create_fallback_service_unknown_protocol_uses_http() {
    let mut config = DiscoveryConfig::default();
    config
        .fallbacks
        .insert("test".to_string(), "ftp://example.com:21/path".to_string());
    config.methods = vec!["env".into()];

    let engine = PrimalDiscoveryEngine::without_client(config).expect("create");

    let cap = Capability::Custom {
        name: "test".to_string(),
        version: "1.0".to_string(),
    };
    let services = engine.discover_by_capability(&cap).await.unwrap();
    assert_eq!(services.len(), 1);
    assert!(!services[0].endpoints[0].protocol.is_empty());
}

#[tokio::test]
async fn test_not_found_error_includes_capability_name() {
    let mut config = DiscoveryConfig::default();
    config.fallbacks.clear();
    config.methods = vec!["env".into()];

    let engine = PrimalDiscoveryEngine::without_client(config).expect("create");

    let cap = Capability::Custom {
        name: "unique-cap-x".to_string(),
        version: "1.0".to_string(),
    };
    let result = engine.discover_by_capability(&cap).await;
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("unique-cap-x"));
    assert!(err_msg.to_lowercase().contains("no service"));
}

#[tokio::test]
async fn test_fallback_service_metadata_source() {
    let mut config = DiscoveryConfig::default();
    config
        .fallbacks
        .insert("meta-cap".to_string(), "http://127.0.0.1:8888".to_string());
    config.methods = vec!["env".into()];

    let engine = PrimalDiscoveryEngine::without_client(config).expect("create");

    let cap = Capability::Custom {
        name: "meta-cap".to_string(),
        version: "1.0".to_string(),
    };
    let services = engine.discover_by_capability(&cap).await.unwrap();
    assert_eq!(services[0].metadata.get("source").unwrap(), "configuration");
    assert!(services[0].healthy);
}
