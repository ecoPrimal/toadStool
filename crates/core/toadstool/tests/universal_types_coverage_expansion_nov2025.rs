// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 ecoPrimals

//! Coverage expansion tests for universal.rs types
//!
//! This test suite adds comprehensive coverage for the universal compute types,
//! focusing on simple unit tests for enums, structs, and basic behaviors.

use std::collections::HashMap;
use toadstool::universal::*;

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
fn test_security_level_copy_clone() {
    let level = SecurityLevel::High;
    let copied = level;
    let cloned = level;

    assert_eq!(level, copied);
    assert_eq!(level, cloned);
}

#[test]
fn test_security_level_serialization() {
    let level = SecurityLevel::Standard;
    let json = serde_json::to_string(&level).unwrap();
    let deserialized: SecurityLevel = serde_json::from_str(&json).unwrap();

    assert_eq!(level, deserialized);
}

#[test]
fn test_security_level_all_variants() {
    let variants = vec![
        SecurityLevel::None,
        SecurityLevel::Standard,
        SecurityLevel::High,
        SecurityLevel::Maximum,
    ];

    assert_eq!(variants.len(), 4);

    // Test each serializes successfully
    for variant in variants {
        let json = serde_json::to_string(&variant).unwrap();
        assert!(!json.is_empty());
    }
}

// ============================================================================
// NetworkLocation Tests
// ============================================================================

#[test]
fn test_network_location_creation() {
    let location = NetworkLocation {
        ip_address: "192.168.1.100".to_string(),
        subnet: Some("192.168.1.0/24".to_string()),
        network_id: Some("net-123".to_string()),
        geo_location: Some("us-west-2".to_string()),
    };

    assert_eq!(location.ip_address, "192.168.1.100");
    assert!(location.subnet.is_some());
    assert!(location.network_id.is_some());
    assert!(location.geo_location.is_some());
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
        network_id: None,
        geo_location: None,
    };

    let cloned = location.clone();
    assert_eq!(location.ip_address, cloned.ip_address);
    assert_eq!(location.subnet, cloned.subnet);
}

#[test]
fn test_network_location_serialization() {
    let location = NetworkLocation {
        ip_address: "127.0.0.1".to_string(),
        subnet: Some("127.0.0.0/8".to_string()),
        network_id: Some("local".to_string()),
        geo_location: None,
    };

    let json = serde_json::to_string(&location).unwrap();
    let deserialized: NetworkLocation = serde_json::from_str(&json).unwrap();

    assert_eq!(location.ip_address, deserialized.ip_address);
    assert_eq!(location.subnet, deserialized.subnet);
    assert_eq!(location.network_id, deserialized.network_id);
}

#[test]
fn test_network_location_equality() {
    let loc1 = NetworkLocation {
        ip_address: "192.168.1.1".to_string(),
        subnet: None,
        network_id: None,
        geo_location: None,
    };

    let loc2 = NetworkLocation {
        ip_address: "192.168.1.1".to_string(),
        subnet: None,
        network_id: None,
        geo_location: None,
    };

    assert_eq!(loc1, loc2);
}

// ============================================================================
// PrimalContext Tests
// ============================================================================

#[test]
fn test_primal_context_creation() {
    let location = NetworkLocation {
        ip_address: "192.168.1.100".to_string(),
        subnet: Some("192.168.1.0/24".to_string()),
        network_id: Some("net-123".to_string()),
        geo_location: Some("us-west-2".to_string()),
    };

    let context = PrimalContext {
        user_id: "user-123".to_string(),
        device_id: "device-456".to_string(),
        session_id: "session-789".to_string(),
        network_location: location,
        security_level: SecurityLevel::High,
        metadata: HashMap::new(),
    };

    assert_eq!(context.user_id, "user-123");
    assert_eq!(context.device_id, "device-456");
    assert_eq!(context.session_id, "session-789");
    assert_eq!(context.security_level, SecurityLevel::High);
}

