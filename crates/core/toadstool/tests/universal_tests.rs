// SPDX-License-Identifier: AGPL-3.0-or-later
//! Comprehensive tests for universal compute platform

use std::collections::{HashMap, HashSet};
use toadstool::universal::*;

// ============================================================================
// SecurityLevel Tests (5 tests)
// ============================================================================

#[test]
fn test_security_level_variants() {
    let levels = [
        SecurityLevel::None,
        SecurityLevel::Standard,
        SecurityLevel::High,
        SecurityLevel::Maximum,
    ];
    assert_eq!(levels.len(), 4);
}

#[test]
fn test_security_level_ordering() {
    assert!(SecurityLevel::None < SecurityLevel::Standard);
    assert!(SecurityLevel::Standard < SecurityLevel::High);
    assert!(SecurityLevel::High < SecurityLevel::Maximum);
}

#[test]
fn test_security_level_equality() {
    assert_eq!(SecurityLevel::None, SecurityLevel::None);
    assert_ne!(SecurityLevel::None, SecurityLevel::Standard);
}

#[test]
fn test_security_level_clone() {
    let level = SecurityLevel::High;
    let cloned = level;
    assert_eq!(level, cloned);
}

#[test]
fn test_security_level_hash() {
    let mut set = HashSet::new();
    set.insert(SecurityLevel::None);
    set.insert(SecurityLevel::Standard);
    set.insert(SecurityLevel::None); // Duplicate
    assert_eq!(set.len(), 2); // Only 2 unique values
}

// ============================================================================
// NetworkLocation Tests (5 tests)
// ============================================================================

#[test]
fn test_network_location_creation() {
    let location = NetworkLocation {
        ip_address: "192.168.1.1".to_string(),
        subnet: Some("192.168.1.0/24".to_string()),
        network_id: Some("net-001".to_string()),
        geo_location: Some("US-WEST".to_string()),
    };

    assert_eq!(location.ip_address, "192.168.1.1");
    assert!(location.subnet.is_some());
}

#[test]
fn test_network_location_minimal() {
    let location = NetworkLocation {
        ip_address: "127.0.0.1".to_string(),
        subnet: None,
        network_id: None,
        geo_location: None,
    };

    assert_eq!(location.ip_address, "127.0.0.1");
    assert!(location.subnet.is_none());
}

#[test]
fn test_network_location_ipv6() {
    let location = NetworkLocation {
        ip_address: "2001:0db8:85a3::8a2e:0370:7334".to_string(),
        subnet: None,
        network_id: None,
        geo_location: None,
    };

    assert!(location.ip_address.contains("2001"));
}

#[test]
fn test_network_location_with_geo() {
    let location = NetworkLocation {
        ip_address: "10.0.0.1".to_string(),
        subnet: None,
        network_id: None,
        geo_location: Some("EU-CENTRAL".to_string()),
    };

    assert_eq!(location.geo_location.unwrap(), "EU-CENTRAL");
}

#[test]
fn test_network_location_clone() {
    let location1 = NetworkLocation {
        ip_address: "10.0.0.1".to_string(),
        subnet: None,
        network_id: None,
        geo_location: None,
    };

    let location2 = location1.clone();
    assert_eq!(location1.ip_address, location2.ip_address);
}

// ============================================================================
// PrimalContext Tests (5 tests)
// ============================================================================

#[test]
fn test_primal_context_creation() {
    let location = NetworkLocation {
        ip_address: "10.0.0.1".to_string(),
        subnet: None,
        network_id: None,
        geo_location: None,
    };

    let context = PrimalContext {
        user_id: "user-123".to_string(),
        device_id: "device-456".to_string(),
        session_id: "session-789".to_string(),
        network_location: location,
        security_level: SecurityLevel::Standard,
        metadata: HashMap::new(),
    };

    assert_eq!(context.user_id, "user-123");
    assert_eq!(context.device_id, "device-456");
}

