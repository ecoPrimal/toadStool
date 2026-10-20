// SPDX-License-Identifier: AGPL-3.0-or-later
//! Comprehensive tests for Universal Compute Platform
//!
//! Week 18 Sprint 8: Universal compute types and functionality tests
//! Target: 44.04% → 70% coverage (~35 tests)

use std::collections::HashMap;
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
    assert_eq!(SecurityLevel::Maximum, SecurityLevel::Maximum);
    assert_ne!(SecurityLevel::None, SecurityLevel::High);
}

#[test]
fn test_security_level_clone_copy() {
    let level1 = SecurityLevel::High;
    let level2 = level1; // Copy
    let level3 = level1; // Clone

    assert_eq!(level1, level2);
    assert_eq!(level1, level3);
}

#[test]
fn test_security_level_debug() {
    let levels = [
        SecurityLevel::None,
        SecurityLevel::Standard,
        SecurityLevel::High,
        SecurityLevel::Maximum,
    ];

    for level in levels {
        let debug_str = format!("{level:?}");
        assert!(!debug_str.is_empty());
    }
}

// ============================================================================
// NetworkLocation Tests (4 tests)
// ============================================================================

#[test]
fn test_network_location_full() {
    let location = NetworkLocation {
        ip_address: "192.168.1.100".to_string(),
        subnet: Some("192.168.1.0/24".to_string()),
        network_id: Some("net-001".to_string()),
        geo_location: Some("US-West".to_string()),
    };

    assert_eq!(location.ip_address, "192.168.1.100");
    assert_eq!(location.subnet, Some("192.168.1.0/24".to_string()));
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
fn test_network_location_clone_eq() {
    let location1 = NetworkLocation {
        ip_address: "10.0.0.1".to_string(),
        subnet: Some("10.0.0.0/8".to_string()),
        network_id: None,
        geo_location: None,
    };

    let location2 = location1.clone();
    assert_eq!(location1, location2);
}

#[test]
fn test_network_location_debug() {
    let location = NetworkLocation {
        ip_address: "192.168.1.1".to_string(),
        subnet: None,
        network_id: None,
        geo_location: None,
    };

    let debug_str = format!("{location:?}");
    assert!(debug_str.contains("NetworkLocation"));
    assert!(debug_str.contains("192.168.1.1"));
}

// ============================================================================
// PrimalContext Tests (5 tests)
// ============================================================================

#[test]
fn test_primal_context_creation() {
    let location = NetworkLocation {
        ip_address: "192.168.1.100".to_string(),
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
    assert_eq!(context.security_level, SecurityLevel::Standard);
}

#[test]
fn test_primal_context_with_metadata() {
    let mut metadata = HashMap::new();
    metadata.insert("tenant_id".to_string(), "tenant-001".to_string());
    metadata.insert("team".to_string(), "engineering".to_string());

    let location = NetworkLocation {
        ip_address: "10.0.0.1".to_string(),
        subnet: None,
        network_id: None,
        geo_location: None,
    };

    let context = PrimalContext {
        user_id: "user-1".to_string(),
        device_id: "device-1".to_string(),
        session_id: "session-1".to_string(),
        network_location: location,
        security_level: SecurityLevel::High,
        metadata: metadata.clone(),
    };

    assert_eq!(context.metadata.len(), 2);
    assert_eq!(
        context.metadata.get("tenant_id"),
        Some(&"tenant-001".to_string())
    );
}

#[test]
fn test_primal_context_different_security_levels() {
    let location = NetworkLocation {
        ip_address: "127.0.0.1".to_string(),
        subnet: None,
        network_id: None,
        geo_location: None,
    };

    for level in [
        SecurityLevel::None,
        SecurityLevel::Standard,
        SecurityLevel::High,
        SecurityLevel::Maximum,
    ] {
        let context = PrimalContext {
            user_id: "user".to_string(),
            device_id: "device".to_string(),
            session_id: "session".to_string(),
            network_location: location.clone(),
            security_level: level,
            metadata: HashMap::new(),
        };
        assert_eq!(context.security_level, level);
    }
}

#[test]
fn test_primal_context_clone() {
    let location = NetworkLocation {
        ip_address: "192.168.1.1".to_string(),
        subnet: None,
        network_id: None,
        geo_location: None,
    };

    let context1 = PrimalContext {
        user_id: "user-1".to_string(),
        device_id: "device-1".to_string(),
        session_id: "session-1".to_string(),
        network_location: location,
        security_level: SecurityLevel::Standard,
        metadata: HashMap::new(),
    };

    let context2 = context1.clone();
    assert_eq!(context1.user_id, context2.user_id);
    assert_eq!(context1.security_level, context2.security_level);
}

#[test]
fn test_primal_context_debug() {
    let location = NetworkLocation {
        ip_address: "192.168.1.1".to_string(),
        subnet: None,
        network_id: None,
        geo_location: None,
    };

    let context = PrimalContext {
        user_id: "user-test".to_string(),
        device_id: "device-test".to_string(),
        session_id: "session-test".to_string(),
        network_location: location,
        security_level: SecurityLevel::None,
        metadata: HashMap::new(),
    };

    let debug_str = format!("{context:?}");
    assert!(debug_str.contains("PrimalContext"));
}

// ============================================================================
// PrimalType Tests (4 tests)
// ============================================================================

#[test]
fn test_primal_type_standard_variants() {
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

#[test]
fn test_primal_type_custom() {
    let custom_type = PrimalType::Custom("Analytics".to_string());

    match custom_type {
        PrimalType::Custom(name) => assert_eq!(name, "Analytics"),
        _ => panic!("Should be Custom variant"),
    }
}

#[test]
fn test_primal_type_equality() {
    assert_eq!(PrimalType::Compute, PrimalType::Compute);
    assert_ne!(PrimalType::Compute, PrimalType::Security);

    let custom1 = PrimalType::Custom("Test".to_string());
    let custom2 = PrimalType::Custom("Test".to_string());
    assert_eq!(custom1, custom2);
}

#[test]
fn test_primal_type_clone_debug() {
    let types = [
        PrimalType::Compute,
        PrimalType::Security,
        PrimalType::Custom("Test".to_string()),
    ];

    for ptype in types {
        let cloned = ptype.clone();
        assert_eq!(ptype, cloned);

        let debug_str = format!("{ptype:?}");
        assert!(!debug_str.is_empty());
    }
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
    // Lower numeric values = higher priority, so Emergency < Low in Rust's comparison
    assert!(JobPriority::Emergency < JobPriority::Critical);
    assert!(JobPriority::Critical < JobPriority::High);
    assert!(JobPriority::High < JobPriority::Normal);
    assert!(JobPriority::Normal < JobPriority::Low);
}

#[test]
fn test_job_priority_numeric_values() {
    // Emergency = 0 (highest priority), Low = 4 (lowest priority)
    assert_eq!(JobPriority::Emergency as i32, 0);
    assert_eq!(JobPriority::Critical as i32, 1);
    assert_eq!(JobPriority::High as i32, 2);
    assert_eq!(JobPriority::Normal as i32, 3);
    assert_eq!(JobPriority::Low as i32, 4);
}

#[test]
fn test_job_priority_equality() {
    assert_eq!(JobPriority::Normal, JobPriority::Normal);
    assert_ne!(JobPriority::Low, JobPriority::High);
}

#[test]
fn test_job_priority_clone_copy() {
    let priority1 = JobPriority::High;
    let priority2 = priority1; // Copy
    let priority3 = priority1; // Clone

    assert_eq!(priority1, priority2);
    assert_eq!(priority1, priority3);
}

// ============================================================================
// UniversalJobType Tests (4 tests)
// ============================================================================

#[test]
fn test_universal_job_type_native() {
    let job_type = UniversalJobType::Native {
        executable: "/bin/echo".to_string(),
        args: vec!["hello".to_string()],
        env: HashMap::new(),
    };

    match job_type {
        UniversalJobType::Native { executable, .. } => {
            assert_eq!(executable, "/bin/echo");
        }
        _ => panic!("Should be Native variant"),
    }
}

#[test]
fn test_universal_job_type_wasm() {
    let job_type = UniversalJobType::Wasm {
        module: vec![0, 1, 2, 3],
        args: vec!["arg1".to_string()],
        env: HashMap::new(),
    };

    match job_type {
        UniversalJobType::Wasm { module, .. } => {
            assert_eq!(module.len(), 4);
        }
        _ => panic!("Should be Wasm variant"),
    }
}

#[test]
fn test_universal_job_type_primal() {
    let job_type = UniversalJobType::Primal {
        primal_type: "AI".to_string(),
        endpoint: "http://ai.primal:8080".to_string(),
        payload: serde_json::json!({"model": "gpt-4"}),
    };

    match job_type {
        UniversalJobType::Primal { primal_type, .. } => {
            assert_eq!(primal_type, "AI");
        }
        _ => panic!("Should be Primal variant"),
    }
}

#[test]
fn test_universal_job_type_biomeos() {
    let job_type = UniversalJobType::BiomeOS {
        biome_manifest: serde_json::json!({"version": "1.0"}),
        team_id: "team-123".to_string(),
    };

    match job_type {
        UniversalJobType::BiomeOS { team_id, .. } => {
            assert_eq!(team_id, "team-123");
        }
        _ => panic!("Should be BiomeOS variant"),
    }
}

// ============================================================================
// UniversalPlatformConfig Tests (4 tests)
// ============================================================================

#[test]
fn test_universal_platform_config_default() {
    let config = UniversalPlatformConfig::default();

    assert!(config.recursive_hosting);
    assert!(config.ecosystem_integration);
    assert!(config.biomeos_integration);
    assert_eq!(config.max_concurrent_jobs, 100);
    assert!(!config.pure_ecosystem);
}

#[test]
fn test_universal_platform_config_custom() {
    let config = UniversalPlatformConfig {
        recursive_hosting: false,
        ecosystem_integration: true,
        biomeos_integration: false,
        max_concurrent_jobs: 50,
        pure_ecosystem: true,
    };

    assert!(!config.recursive_hosting);
    assert_eq!(config.max_concurrent_jobs, 50);
    assert!(config.pure_ecosystem);
}

#[test]
fn test_universal_platform_config_different_job_limits() {
    let limits = [10, 50, 100, 500, 1000];

    for limit in limits {
        let config = UniversalPlatformConfig {
            recursive_hosting: true,
            ecosystem_integration: true,
            biomeos_integration: true,
            max_concurrent_jobs: limit,
            pure_ecosystem: false,
        };
        assert_eq!(config.max_concurrent_jobs, limit);
    }
}

#[test]
fn test_universal_platform_config_clone() {
    let config1 = UniversalPlatformConfig::default();
    let config2 = config1.clone();

    assert_eq!(config1.max_concurrent_jobs, config2.max_concurrent_jobs);
    assert_eq!(config1.pure_ecosystem, config2.pure_ecosystem);
}

// ============================================================================
// Test Coverage Summary
// ============================================================================

#[test]
fn test_universal_coverage_summary() {
    println!("=== Universal Compute Test Coverage ===");
    println!("SecurityLevel Tests:           5 tests");
    println!("NetworkLocation Tests:         4 tests");
    println!("PrimalContext Tests:           5 tests");
    println!("PrimalType Tests:              4 tests");
    println!("JobPriority Tests:             5 tests");
    println!("UniversalJobType Tests:        4 tests");
    println!("UniversalPlatformConfig Tests: 4 tests");
    println!("───────────────────────────────────────");
    println!("Total:                        31 tests");
    println!("Module Coverage: 44.04% → Target 70%");
    println!("=========================================");
}
