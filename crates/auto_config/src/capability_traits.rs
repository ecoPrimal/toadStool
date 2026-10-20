// SPDX-License-Identifier: AGPL-3.0-or-later
//! # Capability Detection Traits
//!
//! Trait-based interfaces for hardware and ecosystem detection.
//! Enables dependency injection and testing with mock implementations.

use crate::ToadStoolResult;
use crate::ecosystem_types::DiscoveredServices;
use crate::hardware::SystemCapabilities;

/// Trait for hardware capability detection
///
/// This trait enables dependency injection and testing by allowing
/// both real and mock implementations of hardware detection.
#[expect(
    async_fn_in_trait,
    reason = "all implementors are Send + Sync; trait is internal, no dyn dispatch"
)]
pub trait HardwareCapabilityDetector: Send + Sync {
    /// Scan system hardware capabilities
    async fn scan_system(&mut self) -> ToadStoolResult<SystemCapabilities>;
}

/// Trait for ecosystem service discovery
///
/// This trait enables dependency injection and testing by allowing
/// both real and mock implementations of service discovery.
#[expect(
    async_fn_in_trait,
    reason = "all implementors are Send + Sync; trait is internal, no dyn dispatch"
)]
pub trait EcosystemServiceDiscoverer: Send + Sync {
    /// Discover available ecosystem services
    async fn discover_services(&mut self) -> ToadStoolResult<DiscoveredServices>;
}

// ============================================================================
// Adapters for Real Implementations
// ============================================================================

/// Adapter to make `HardwareDetector` implement the trait
impl HardwareCapabilityDetector for crate::hardware::HardwareDetector {
    async fn scan_system(&mut self) -> ToadStoolResult<SystemCapabilities> {
        Self::scan_system(self).await
    }
}

/// Adapter to make `EcosystemDiscoverer` implement the trait
#[cfg(feature = "runtime")]
impl EcosystemServiceDiscoverer for crate::ecosystem::EcosystemDiscoverer {
    async fn discover_services(&mut self) -> ToadStoolResult<DiscoveredServices> {
        Self::discover_services(self).await
    }
}

// ============================================================================
// Mock Implementations for Testing
// ============================================================================
//
// Evolution path: These mocks are behind #[cfg(any(test, feature = "test-mocks"))].
// Production always uses real HardwareDetector and EcosystemDiscoverer (see
// IntelligentAutoConfig::new(), ConfigBuilder::build(), get_system_summary()).
// If test-mocks feature is enabled, callers can inject mocks for integration tests.

/// Mock hardware detector for testing
///
/// Returns pre-configured capabilities instantly without any I/O.
/// Perfect for fast unit and integration tests.
#[cfg(any(test, feature = "test-mocks"))]
pub struct MockHardwareDetector {
    /// Pre-configured capabilities to return
    pub capabilities: SystemCapabilities,
}

#[cfg(any(test, feature = "test-mocks"))]
impl MockHardwareDetector {
    /// Create a mock detector with default test capabilities
    #[must_use]
    pub fn new() -> Self {
        Self {
            capabilities: SystemCapabilities {
                cpu_info: crate::hardware::CpuInfo::default(),
                cpu_cores: 8.0,
                memory_info: crate::hardware::MemoryInfo::default(),
                memory_gb: 16.0,
                gpu_info: Vec::new(),
                gpu_count: 0,
                gpu_memory_gb: None,
                storage_info: crate::hardware::StorageInfo::default(),
                storage_gb: 512.0,
                network_info: crate::hardware::NetworkInfo::default(),
                performance_class: crate::hardware::PerformanceClass::Mainstream,
            },
        }
    }

    /// Create a mock detector with custom capabilities
    #[must_use]
    pub const fn with_capabilities(capabilities: SystemCapabilities) -> Self {
        Self { capabilities }
    }
}

#[cfg(any(test, feature = "test-mocks"))]
impl Default for MockHardwareDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(any(test, feature = "test-mocks"))]
impl HardwareCapabilityDetector for MockHardwareDetector {
    async fn scan_system(&mut self) -> ToadStoolResult<SystemCapabilities> {
        // Instant return - no I/O
        Ok(self.capabilities.clone())
    }
}

/// Mock ecosystem discoverer for testing
///
/// Returns empty service list instantly without any network I/O.
/// Perfect for fast unit and integration tests.
#[cfg(any(test, feature = "test-mocks"))]
pub struct MockEcosystemDiscoverer {
    /// Pre-configured services to return
    pub services: DiscoveredServices,
}

