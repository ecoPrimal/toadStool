// SPDX-License-Identifier: AGPL-3.0-or-later

use super::*;

#[tokio::test]
async fn test_mdns_adapter_creation() {
    let config = DiscoveryConfig::default();
    let result = MdnsAdapter::new(config);
    // mDNS may not be available in all test environments
    if let Err(e) = &result {
        eprintln!("mDNS adapter creation failed (expected in some environments): {e}");
    }
    // Don't assert success - mDNS requires network access
}

#[tokio::test]
async fn test_mdns_adapter_with_timeout() {
    let config = DiscoveryConfig::default();
    let timeout = Duration::from_millis(500);
    let result = MdnsAdapter::with_timeout(config, timeout);

    if let Ok(adapter) = result {
        assert_eq!(adapter.timeout(), timeout);
    }
}

#[tokio::test]
async fn test_mdns_adapter_discover_handles_no_services() {
    let config = DiscoveryConfig::default();
    // Use short timeout for test
    let result = MdnsAdapter::with_timeout(config, Duration::from_millis(100));

    if let Ok(adapter) = result {
        // Discovery should complete without error even if no services found
        let endpoints = adapter.discover("nonexistent-capability");
        if let Ok(eps) = endpoints {
            // In most test environments, no real services will be found
            eprintln!(
                "Found {} endpoints (expected 0 in test environment)",
                eps.len()
            );
        }
    }
}

#[tokio::test]
async fn test_mdns_adapter_discover_all() {
    let config = DiscoveryConfig::default();
    let result = MdnsAdapter::with_timeout(config, Duration::from_millis(100));

    if let Ok(adapter) = result {
        let endpoints = adapter.discover_all();
        if let Ok(eps) = endpoints {
            eprintln!("discover_all found {} endpoints", eps.len());
        }
    }
}

#[test]
fn test_toadstool_service_type_constant() {
    assert_eq!(TOADSTOOL_SERVICE_TYPE, "_toadstool._tcp.local.");
    assert!(TOADSTOOL_SERVICE_TYPE.ends_with(".local."));
}

#[test]
fn test_convert_mdns_service_to_endpoint() {
    let endpoint = convert_mdns_service_to_endpoint(
        "service-123".to_string(),
        vec!["storage".to_string(), "replication".to_string()],
        "http://192.168.1.100:8000".to_string(),
    );

    assert_eq!(endpoint.service_id, "service-123");
    assert_eq!(endpoint.capabilities.len(), 2);
    assert_eq!(endpoint.capabilities[0], "storage");
    assert_eq!(endpoint.capabilities[1], "replication");
    assert_eq!(endpoint.url, "http://192.168.1.100:8000");
    assert_eq!(endpoint.trust_level, TrustLevel::Local);
    assert_eq!(endpoint.discovered_via, DiscoveryMethod::MDns);
    assert_eq!(endpoint.latency_ms, 0);
}

#[test]
fn test_convert_mdns_service_empty_capabilities() {
    let endpoint = convert_mdns_service_to_endpoint(
        "service-456".to_string(),
        vec![],
        "http://localhost:9000".to_string(),
    );

    assert_eq!(endpoint.service_id, "service-456");
    assert_eq!(endpoint.capabilities.len(), 0);
    assert_eq!(endpoint.url, "http://localhost:9000");
}

#[test]
fn test_convert_mdns_service_single_capability() {
    let endpoint = convert_mdns_service_to_endpoint(
        "compute-1".to_string(),
        vec!["gpu-compute".to_string()],
        "http://10.0.0.5:7777".to_string(),
    );

    assert_eq!(endpoint.service_id, "compute-1");
    assert_eq!(endpoint.capabilities.len(), 1);
    assert_eq!(endpoint.capabilities[0], "gpu-compute");
}

#[test]
fn test_convert_mdns_service_trust_level_always_local() {
    // mDNS services should always be Local trust level
    let endpoint1 = convert_mdns_service_to_endpoint(
        "svc1".to_string(),
        vec!["test".to_string()],
        "http://192.168.0.1:8080".to_string(),
    );
    let endpoint2 = convert_mdns_service_to_endpoint(
        "svc2".to_string(),
        vec!["test".to_string()],
        "http://10.0.0.1:8080".to_string(),
    );

    assert_eq!(endpoint1.trust_level, TrustLevel::Local);
    assert_eq!(endpoint2.trust_level, TrustLevel::Local);
}

#[test]
fn test_convert_mdns_service_discovery_method_always_mdns() {
    let endpoint = convert_mdns_service_to_endpoint(
        "test-service".to_string(),
        vec!["capability".to_string()],
        "http://example.local:8080".to_string(),
    );

    assert_eq!(endpoint.discovered_via, DiscoveryMethod::MDns);
}

#[test]
fn test_default_discovery_timeout() {
    // Under cfg(test) the timeout is 50ms; in production it would be 3s.
    assert_eq!(DEFAULT_DISCOVERY_TIMEOUT, Duration::from_millis(50));
}

#[test]
fn test_convert_mdns_service_url_format() {
    let endpoint = convert_mdns_service_to_endpoint(
        "host.local".to_string(),
        vec!["compute".to_string()],
        "http://192.168.1.1:8080".to_string(),
    );
    assert!(endpoint.url.starts_with("http://"));
    assert!(endpoint.url.contains(":8080"));
}

#[test]
fn test_convert_mdns_service_latency_initial_zero() {
    let endpoint = convert_mdns_service_to_endpoint(
        "svc".to_string(),
        vec!["storage".to_string()],
        "http://10.0.0.1:9000".to_string(),
    );
    assert_eq!(endpoint.latency_ms, 0);
}

#[test]
fn test_mdns_adapter_config_accessor() {
    let config = DiscoveryConfig::default();
    let result = MdnsAdapter::with_timeout(config, Duration::from_millis(100));
    if let Ok(adapter) = result {
        let adapter_config = adapter.config();
        assert_eq!(adapter_config.cache_ttl_secs, 60);
        assert!(adapter_config.methods.contains(&"mdns".to_string()));
    }
}

#[test]
fn test_toadstool_service_type_format() {
    assert!(TOADSTOOL_SERVICE_TYPE.contains("toadstool"));
    assert!(TOADSTOOL_SERVICE_TYPE.contains("tcp"));
}
