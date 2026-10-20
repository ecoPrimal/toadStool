// SPDX-License-Identifier: AGPL-3.0-or-later
//! mDNS discovery backend

use std::collections::HashMap;
use std::time::SystemTime;

use tracing::info;

use crate::primal_identity::ServiceEndpoint;

use super::config::capability_from_str;
use super::types::{DiscoveredService, DiscoveryError, DiscoveryResult};

/// Convert a primal discovery endpoint to `DiscoveredService` (pure, testable).
pub fn primal_endpoint_to_discovered_service(
    service_id: &str,
    url: &str,
    capabilities: &[String],
    now: SystemTime,
) -> DiscoveredService {
    let caps: Vec<_> = capabilities
        .iter()
        .map(|s| capability_from_str(s))
        .collect();
    let endpoint = ServiceEndpoint::from_url_string(url).unwrap_or_else(|_| ServiceEndpoint {
        protocol: "http".to_string(),
        address: url.to_string(),
        port: 80,
        path: None,
        metadata: HashMap::new(),
    });
    DiscoveredService {
        id: service_id.to_string(),
        name: service_id.to_string(),
        version: "mdns".to_string(),
        capabilities: caps,
        endpoints: vec![endpoint],
        metadata: HashMap::new(),
        discovered_at: now,
        last_seen: now,
        healthy: true,
    }
}

/// Discover services via mDNS.
pub async fn discover_via_mdns() -> DiscoveryResult<Vec<DiscoveredService>> {
    use crate::primal_discovery::DiscoveryConfig as PrimalDiscoveryConfig;
    use crate::primal_discovery_mdns::MdnsAdapter;

    let mdns_config = PrimalDiscoveryConfig {
        methods: vec!["mdns".into()],
        ..Default::default()
    };

    // MdnsAdapter::discover_all() uses blocking recv_timeout internally;
    // run on the blocking thread pool to avoid starving the async executor.
    let endpoints = tokio::task::spawn_blocking(move || {
        MdnsAdapter::new(mdns_config)
            .map_err(|e| DiscoveryError::MethodUnavailable {
                method: format!("mDNS init failed: {e}"),
            })?
            .discover_all()
            .map_err(|e| DiscoveryError::MethodUnavailable {
                method: format!("mDNS browse failed: {e}"),
            })
    })
    .await
    .map_err(|e| DiscoveryError::MethodUnavailable {
        method: format!("spawn_blocking failed: {e}"),
    })??;

    let now = SystemTime::now();
    let services: Vec<DiscoveredService> = endpoints
        .into_iter()
        .map(|ep| {
            primal_endpoint_to_discovered_service(&ep.service_id, &ep.url, &ep.capabilities, now)
        })
        .collect();

    info!("mDNS discovery: found {} services", services.len());
    Ok(services)
}

#[cfg(test)]
mod tests {
    use std::time::SystemTime;

    use crate::primal_identity::{Capability, ComputeCapability, CoordinationCapability};

    use super::*;

    #[test]
    fn test_primal_endpoint_to_discovered_service_valid_url() {
        let now = SystemTime::now();
        let service = primal_endpoint_to_discovered_service(
            "service-1",
            "http://127.0.0.1:8080",
            &["compute".to_string(), "coordination".to_string()],
            now,
        );
        assert_eq!(service.id, "service-1");
        assert_eq!(service.name, "service-1");
        assert_eq!(service.version, "mdns");
        assert_eq!(service.capabilities.len(), 2);
        assert!(service.has_capability(&Capability::Compute(ComputeCapability::NativeExecution)));
        assert!(service.has_capability(&Capability::Coordination(
            CoordinationCapability::ServiceDiscovery
        )));
        assert_eq!(service.endpoints.len(), 1);
        assert_eq!(service.endpoints[0].protocol, "http");
        assert_eq!(service.endpoints[0].port, 8080);
    }

    #[test]
    fn test_primal_endpoint_to_discovered_service_invalid_url_fallback() {
        let now = SystemTime::now();
        let service = primal_endpoint_to_discovered_service(
            "service-2",
            "not-a-valid-url",
            &["storage".to_string()],
            now,
        );
        assert_eq!(service.id, "service-2");
        assert_eq!(service.endpoints.len(), 1);
        assert_eq!(service.endpoints[0].protocol, "http");
        assert_eq!(service.endpoints[0].address, "not-a-valid-url");
        assert_eq!(service.endpoints[0].port, 80);
    }

    #[test]
    fn test_primal_endpoint_to_discovered_service_empty_capabilities() {
        let now = SystemTime::now();
        let service =
            primal_endpoint_to_discovered_service("service-3", "http://192.168.1.1:9000", &[], now);
        assert!(service.capabilities.is_empty());
    }

    #[test]
    fn test_primal_endpoint_to_discovered_service_custom_capability() {
        let now = SystemTime::now();
        let service = primal_endpoint_to_discovered_service(
            "custom-svc",
            "https://example.com:443",
            &["custom-thing".to_string()],
            now,
        );
        assert_eq!(service.capabilities.len(), 1);
        assert!(matches!(service.capabilities[0], Capability::Custom { .. }));
    }
}