#[cfg(any(test, feature = "test-mocks"))]
impl MockEcosystemDiscoverer {
    /// Create a mock discoverer with no services
    #[must_use]
    pub fn new() -> Self {
        Self {
            services: DiscoveredServices {
                discovered_services: std::collections::HashMap::new(),
                discovery_summary: crate::ecosystem_types::DiscoverySummary {
                    total_services_found: 0,
                    discovery_methods_used: vec!["mock".to_string()],
                    services_by_type: std::collections::HashMap::new(),
                    discovery_errors: Vec::new(),
                },
                discovery_timestamp: std::time::SystemTime::now(),
            },
        }
    }

    /// Create a mock discoverer with custom services
    #[must_use]
    pub const fn with_services(services: DiscoveredServices) -> Self {
        Self { services }
    }
}

#[cfg(any(test, feature = "test-mocks"))]
impl Default for MockEcosystemDiscoverer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(any(test, feature = "test-mocks"))]
impl EcosystemServiceDiscoverer for MockEcosystemDiscoverer {
    async fn discover_services(&mut self) -> ToadStoolResult<DiscoveredServices> {
        // Instant return - no network I/O
        Ok(self.services.clone())
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::float_cmp,
        reason = "exact comparison intended in this context"
    )]

    use super::*;

    #[tokio::test]
    async fn test_mock_hardware_detector() {
        let mut detector = MockHardwareDetector::new();
        let result = detector.scan_system().await;

        assert!(result.is_ok());
        let capabilities = result.unwrap();
        assert_eq!(capabilities.cpu_cores, 8.0);
        assert_eq!(capabilities.memory_gb, 16.0);
    }

    #[tokio::test]
    async fn test_mock_ecosystem_discoverer() {
        let mut discoverer = MockEcosystemDiscoverer::new();
        let result = discoverer.discover_services().await;

        assert!(result.is_ok());
        let services = result.unwrap();
        assert_eq!(services.discovered_services.len(), 0);
    }

    #[tokio::test]
    async fn test_mock_is_fast() {
        use std::time::Instant;

        let mut detector = MockHardwareDetector::new();
        let start = Instant::now();
        let _ = detector.scan_system().await;
        let duration = start.elapsed();

        // Should complete quickly (no I/O) - allowing 10ms for system variance
        assert!(
            duration.as_millis() < 10,
            "Mock should be fast but took {}ms",
            duration.as_millis()
        );
    }

    #[tokio::test]
    async fn test_mock_hardware_detector_with_custom_capabilities() {
        use crate::hardware::{PerformanceClass, SystemCapabilities};

        let caps = SystemCapabilities {
            cpu_cores: 16.0,
            memory_gb: 32.0,
            performance_class: PerformanceClass::HighEnd,
            ..Default::default()
        };

        let mut detector = MockHardwareDetector::with_capabilities(caps);
        let result = detector.scan_system().await;

        assert!(result.is_ok());
        let capabilities = result.unwrap();
        assert_eq!(capabilities.cpu_cores, 16.0);
        assert_eq!(capabilities.memory_gb, 32.0);
        assert!(matches!(
            capabilities.performance_class,
            PerformanceClass::HighEnd
        ));
    }

    #[tokio::test]
    async fn test_mock_ecosystem_discoverer_with_services() {
        use crate::ecosystem::{DiscoveredServices, DiscoverySummary};

        let mut services = std::collections::HashMap::new();
        services.insert(
            "test-svc".to_string(),
            {
                let mut si = crate::ecosystem::ServiceInfo::new("test-svc", "test-svc")
                    .with_endpoint("http://localhost:8080")
                    .with_version("1.0")
                    .with_capability("test");
                si.status = crate::ecosystem::ServiceStatus::Running;
                si.metadata.insert("service_type".into(), "Test".into());
                si.metadata
                    .insert("discovered_via".into(), "unit_test".into());
                si.metadata.insert("response_time_ms".into(), "0".into());
                si
            },
        );

        let discovered = DiscoveredServices {
            discovered_services: services,
            discovery_summary: DiscoverySummary::default(),
            discovery_timestamp: std::time::SystemTime::now(),
        };

        let mut mock_discoverer = MockEcosystemDiscoverer::with_services(discovered);
        let result = mock_discoverer.discover_services().await;

        assert!(result.is_ok());
        let discovered_result = result.unwrap();
        assert_eq!(discovered_result.discovered_services.len(), 1);
        assert!(
            discovered_result
                .discovered_services
                .contains_key("test-svc")
        );
    }
}
