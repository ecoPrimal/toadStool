// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 ecoPrimals
#![allow(
    clippy::float_cmp,
    reason = "exact comparison intended in this context"
)]
//! Comprehensive tests for universal.rs core types
//!
//! Coverage push - Nov 7, 2025
//! Target: Bring universal.rs from 44% → 60%+

use std::collections::HashMap;
use toadstool::SystemResources;
use toadstool::universal::*;
use uuid::Uuid;

// ============================================================================
// SecurityLevel Tests
// ============================================================================

#[test]
fn test_security_level_ordering() {
    assert!(SecurityLevel::Maximum > SecurityLevel::High);
    assert!(SecurityLevel::High > SecurityLevel::Standard);
    assert!(SecurityLevel::Standard > SecurityLevel::None);
}

#[test]
fn test_security_level_equality() {
    assert_eq!(SecurityLevel::None, SecurityLevel::None);
    assert_eq!(SecurityLevel::Maximum, SecurityLevel::Maximum);
    assert_ne!(SecurityLevel::None, SecurityLevel::High);
}

#[test]
fn test_security_level_clone() {
    let level = SecurityLevel::High;
    let cloned = level;
    assert_eq!(level, cloned);
}

#[test]
fn test_security_level_copy() {
    let level = SecurityLevel::Standard;
    let copied = level;
    assert_eq!(level, copied);
}

// ============================================================================
// NetworkLocation Tests
// ============================================================================

#[test]
fn test_network_location_creation() {
    let location = NetworkLocation {
        ip_address: "192.168.1.100".to_string(),
        subnet: Some("192.168.1.0/24".to_string()),
        network_id: Some("net-001".to_string()),
        geo_location: Some("us-west-2".to_string()),
    };

    assert_eq!(location.ip_address, "192.168.1.100");
    assert_eq!(location.subnet, Some("192.168.1.0/24".to_string()));
    assert_eq!(location.network_id, Some("net-001".to_string()));
    assert_eq!(location.geo_location, Some("us-west-2".to_string()));
}

#[test]
fn test_network_location_minimal() {
    let location = NetworkLocation {
        ip_address: "10.0.0.1".to_string(),
        subnet: None,
        network_id: None,
        geo_location: None,
    };

    assert_eq!(location.ip_address, "10.0.0.1");
    assert!(location.subnet.is_none());
    assert!(location.network_id.is_none());
    assert!(location.geo_location.is_none());
}

#[test]
fn test_network_location_clone() {
    let location = NetworkLocation {
        ip_address: "172.16.0.1".to_string(),
        subnet: Some("172.16.0.0/16".to_string()),
        network_id: Some("net-prod".to_string()),
        geo_location: Some("eu-central-1".to_string()),
    };

    let cloned = location.clone();
    assert_eq!(location.ip_address, cloned.ip_address);
    assert_eq!(location.subnet, cloned.subnet);
}

// ============================================================================
// PrimalContext Tests
// ============================================================================

#[test]
fn test_primal_context_creation() {
    let mut metadata = HashMap::new();
    metadata.insert("app_version".to_string(), "1.0.0".to_string());

    let context = PrimalContext {
        user_id: "user-123".to_string(),
        device_id: "device-456".to_string(),
        session_id: "session-789".to_string(),
        network_location: NetworkLocation {
            ip_address: "203.0.113.0".to_string(),
            subnet: None,
            network_id: None,
            geo_location: None,
        },
        security_level: SecurityLevel::High,
        metadata,
    };

    assert_eq!(context.user_id, "user-123");
    assert_eq!(context.device_id, "device-456");
    assert_eq!(context.session_id, "session-789");
    assert_eq!(context.security_level, SecurityLevel::High);
    assert_eq!(context.metadata.len(), 1);
}

#[test]
fn test_primal_context_metadata() {
    let mut metadata = HashMap::new();
    metadata.insert("region".to_string(), "us-east-1".to_string());
    metadata.insert("environment".to_string(), "production".to_string());
    metadata.insert("tenant_id".to_string(), "tenant-001".to_string());

    let context = PrimalContext {
        user_id: "admin".to_string(),
        device_id: "server-01".to_string(),
        session_id: Uuid::new_v4().to_string(),
        network_location: NetworkLocation {
            ip_address: "10.0.0.1".to_string(),
            subnet: Some("10.0.0.0/8".to_string()),
            network_id: Some("vpc-prod".to_string()),
            geo_location: Some("us-east-1a".to_string()),
        },
        security_level: SecurityLevel::Maximum,
        metadata,
    };

    assert_eq!(context.metadata.len(), 3);
    assert_eq!(
        context.metadata.get("region"),
        Some(&"us-east-1".to_string())
    );
    assert_eq!(
        context.metadata.get("environment"),
        Some(&"production".to_string())
    );
}