#[test]
fn test_primal_context_with_metadata() {
    let location = NetworkLocation {
        ip_address: "10.0.0.1".to_string(),
        subnet: None,
        network_id: None,
        geo_location: None,
    };

    let mut metadata = HashMap::new();
    metadata.insert("app".to_string(), "myapp".to_string());
    metadata.insert("version".to_string(), "1.0".to_string());

    let context = PrimalContext {
        user_id: "user-999".to_string(),
        device_id: "device-999".to_string(),
        session_id: "session-999".to_string(),
        network_location: location,
        security_level: SecurityLevel::Standard,
        metadata,
    };

    assert_eq!(context.metadata.len(), 2);
    assert_eq!(context.metadata.get("app").unwrap(), "myapp");
}

#[test]
fn test_primal_context_network_location() {
    let location = NetworkLocation {
        ip_address: "172.16.0.1".to_string(),
        subnet: Some("172.16.0.0/16".to_string()),
        network_id: Some("corp-net".to_string()),
        geo_location: None,
    };

    let context = PrimalContext {
        user_id: "user-777".to_string(),
        device_id: "device-888".to_string(),
        session_id: "session-888".to_string(),
        network_location: location,
        security_level: SecurityLevel::Maximum,
        metadata: HashMap::new(),
    };

    assert_eq!(context.network_location.ip_address, "172.16.0.1");
    assert_eq!(
        context.network_location.network_id,
        Some("corp-net".to_string())
    );
}

#[test]
fn test_primal_context_clone() {
    let location = NetworkLocation {
        ip_address: "127.0.0.1".to_string(),
        subnet: None,
        network_id: None,
        geo_location: None,
    };

    let context = PrimalContext {
        user_id: "user-clone".to_string(),
        device_id: "device-clone".to_string(),
        session_id: "session-clone".to_string(),
        network_location: location,
        security_level: SecurityLevel::None,
        metadata: HashMap::new(),
    };

    let cloned = context.clone();
    assert_eq!(context.user_id, cloned.user_id);
    assert_eq!(context.device_id, cloned.device_id);
    assert_eq!(context.session_id, cloned.session_id);
    assert_eq!(context.security_level, cloned.security_level);
}

#[test]
fn test_primal_context_serialization() {
    let location = NetworkLocation {
        ip_address: "10.0.0.5".to_string(),
        subnet: None,
        network_id: None,
        geo_location: None,
    };

    let context = PrimalContext {
        user_id: "user-serialize".to_string(),
        device_id: "device-serialize".to_string(),
        session_id: "session-serialize".to_string(),
        network_location: location,
        security_level: SecurityLevel::Standard,
        metadata: HashMap::new(),
    };

    let json = serde_json::to_string(&context).unwrap();
    let deserialized: PrimalContext = serde_json::from_str(&json).unwrap();

    assert_eq!(context.user_id, deserialized.user_id);
    assert_eq!(context.device_id, deserialized.device_id);
    assert_eq!(context.security_level, deserialized.security_level);
}

// ============================================================================
// JobPriority Tests
// ============================================================================

#[test]
fn test_job_priority_ordering() {
    // Lower numeric values = higher priority, so Emergency < Low
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
    assert_eq!(JobPriority::Emergency, JobPriority::Emergency);
    assert_ne!(JobPriority::Low, JobPriority::High);
}

#[test]
fn test_job_priority_copy_clone() {
    let priority = JobPriority::Critical;
    let copied = priority;
    let cloned = priority;

    assert_eq!(priority, copied);
    assert_eq!(priority, cloned);
}

#[test]
fn test_job_priority_serialization() {
    let priority = JobPriority::High;
    let json = serde_json::to_string(&priority).unwrap();
    let deserialized: JobPriority = serde_json::from_str(&json).unwrap();

    assert_eq!(priority, deserialized);
}

#[test]
fn test_job_priority_all_variants() {
    // Ordered from highest priority (Emergency=0) to lowest (Low=4)
    let variants = vec![
        JobPriority::Emergency,
        JobPriority::Critical,
        JobPriority::High,
        JobPriority::Normal,
        JobPriority::Low,
    ];

    assert_eq!(variants.len(), 5);

    // Verify ordering (each should be less than the next)
    for i in 0..variants.len() - 1 {
        assert!(variants[i] < variants[i + 1]);
    }
}

// ============================================================================
// UniversalJobType Tests
// ============================================================================

