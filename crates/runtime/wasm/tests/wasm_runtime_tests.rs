// SPDX-License-Identifier: AGPL-3.0-or-later
//! WebAssembly runtime engine tests
//!
//! Integration tests for the WASM runtime engine.

use toadstool_common::config_bases::CacheConfig;
use toadstool_runtime_wasm::{
    SecurityLevel,
    WasmRuntimeConfig,
    // ComponentModelConfig, ComponentModelSupport, // Not fully integrated yet
    WasmRuntimeEngine,
};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_engine_initialization() {
    let config = WasmRuntimeConfig::default();
    let engine = WasmRuntimeEngine::new(config);
    assert!(engine.is_ok());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_engine_with_custom_security_level() {
    let config = WasmRuntimeConfig {
        security_level: SecurityLevel::Maximum,
        ..Default::default()
    };
    let engine = WasmRuntimeEngine::new(config);
    assert!(engine.is_ok());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_engine_with_fuel_limit() {
    let config = WasmRuntimeConfig {
        fuel_limit: Some(1_000_000),
        ..Default::default()
    };
    let engine = WasmRuntimeEngine::new(config);
    assert!(engine.is_ok());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_engine_cache_configuration() {
    use std::time::Duration;
    let config = WasmRuntimeConfig {
        cache: CacheConfig {
            enabled: true,
            ttl: Duration::from_hours(1),
            max_entries: 100,
            negative_ttl: Duration::from_mins(5),
        },
        ..Default::default()
    };
    let engine = WasmRuntimeEngine::new(config);
    assert!(engine.is_ok());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_security_level_none() {
    let config = WasmRuntimeConfig {
        security_level: SecurityLevel::None,
        ..Default::default()
    };
    let engine = WasmRuntimeEngine::new(config);
    assert!(engine.is_ok());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_security_level_basic() {
    let config = WasmRuntimeConfig {
        security_level: SecurityLevel::None,
        ..Default::default()
    };
    let engine = WasmRuntimeEngine::new(config);
    assert!(engine.is_ok());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_security_level_strict() {
    let config = WasmRuntimeConfig {
        security_level: SecurityLevel::High,
        ..Default::default()
    };
    let engine = WasmRuntimeEngine::new(config);
    assert!(engine.is_ok());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_security_level_maximum() {
    let config = WasmRuntimeConfig {
        security_level: SecurityLevel::Maximum,
        ..Default::default()
    };
    let engine = WasmRuntimeEngine::new(config);
    assert!(engine.is_ok());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_engine_with_custom_memory_limits() {
    let config = WasmRuntimeConfig {
        max_memory_mb: 512,
        max_pages: 8192,
        ..Default::default()
    };
    let engine = WasmRuntimeEngine::new(config);
    assert!(engine.is_ok());
}