#[test]
fn test_primal_context_security_levels() {
    let create_context = |level: SecurityLevel| PrimalContext {
        user_id: "test-user".to_string(),
        device_id: "test-device".to_string(),
        session_id: "test-session".to_string(),
        network_location: NetworkLocation {
            ip_address: "127.0.0.1".to_string(),
            subnet: None,
            network_id: None,
            geo_location: None,
        },
        security_level: level,
        metadata: HashMap::new(),
    };

    let basic = create_context(SecurityLevel::None);
    let standard = create_context(SecurityLevel::Standard);
    let high = create_context(SecurityLevel::High);
    let maximum = create_context(SecurityLevel::Maximum);

    assert_eq!(basic.security_level, SecurityLevel::None);
    assert_eq!(standard.security_level, SecurityLevel::Standard);
    assert_eq!(high.security_level, SecurityLevel::High);
    assert_eq!(maximum.security_level, SecurityLevel::Maximum);
}

// ============================================================================
// PrimalType Tests
// ============================================================================

#[test]
fn test_primal_type_variants() {
    let compute = PrimalType::Compute;
    let security = PrimalType::Security;
    let storage = PrimalType::Storage;
    let ai = PrimalType::AI;
    let network = PrimalType::Network;
    let os = PrimalType::OS;
    let custom = PrimalType::Custom("CustomPrimal".to_string());

    assert!(matches!(compute, PrimalType::Compute));
    assert!(matches!(security, PrimalType::Security));
    assert!(matches!(storage, PrimalType::Storage));
    assert!(matches!(ai, PrimalType::AI));
    assert!(matches!(network, PrimalType::Network));
    assert!(matches!(os, PrimalType::OS));
    assert!(matches!(custom, PrimalType::Custom(_)));
}

#[test]
fn test_primal_type_equality() {
    assert_eq!(PrimalType::Compute, PrimalType::Compute);
    assert_eq!(PrimalType::Security, PrimalType::Security);
    assert_ne!(PrimalType::Compute, PrimalType::Security);

    let custom1 = PrimalType::Custom("Type1".to_string());
    let custom2 = PrimalType::Custom("Type1".to_string());
    let custom3 = PrimalType::Custom("Type2".to_string());

    assert_eq!(custom1, custom2);
    assert_ne!(custom1, custom3);
}

#[test]
fn test_primal_type_clone() {
    let primal_type = PrimalType::Custom("Analytics".to_string());
    let cloned = primal_type.clone();
    assert_eq!(primal_type, cloned);
}

// ============================================================================
// JobPriority Tests
// ============================================================================

#[test]
fn test_job_priority_ordering() {
    // Lower numeric values = higher priority, so Emergency < Low in Rust's comparison
    assert!(JobPriority::Emergency < JobPriority::Critical);
    assert!(JobPriority::Critical < JobPriority::High);
    assert!(JobPriority::High < JobPriority::Normal);
    assert!(JobPriority::Normal < JobPriority::Low);
}

#[test]
fn test_job_priority_values() {
    // Emergency = 0 (highest priority), Low = 4 (lowest priority)
    assert_eq!(JobPriority::Emergency as i32, 0);
    assert_eq!(JobPriority::Critical as i32, 1);
    assert_eq!(JobPriority::High as i32, 2);
    assert_eq!(JobPriority::Normal as i32, 3);
    assert_eq!(JobPriority::Low as i32, 4);
}

#[test]
fn test_job_priority_equality() {
    assert_eq!(JobPriority::Low, JobPriority::Low);
    assert_eq!(JobPriority::Emergency, JobPriority::Emergency);
    assert_ne!(JobPriority::Low, JobPriority::High);
}

#[test]
fn test_job_priority_clone() {
    let priority = JobPriority::Critical;
    let cloned = priority;
    assert_eq!(priority, cloned);
}

// ============================================================================
// SystemResources Tests
// ============================================================================

#[test]
fn test_system_resources_creation() {
    let resources = SystemResources {
        available_cpu_cores: 16.0,
        available_memory_bytes: 64 * 1024 * 1024 * 1024, // 64 GB
        available_storage_bytes: 1000 * 1024 * 1024 * 1024, // 1000 GB
        available_network_bandwidth: Some(10 * 1024 * 1024 * 1024), // 10 Gbps
        available_gpu_units: 2,
        ..Default::default()
    };

    assert_eq!(resources.available_cpu_cores, 16.0);
    assert_eq!(resources.available_memory_bytes, 64 * 1024 * 1024 * 1024);
    assert_eq!(resources.available_storage_bytes, 1000 * 1024 * 1024 * 1024);
    assert_eq!(resources.available_gpu_units, 2);
    assert_eq!(
        resources.available_network_bandwidth,
        Some(10 * 1024 * 1024 * 1024)
    );
}

