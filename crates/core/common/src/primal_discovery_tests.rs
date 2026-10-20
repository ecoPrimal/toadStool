// SPDX-License-Identifier: AGPL-3.0-or-later

use super::*;

#[tokio::test]
async fn test_discovery_with_fallback() {
    let config = DiscoveryConfig {
        methods: vec!["env".into()],
        fallbacks: {
            let mut m = std::collections::HashMap::new();
            m.insert("orchestration".to_string(), "http://localhost:8080".to_string());
            m
        },
        ..Default::default()
    };

    let discovery = PrimalDiscovery::with_config(config).unwrap();
    let endpoint = discovery.find_capability("orchestration").await.unwrap();

    assert_eq!(endpoint.url(), "http://localhost:8080");
    assert_eq!(endpoint.discovered_via, DiscoveryMethod::Configuration);
    assert!(endpoint.has_capability("orchestration"));
}

#[tokio::test]
async fn test_discovery_not_found() {
    let config = DiscoveryConfig {
        methods: vec!["env".into()],
        ..Default::default()
    };

    let discovery = PrimalDiscovery::with_config(config).unwrap();
    let result = discovery.find_capability("nonexistent").await;

    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        DiscoveryError::NotFound { .. }
    ));
}

#[tokio::test]
async fn test_cache_freshness() {
    let endpoint = PrimalEndpoint {
        service_id: "test".to_string(),
        capabilities: vec!["test".to_string()],
        url: "http://localhost:8080".to_string(),
        trust_level: TrustLevel::Local,
        discovered_via: DiscoveryMethod::Configuration,
        discovered_at: Instant::now(),
        last_seen: Instant::now(),
        latency_ms: 0,
    };

    assert!(endpoint.is_fresh(Duration::from_secs(10)));

    // Simulate old endpoint
    let mut old_endpoint = endpoint;
    old_endpoint.last_seen = Instant::now()
        .checked_sub(Duration::from_secs(100))
        .unwrap();
    assert!(!old_endpoint.is_fresh(Duration::from_secs(50)));
}

#[tokio::test]
async fn test_refresh_clears_cache() {
    let config = DiscoveryConfig {
        methods: vec!["env".into()],
        fallbacks: {
            let mut m = std::collections::HashMap::new();
            m.insert("test".to_string(), "http://localhost:8080".to_string());
            m
        },
        ..Default::default()
    };

    let discovery = PrimalDiscovery::with_config(config).unwrap();

    // Populate cache
    let _endpoint = discovery.find_capability("test").await.unwrap();

    // Refresh
    discovery.refresh().await.unwrap();

    // Cache should be cleared (need to refetch)
    let cache = discovery.cache.read().unwrap();
    assert!(cache.is_empty());
}

/// Test: Multiple capabilities per endpoint
#[tokio::test]
async fn test_multi_capability_endpoint() {
    let endpoint = PrimalEndpoint {
        service_id: "multi-service".to_string(),
        capabilities: vec![
            "security".to_string(),
            "storage".to_string(),
            "compute".to_string(),
        ],
        url: "http://localhost:8000".to_string(),
        trust_level: TrustLevel::Local,
        discovered_via: DiscoveryMethod::Configuration,
        discovered_at: Instant::now(),
        last_seen: Instant::now(),
        latency_ms: 5,
    };

    assert!(endpoint.has_capability("security"));
    assert!(endpoint.has_capability("storage"));
    assert!(endpoint.has_capability("compute"));
    assert!(!endpoint.has_capability("nonexistent"));
}

/// Test: Stale endpoint detection
#[tokio::test]
async fn test_stale_endpoint_filtering() {
    let fresh = PrimalEndpoint {
        service_id: "fresh".to_string(),
        capabilities: vec!["test".to_string()],
        url: "http://fresh:8000".to_string(),
        trust_level: TrustLevel::Local,
        discovered_via: DiscoveryMethod::MDns,
        discovered_at: Instant::now(),
        last_seen: Instant::now(),
        latency_ms: 5,
    };

    let stale = PrimalEndpoint {
        service_id: "stale".to_string(),
        capabilities: vec!["test".to_string()],
        url: "http://stale:8000".to_string(),
        trust_level: TrustLevel::Local,
        discovered_via: DiscoveryMethod::MDns,
        discovered_at: Instant::now()
            .checked_sub(Duration::from_secs(1000))
            .unwrap(),
        last_seen: Instant::now()
            .checked_sub(Duration::from_secs(1000))
            .unwrap(),
        latency_ms: 5,
    };

    let ttl = Duration::from_mins(5); // 5 minutes
    assert!(fresh.is_fresh(ttl));
    assert!(!stale.is_fresh(ttl));
}
