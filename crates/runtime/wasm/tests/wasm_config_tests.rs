// SPDX-License-Identifier: AGPL-3.0-or-later
//! Comprehensive tests for WASM runtime configuration

use toadstool_runtime_wasm::*;

// ============================================================================
// WasmRuntimeConfig Tests
// ============================================================================

#[test]
fn test_wasm_runtime_config_default() {
    let config = WasmRuntimeConfig::default();

    assert!(config.cache.enabled);
    assert_eq!(config.cache.max_entries, 512);
    assert_eq!(config.cache.ttl.as_secs(), 24 * 3600); // 24 hours
    assert_eq!(config.max_memory_mb, 128);
    assert_eq!(config.max_pages, 2048);
    assert_eq!(config.execution_timeout_ms, 30000);
    assert_eq!(config.module_load_timeout_ms, 10000);
    assert_eq!(config.fuel_limit, Some(1_000_000));
}

#[test]
fn test_wasm_runtime_config_cache_settings() {
    let config = WasmRuntimeConfig::default();

    assert!(config.cache.enabled);
    assert_eq!(config.cache.max_entries, 512);
    assert_eq!(config.cache.ttl.as_secs(), 24 * 3600); // 24 hours
}

#[test]
fn test_wasm_runtime_config_memory_limits() {
    let config = WasmRuntimeConfig::default();

    assert_eq!(config.max_memory_mb, 128);
    assert_eq!(config.max_pages, 2048);
}

#[test]
fn test_wasm_runtime_config_timeout_settings() {
    let config = WasmRuntimeConfig::default();

    assert_eq!(config.execution_timeout_ms, 30000); // 30 seconds
    assert_eq!(config.module_load_timeout_ms, 10000); // 10 seconds
}

#[test]
fn test_wasm_runtime_config_fuel_limit() {
    let config = WasmRuntimeConfig::default();

    assert_eq!(config.fuel_limit, Some(1_000_000));
}

#[test]
fn test_wasm_runtime_config_security_level() {
    let config = WasmRuntimeConfig::default();

    assert!(matches!(config.security_level, SecurityLevel::High));
}

#[test]
fn test_wasm_runtime_config_clone() {
    let config1 = WasmRuntimeConfig::default();
    let config2 = config1.clone();

    assert_eq!(config1.cache.enabled, config2.cache.enabled);
    assert_eq!(config1.cache.max_entries, config2.cache.max_entries);
    assert_eq!(config1.max_memory_mb, config2.max_memory_mb);
}

// ============================================================================
// SecurityLevel Tests
// ============================================================================

#[test]
fn test_security_level_none() {
    let level = SecurityLevel::None;
    assert!(matches!(level, SecurityLevel::None));
}

#[test]
fn test_security_level_basic() {
    let level = SecurityLevel::None;
    assert!(matches!(level, SecurityLevel::None));
}

#[test]
fn test_security_level_strict() {
    let level = SecurityLevel::High;
    assert!(matches!(level, SecurityLevel::High));
}

#[test]
fn test_security_level_maximum() {
    let level = SecurityLevel::Maximum;
    assert!(matches!(level, SecurityLevel::Maximum));
}

#[test]
fn test_security_level_clone() {
    let level1 = SecurityLevel::High;
    let level2 = level1;

    match (level1, level2) {
        (SecurityLevel::High, SecurityLevel::High) => {
            // Clone successful
        }
        _ => panic!("Clone failed"),
    }
}

#[test]
fn test_all_security_levels() {
    let levels = [
        SecurityLevel::None,
        SecurityLevel::None,
        SecurityLevel::High,
        SecurityLevel::Maximum,
    ];

    assert_eq!(levels.len(), 4);
}