#[test]
fn test_primal_context_with_metadata() {
    let mut metadata = HashMap::new();
    metadata.insert("app_version".to_string(), "1.0.0".to_string());
    metadata.insert("platform".to_string(), "linux".to_string());

    let location = NetworkLocation {
        ip_address: "10.0.0.1".to_string(),
        subnet: None,
        network_id: None,
        geo_location: None,
    };

    let context = PrimalContext {
        user_id: "user-123".to_string(),
        device_id: "device-456".to_string(),
        session_id: "session-789".to_string(),
        network_location: location,
        security_level: SecurityLevel::High,
        metadata,
    };

    assert_eq!(context.metadata.len(), 2);
    assert_eq!(context.metadata.get("app_version").unwrap(), "1.0.0");
}

#[test]
fn test_primal_context_security_levels() {
    let location = NetworkLocation {
        ip_address: "10.0.0.1".to_string(),
        subnet: None,
        network_id: None,
        geo_location: None,
    };

    let levels = [
        SecurityLevel::None,
        SecurityLevel::Standard,
        SecurityLevel::High,
        SecurityLevel::Maximum,
    ];
    for level in levels {
        let _ = PrimalContext {
            user_id: "user".to_string(),
            device_id: "device".to_string(),
            session_id: "session".to_string(),
            network_location: location.clone(),
            security_level: level,
            metadata: HashMap::new(),
        };
    }
    assert_eq!(levels.len(), 4);
}

#[test]
fn test_primal_context_clone() {
    let location = NetworkLocation {
        ip_address: "10.0.0.1".to_string(),
        subnet: None,
        network_id: None,
        geo_location: None,
    };

    let context1 = PrimalContext {
        user_id: "user-123".to_string(),
        device_id: "device-456".to_string(),
        session_id: "session-789".to_string(),
        network_location: location,
        security_level: SecurityLevel::Standard,
        metadata: HashMap::new(),
    };

    let context2 = context1.clone();
    assert_eq!(context1.user_id, context2.user_id);
}

#[test]
fn test_primal_context_equality() {
    let location = NetworkLocation {
        ip_address: "10.0.0.1".to_string(),
        subnet: None,
        network_id: None,
        geo_location: None,
    };

    let context1 = PrimalContext {
        user_id: "user-123".to_string(),
        device_id: "device-456".to_string(),
        session_id: "session-789".to_string(),
        network_location: location,
        security_level: SecurityLevel::Standard,
        metadata: HashMap::new(),
    };

    let context2 = context1.clone();
    assert_eq!(context1, context2);
}

// ============================================================================
// PrimalType Tests (7 tests)
// ============================================================================

#[test]
fn test_primal_type_variants() {
    let types = [
        PrimalType::Compute,
        PrimalType::Security,
        PrimalType::Storage,
        PrimalType::AI,
        PrimalType::Network,
        PrimalType::OS,
        PrimalType::Custom("test".to_string()),
    ];

    assert_eq!(types.len(), 7);
}

#[test]
fn test_primal_type_compute() {
    let primal_type = PrimalType::Compute;
    assert!(matches!(primal_type, PrimalType::Compute));
}

#[test]
fn test_primal_type_security() {
    let primal_type = PrimalType::Security;
    assert!(matches!(primal_type, PrimalType::Security));
}

#[test]
fn test_primal_type_custom() {
    let primal_type = PrimalType::Custom("my-primal".to_string());

    if let PrimalType::Custom(name) = primal_type {
        assert_eq!(name, "my-primal");
    } else {
        panic!("Expected Custom variant");
    }
}

#[test]
fn test_primal_type_equality() {
    assert_eq!(PrimalType::Compute, PrimalType::Compute);
    assert_ne!(PrimalType::Compute, PrimalType::Security);
}

#[test]
fn test_primal_type_hash() {
    let mut set = HashSet::new();
    set.insert(PrimalType::Compute);
    set.insert(PrimalType::Security);
    set.insert(PrimalType::Compute); // Duplicate

    assert_eq!(set.len(), 2);
}

#[test]
fn test_primal_type_all_built_in() {
    let types = [
        PrimalType::Compute,
        PrimalType::Security,
        PrimalType::Storage,
        PrimalType::AI,
        PrimalType::Network,
        PrimalType::OS,
    ];

    assert_eq!(types.len(), 6);
}

// ============================================================================
// PrimalHealth Tests (3 tests)
// ============================================================================