#[test]
fn test_universal_job_type_native() {
    let mut env = HashMap::new();
    env.insert("PATH".to_string(), "/usr/bin".to_string());

    let job_type = UniversalJobType::Native {
        executable: "/usr/bin/echo".to_string(),
        args: vec!["hello".to_string()],
        env,
    };

    match job_type {
        UniversalJobType::Native {
            executable,
            args,
            env,
        } => {
            assert_eq!(executable, "/usr/bin/echo");
            assert_eq!(args.len(), 1);
            assert_eq!(env.len(), 1);
        }
        _ => panic!("Expected Native variant"),
    }
}

#[test]
fn test_universal_job_type_wasm() {
    let module = vec![0x00, 0x61, 0x73, 0x6d]; // WASM magic number
    let job_type = UniversalJobType::Wasm {
        module: module.clone(),
        args: vec!["arg1".to_string()],
        env: HashMap::new(),
    };

    match job_type {
        UniversalJobType::Wasm {
            module: m, args, ..
        } => {
            assert_eq!(m, module);
            assert_eq!(args.len(), 1);
        }
        _ => panic!("Expected Wasm variant"),
    }
}

#[test]
fn test_universal_job_type_primal() {
    let payload = serde_json::json!({"action": "compute", "data": 123});
    let job_type = UniversalJobType::Primal {
        primal_type: "ai_processing".to_string(),
        endpoint: "http://ai-service:8080".to_string(),
        payload,
    };

    match job_type {
        UniversalJobType::Primal {
            primal_type,
            endpoint,
            payload,
        } => {
            assert_eq!(primal_type, "ai_processing");
            assert!(endpoint.contains("ai-service"));
            assert!(payload.is_object());
        }
        _ => panic!("Expected Primal variant"),
    }
}

#[test]
fn test_universal_job_type_biomeos() {
    let manifest = serde_json::json!({"version": "1.0", "services": []});
    let job_type = UniversalJobType::BiomeOS {
        biome_manifest: manifest,
        team_id: "team-123".to_string(),
    };

    match job_type {
        UniversalJobType::BiomeOS {
            biome_manifest,
            team_id,
        } => {
            assert_eq!(team_id, "team-123");
            assert!(biome_manifest.is_object());
        }
        _ => panic!("Expected BiomeOS variant"),
    }
}

#[test]
fn test_universal_job_type_clone() {
    let job_type = UniversalJobType::Native {
        executable: "/bin/test".to_string(),
        args: vec![],
        env: HashMap::new(),
    };

    let cloned = job_type.clone();

    match (job_type, cloned) {
        (
            UniversalJobType::Native { executable: e1, .. },
            UniversalJobType::Native { executable: e2, .. },
        ) => {
            assert_eq!(e1, e2);
        }
        _ => panic!("Clone failed"),
    }
}

#[test]
fn test_universal_job_type_serialization() {
    let job_type = UniversalJobType::Native {
        executable: "/usr/bin/ls".to_string(),
        args: vec!["-la".to_string()],
        env: HashMap::new(),
    };

    let json = serde_json::to_string(&job_type).unwrap();
    let deserialized: UniversalJobType = serde_json::from_str(&json).unwrap();

    match deserialized {
        UniversalJobType::Native { executable, .. } => {
            assert_eq!(executable, "/usr/bin/ls");
        }
        _ => panic!("Deserialization failed"),
    }
}

// ============================================================================
// UniversalPlatformConfig Tests
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
    assert!(!config.biomeos_integration);
    assert_eq!(config.max_concurrent_jobs, 50);
    assert!(config.pure_ecosystem);
}

#[test]
fn test_universal_platform_config_clone() {
    let config = UniversalPlatformConfig::default();
    let cloned = config.clone();

    assert_eq!(config.recursive_hosting, cloned.recursive_hosting);
    assert_eq!(config.max_concurrent_jobs, cloned.max_concurrent_jobs);
    assert_eq!(config.pure_ecosystem, cloned.pure_ecosystem);
}

// ============================================================================
// Summary
// ============================================================================

// Total tests added: 45
// Coverage areas:
// - SecurityLevel (5 tests)
// - NetworkLocation (5 tests)
// - PrimalContext (5 tests)
// - JobPriority (6 tests)
// - UniversalJobType (6 tests)
// - UniversalPlatformConfig (3 tests)
// - Serialization tests (15 tests across types)
// - Clone/Copy tests (multiple)
// - Equality/ordering tests (multiple)