#[test]
fn test_system_resources_minimal() {
    let resources = SystemResources {
        available_cpu_cores: 1.0,
        available_memory_bytes: 1024 * 1024 * 1024, // 1 GB
        available_storage_bytes: 10 * 1024 * 1024 * 1024, // 10 GB
        available_network_bandwidth: Some(100 * 1024 * 1024), // 100 Mbps
        available_gpu_units: 0,
        ..Default::default()
    };

    assert_eq!(resources.available_cpu_cores, 1.0);
    assert_eq!(resources.available_memory_bytes, 1024 * 1024 * 1024);
    assert_eq!(resources.available_storage_bytes, 10 * 1024 * 1024 * 1024);
    assert_eq!(resources.available_gpu_units, 0);
}

#[test]
fn test_system_resources_high_spec() {
    let resources = SystemResources {
        available_cpu_cores: 128.0,
        available_memory_bytes: 512 * 1024 * 1024 * 1024, // 512 GB
        available_storage_bytes: 10000 * 1024 * 1024 * 1024, // 10 TB
        available_network_bandwidth: Some(100 * 1024 * 1024 * 1024), // 100 Gbps
        available_gpu_units: 8,
        ..Default::default()
    };

    assert!(resources.available_cpu_cores >= 64.0);
    assert!(resources.available_memory_bytes >= 256 * 1024 * 1024 * 1024);
    assert!(resources.available_gpu_units >= 4);
}

#[test]
fn test_system_resources_clone() {
    let resources = SystemResources {
        available_cpu_cores: 8.0,
        available_memory_bytes: 32 * 1024 * 1024 * 1024, // 32 GB
        available_storage_bytes: 500 * 1024 * 1024 * 1024, // 500 GB
        available_network_bandwidth: Some(1024 * 1024 * 1024), // 1 Gbps
        available_gpu_units: 1,
        ..Default::default()
    };

    let cloned = resources.clone();
    assert_eq!(resources.available_cpu_cores, cloned.available_cpu_cores);
    assert_eq!(
        resources.available_memory_bytes,
        cloned.available_memory_bytes
    );
    assert_eq!(resources.available_gpu_units, cloned.available_gpu_units);
    assert_eq!(
        resources.available_storage_bytes,
        cloned.available_storage_bytes
    );
    assert_eq!(
        resources.available_network_bandwidth,
        cloned.available_network_bandwidth
    );
}

// ============================================================================
// Integration Tests
// ============================================================================

#[test]
fn test_complete_primal_context_workflow() {
    // Create a complete context with all fields
    let mut metadata = HashMap::new();
    metadata.insert("correlation_id".to_string(), Uuid::new_v4().to_string());
    metadata.insert("request_source".to_string(), "api".to_string());

    let context = PrimalContext {
        user_id: format!("user-{}", Uuid::new_v4()),
        device_id: format!("device-{}", Uuid::new_v4()),
        session_id: Uuid::new_v4().to_string(),
        network_location: NetworkLocation {
            ip_address: "198.51.100.42".to_string(),
            subnet: Some("198.51.100.0/24".to_string()),
            network_id: Some("vpc-12345".to_string()),
            geo_location: Some("ap-southeast-1".to_string()),
        },
        security_level: SecurityLevel::High,
        metadata,
    };

    // Verify complete context
    assert!(context.user_id.starts_with("user-"));
    assert!(context.device_id.starts_with("device-"));
    assert_eq!(context.security_level, SecurityLevel::High);
    assert_eq!(context.network_location.ip_address, "198.51.100.42");
    assert_eq!(context.metadata.len(), 2);
}

#[test]
fn test_security_level_progression() {
    let levels = vec![
        SecurityLevel::None,
        SecurityLevel::Standard,
        SecurityLevel::High,
        SecurityLevel::Maximum,
    ];

    // Verify progression
    for i in 0..levels.len() - 1 {
        assert!(levels[i] < levels[i + 1]);
    }
}

#[test]
fn test_primal_types_comprehensive() {
    let types = vec![
        PrimalType::Compute,
        PrimalType::Security,
        PrimalType::Storage,
        PrimalType::AI,
        PrimalType::Network,
        PrimalType::OS,
        PrimalType::Custom("Edge".to_string()),
        PrimalType::Custom("Analytics".to_string()),
    ];

    assert_eq!(types.len(), 8);

    // Verify each type
    for primal_type in &types {
        match primal_type {
            PrimalType::Compute
            | PrimalType::Security
            | PrimalType::Storage
            | PrimalType::AI
            | PrimalType::Network
            | PrimalType::OS
            | PrimalType::Custom(_) => { /* Valid */ }
        }
    }
}