#[test]
fn test_primal_health_healthy() {
    let health = PrimalHealth::Healthy;
    assert!(matches!(health, PrimalHealth::Healthy));
}

#[test]
fn test_primal_health_degraded() {
    let health = PrimalHealth::Degraded {
        issues: vec!["high latency".to_string()],
    };

    if let PrimalHealth::Degraded { issues } = health {
        assert_eq!(issues.len(), 1);
    }
}

#[test]
fn test_primal_health_unhealthy() {
    let health = PrimalHealth::Unhealthy {
        reason: "connection failed".to_string(),
    };

    if let PrimalHealth::Unhealthy { reason } = health {
        assert!(reason.contains("failed"));
    }
}

// ============================================================================
// ResponseStatus Tests (4 tests)
// ============================================================================

#[test]
fn test_response_status_success() {
    let status = ResponseStatus::Success;
    assert!(matches!(status, ResponseStatus::Success));
}

#[test]
fn test_response_status_error() {
    let status = ResponseStatus::Error {
        code: "500".to_string(),
        message: "test error".to_string(),
    };

    if let ResponseStatus::Error { code, message } = status {
        assert_eq!(code, "500");
        assert_eq!(message, "test error");
    }
}

#[test]
fn test_response_status_timeout() {
    let status = ResponseStatus::Timeout;
    assert!(matches!(status, ResponseStatus::Timeout));
}

#[test]
fn test_response_status_service_unavailable() {
    let status = ResponseStatus::ServiceUnavailable;
    assert!(matches!(status, ResponseStatus::ServiceUnavailable));
}

// ============================================================================
// JobPriority Tests (5 tests)
// ============================================================================

#[test]
fn test_job_priority_variants() {
    let priorities = [
        JobPriority::Low,
        JobPriority::Normal,
        JobPriority::High,
        JobPriority::Critical,
        JobPriority::Emergency,
    ];

    assert_eq!(priorities.len(), 5);
}

#[test]
fn test_job_priority_ordering() {
    // Lower numeric values = higher priority, so Emergency < Low
    assert!(JobPriority::Emergency < JobPriority::Critical);
    assert!(JobPriority::Critical < JobPriority::High);
    assert!(JobPriority::High < JobPriority::Normal);
    assert!(JobPriority::Normal < JobPriority::Low);
}

#[test]
fn test_job_priority_equality() {
    assert_eq!(JobPriority::High, JobPriority::High);
    assert_ne!(JobPriority::Low, JobPriority::Critical);
}

#[test]
fn test_job_priority_max() {
    // With Emergency=0, Low=4: Critical (1) < Low (4), so max returns Low
    assert_eq!(
        JobPriority::Low,
        JobPriority::Critical.max(JobPriority::Low)
    );
}

#[test]
fn test_job_priority_min() {
    // With Emergency=0, Low=4: Critical (1) < Low (4), so min returns Critical
    assert_eq!(
        JobPriority::Critical,
        JobPriority::Critical.min(JobPriority::Low)
    );
}

// ============================================================================
// UniversalPrimalRegistry Tests (2 tests)
// ============================================================================

#[test]
fn test_universal_primal_registry_creation() {
    let _registry = UniversalPrimalRegistry::<UniversalPrimalProviderDispatch>::new();
    // Registry should be created without panicking
}

#[test]
fn test_universal_primal_registry_default() {
    let _registry = UniversalPrimalRegistry::<UniversalPrimalProviderDispatch>::default();
    // Default registry should be valid
}

// ============================================================================
// PlatformStatus Tests (4 tests)
// ============================================================================

#[test]
fn test_platform_status_initializing() {
    let status = PlatformStatus::Initializing;
    assert!(matches!(status, PlatformStatus::Initializing));
}

#[test]
fn test_platform_status_running() {
    let status = PlatformStatus::Running;
    assert!(matches!(status, PlatformStatus::Running));
}

#[test]
fn test_platform_status_degraded() {
    let status = PlatformStatus::Degraded;
    assert!(matches!(status, PlatformStatus::Degraded));
}

#[test]
fn test_platform_status_stopped() {
    let status = PlatformStatus::Stopped;
    assert!(matches!(status, PlatformStatus::Stopped));
}
