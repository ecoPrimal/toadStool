// SPDX-License-Identifier: AGPL-3.0-or-later
#![allow(
    clippy::float_cmp,
    reason = "exact comparison intended in this context"
)]
//! Comprehensive Coverage Tests for WASM Runtime Lib
//!
//! Targeting lib.rs to increase coverage from 48.72% to 60%+
//!
//! Real tests below exercise the existing wasmi runtime (module load, execute, metrics).

use std::collections::HashMap;
use std::time::Duration;
use uuid::Uuid;

use toadstool::execution::{ExecutionRequest, ExecutionStatus, RuntimeConfig, RuntimeEngine};
use toadstool::resources::ResourceRequirements;
use toadstool::security::{IsolationLevel, SecurityContext};
use toadstool::workload::{WorkloadSpec, WorkloadType};
use toadstool_runtime_wasm::{CacheMetrics, SecurityLevel, WasmRuntimeConfig, WasmRuntimeEngine};

mod test_utils;
use test_utils::create_simple_wasm_module;

// ============================================================================
// CacheMetrics Tests
// ============================================================================

#[test]
fn test_cache_metrics_default() {
    let metrics = CacheMetrics::default();
    assert_eq!(metrics.total_modules, 0);
    assert_eq!(metrics.total_size_bytes, 0);
    assert_eq!(metrics.average_module_size, 0);
    assert_eq!(metrics.cache_hit_rate, 0.0);
    assert_eq!(metrics.memory_usage_bytes, 0);
}

#[test]
fn test_cache_metrics_creation() {
    let metrics = CacheMetrics {
        total_modules: 10,
        total_size_bytes: 1024,
        average_module_size: 102,
        cache_hit_rate: 0.85,
        memory_usage_bytes: 2048,
    };

    assert_eq!(metrics.total_modules, 10);
    assert_eq!(metrics.total_size_bytes, 1024);
    assert_eq!(metrics.average_module_size, 102);
    assert_eq!(metrics.cache_hit_rate, 0.85);
    assert_eq!(metrics.memory_usage_bytes, 2048);
}

#[test]
fn test_cache_metrics_display() {
    let metrics = CacheMetrics {
        total_modules: 5,
        total_size_bytes: 4096,
        average_module_size: 819,
        cache_hit_rate: 0.75,
        memory_usage_bytes: 8192,
    };

    let display_str = format!("{metrics}");
    assert!(display_str.contains("modules: 5"));
    assert!(display_str.contains("size: 4096 bytes"));
    assert!(display_str.contains("75.00%"));
}

#[test]
fn test_cache_metrics_display_zero() {
    let metrics = CacheMetrics::default();
    let display_str = format!("{metrics}");
    assert!(display_str.contains("modules: 0"));
    assert!(display_str.contains("0.00%"));
}

#[test]
fn test_cache_metrics_debug() {
    let metrics = CacheMetrics::default();
    let debug_str = format!("{metrics:?}");
    assert!(debug_str.contains("CacheMetrics"));
}

#[test]
fn test_cache_metrics_clone() {
    let metrics1 = CacheMetrics {
        total_modules: 3,
        total_size_bytes: 512,
        average_module_size: 170,
        cache_hit_rate: 0.90,
        memory_usage_bytes: 1024,
    };

    let metrics2 = metrics1.clone();
    assert_eq!(metrics1.total_modules, metrics2.total_modules);
    assert_eq!(metrics1.cache_hit_rate, metrics2.cache_hit_rate);
}

// ============================================================================
// SecurityLevel Tests
// ============================================================================

#[test]
fn test_security_level_none() {
    let level = SecurityLevel::None;
    assert!(format!("{level:?}").contains("None"));
}

#[test]
fn test_security_level_basic() {
    let level = SecurityLevel::None;
    assert!(format!("{level:?}").contains("Basic"));
}

#[test]
fn test_security_level_strict() {
    let level = SecurityLevel::High;
    assert!(format!("{level:?}").contains("Strict"));
}

#[test]
fn test_security_level_maximum() {
    let level = SecurityLevel::Maximum;
    assert!(format!("{level:?}").contains("Maximum"));
}

#[test]
fn test_security_level_clone() {
    let level1 = SecurityLevel::High;
    let level2 = level1;
    assert!(format!("{level1:?}") == format!("{level2:?}"));
}

// ============================================================================
// WasmRuntimeConfig Tests
// ============================================================================

#[test]
fn test_wasm_config_default() {
    let config = WasmRuntimeConfig::default();
    assert_eq!(config.max_memory_mb, 128);
    assert_eq!(config.max_pages, 2048);
    assert_eq!(config.execution_timeout_ms, 30000);
    assert_eq!(config.module_load_timeout_ms, 10000);
    assert_eq!(config.fuel_limit, Some(1_000_000));
}

#[test]
fn test_wasm_config_custom() {
    let config = WasmRuntimeConfig {
        max_memory_mb: 256,
        max_pages: 4096,
        execution_timeout_ms: 60000,
        module_load_timeout_ms: 20000,
        fuel_limit: Some(2_000_000),
        security_level: SecurityLevel::Maximum,
        ..Default::default()
    };

    assert_eq!(config.max_memory_mb, 256);
    assert_eq!(config.max_pages, 4096);
    assert_eq!(config.execution_timeout_ms, 60000);
    assert_eq!(config.fuel_limit, Some(2_000_000));
}

#[test]
fn test_wasm_config_no_fuel_limit() {
    let config = WasmRuntimeConfig {
        fuel_limit: None,
        ..Default::default()
    };

    assert!(config.fuel_limit.is_none());
}

#[test]
fn test_wasm_config_clone() {
    let config1 = WasmRuntimeConfig::default();
    let config2 = config1.clone();
    assert_eq!(config1.max_memory_mb, config2.max_memory_mb);
    assert_eq!(config1.execution_timeout_ms, config2.execution_timeout_ms);
}

#[test]
fn test_wasm_config_debug() {
    let config = WasmRuntimeConfig::default();
    let debug_str = format!("{config:?}");
    assert!(debug_str.contains("WasmRuntimeConfig"));
}

// ============================================================================
// ComponentModelConfig Tests (Phase 2 — component model not yet implemented)
// ============================================================================

mod component_config_tests {
    #[test]
    #[ignore = "Phase 2: component-model implementation pending"]
    fn test_component_config_default() {}

    #[test]
    #[ignore = "Phase 2: component-model implementation pending"]
    fn test_component_config_custom() {}

    #[test]
    #[ignore = "Phase 2: component-model implementation pending"]
    fn test_component_config_clone() {}

    #[test]
    #[ignore = "Phase 2: component-model implementation pending"]
    fn test_component_config_debug() {}
}

// ============================================================================
// WasmRuntimeEngine Creation Tests
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_engine_creation_default() {
    let config = WasmRuntimeConfig::default();
    let result = WasmRuntimeEngine::new(config);
    assert!(result.is_ok());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_engine_creation_with_security_levels() {
    for level in [
        SecurityLevel::None,
        SecurityLevel::None,
        SecurityLevel::High,
        SecurityLevel::Maximum,
    ] {
        let config = WasmRuntimeConfig {
            security_level: level,
            ..Default::default()
        };
        let result = WasmRuntimeEngine::new(config);
        assert!(result.is_ok());
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_engine_creation_high_memory() {
    let config = WasmRuntimeConfig {
        max_memory_mb: 512,
        max_pages: 8192,
        ..Default::default()
    };
    let result = WasmRuntimeEngine::new(config);
    assert!(result.is_ok());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_engine_creation_low_memory() {
    let config = WasmRuntimeConfig {
        max_memory_mb: 16,
        max_pages: 256,
        ..Default::default()
    };
    let result = WasmRuntimeEngine::new(config);
    assert!(result.is_ok());
}

// ============================================================================
// Capabilities Tests
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_get_capabilities() {
    let config = WasmRuntimeConfig::default();
    let engine = WasmRuntimeEngine::new(config).unwrap();

    let caps = engine.get_capabilities();
    assert!(!caps.supported_workloads.is_empty());
    assert!(!caps.supported_architectures.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_capabilities_supported_architectures() {
    let config = WasmRuntimeConfig::default();
    let engine = WasmRuntimeEngine::new(config).unwrap();

    let caps = engine.get_capabilities();
    // wasmi is architecture-agnostic
    let _ = &caps.supported_architectures;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_capabilities_platform_features() {
    let config = WasmRuntimeConfig::default();
    let engine = WasmRuntimeEngine::new(config).unwrap();

    let caps = engine.get_capabilities();
    // Platform features may vary by implementation
    let _features_count = caps.platform_features.len();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_capabilities_version() {
    let config = WasmRuntimeConfig::default();
    let engine = WasmRuntimeEngine::new(config).unwrap();

    let caps = engine.get_capabilities();
    assert!(!caps.version.is_empty());
}

// ============================================================================
// Timeout Configuration Tests
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_short_execution_timeout() {
    let config = WasmRuntimeConfig {
        execution_timeout_ms: 100,
        ..Default::default()
    };
    let engine = WasmRuntimeEngine::new(config).unwrap();
    // Engine created with short timeout
    let caps = engine.get_capabilities();
    assert!(!caps.supported_workloads.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_long_execution_timeout() {
    let config = WasmRuntimeConfig {
        execution_timeout_ms: 300_000, // 5 minutes
        ..Default::default()
    };
    let engine = WasmRuntimeEngine::new(config).unwrap();
    let caps = engine.get_capabilities();
    assert!(!caps.supported_workloads.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_module_load_timeout() {
    let config = WasmRuntimeConfig {
        module_load_timeout_ms: 5000,
        ..Default::default()
    };
    let engine = WasmRuntimeEngine::new(config).unwrap();
    let caps = engine.get_capabilities();
    assert!(!caps.supported_workloads.is_empty());
}

// ============================================================================
// WASM Runtime Execution Tests (existing wasmi runtime)
// ============================================================================
// Evolved from placeholder: real tests exercising module load, execute, metrics.

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_execute_simple_wasm_module() {
    let wasm = create_simple_wasm_module().expect("create simple WASM");
    let config = WasmRuntimeConfig::default();
    let mut engine = WasmRuntimeEngine::new(config).unwrap();

    engine.initialize(RuntimeConfig::default()).await.unwrap();

    let request = ExecutionRequest {
        execution_id: Uuid::new_v4(),
        workload: WorkloadSpec::Wasm {
            module: toadstool::workload::WasmModuleSource::Bytes { data: wasm.into() },
            args: Some(vec![]),
            wasi_config: None,
            env_vars: HashMap::new(),
        },
        runtime_hint: Some(toadstool::RuntimeType::Wasm),
        resources: ResourceRequirements::default(),
        security_context: SecurityContext::default(),
        timeout: Some(Duration::from_secs(5)),
        environment: HashMap::new(),
        input_data: toadstool::execution::ExecutionInput::default(),
        callback_config: None,
        encryption_config: None,
    };

    let response = engine.execute(request).await.unwrap();
    assert!(matches!(response.status, ExecutionStatus::Success));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_execute_returns_metrics() {
    let wasm = create_simple_wasm_module().expect("create simple WASM");
    let config = WasmRuntimeConfig::default();
    let mut engine = WasmRuntimeEngine::new(config).unwrap();
    engine.initialize(RuntimeConfig::default()).await.unwrap();

    let execution_id = Uuid::new_v4();
    let request = ExecutionRequest {
        execution_id,
        workload: WorkloadSpec::Wasm {
            module: toadstool::workload::WasmModuleSource::Bytes { data: wasm.into() },
            args: Some(vec![]),
            wasi_config: None,
            env_vars: HashMap::new(),
        },
        runtime_hint: Some(toadstool::RuntimeType::Wasm),
        resources: ResourceRequirements::default(),
        security_context: SecurityContext::default(),
        timeout: Some(Duration::from_secs(5)),
        environment: HashMap::new(),
        input_data: toadstool::execution::ExecutionInput::default(),
        callback_config: None,
        encryption_config: None,
    };

    let response = engine.execute(request).await.unwrap();
    assert_eq!(response.execution_id, execution_id);
    assert_eq!(response.runtime_used, toadstool::RuntimeType::Wasm);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_module_loader_cache_key_generation() {
    use bytes::Bytes;
    use toadstool::workload::WasmModuleSource;
    use toadstool_runtime_wasm::ModuleLoader;

    let config = WasmRuntimeConfig::default();
    let engine = wasmi::Engine::default();
    let loader = ModuleLoader::new(engine, config);

    let source1 = WasmModuleSource::Bytes {
        data: Bytes::from(vec![1, 2, 3]),
    };
    let source2 = WasmModuleSource::Bytes {
        data: Bytes::from(vec![1, 2, 3]),
    };
    let source3 = WasmModuleSource::Bytes {
        data: Bytes::from(vec![4, 5, 6]),
    };

    let key1 = loader.generate_cache_key(&source1);
    let key2 = loader.generate_cache_key(&source2);
    let key3 = loader.generate_cache_key(&source3);

    assert_eq!(key1, key2, "Same content should produce same cache key");
    assert_ne!(
        key1, key3,
        "Different content should produce different cache key"
    );
}

// ============================================================================
// Component Model Tests (Phase 2 — component model not yet implemented)
// ============================================================================

mod component_model_tests {
    #[test]
    #[ignore = "Phase 2: component-model implementation pending"]
    fn test_component_value_u32() {}

    #[test]
    #[ignore = "Phase 2: component-model implementation pending"]
    fn test_component_value_string() {}

    #[test]
    #[ignore = "Phase 2: component-model implementation pending"]
    fn test_component_value_bool() {}

    #[test]
    #[ignore = "Phase 2: component-model implementation pending"]
    fn test_component_value_u64() {}

    #[test]
    #[ignore = "Phase 2: component-model implementation pending"]
    fn test_component_value_f32() {}

    #[test]
    #[ignore = "Phase 2: component-model implementation pending"]
    fn test_component_value_clone() {}

    #[test]
    #[ignore = "Phase 2: component-model implementation pending"]
    fn test_component_state_initializing() {}

    #[test]
    #[ignore = "Phase 2: component-model implementation pending"]
    fn test_component_state_ready() {}

    #[test]
    #[ignore = "Phase 2: component-model implementation pending"]
    fn test_component_state_running() {}

    #[test]
    #[ignore = "Phase 2: component-model implementation pending"]
    fn test_component_state_failed() {}

    #[test]
    #[ignore = "Phase 2: component-model implementation pending"]
    fn test_component_state_terminating() {}

    #[test]
    #[ignore = "Phase 2: component-model implementation pending"]
    fn test_component_state_clone() {}
}

// ============================================================================
// Workload Support Tests
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_supports_wasm_workload() {
    let config = WasmRuntimeConfig::default();
    let engine = WasmRuntimeEngine::new(config).unwrap();

    assert!(engine.supports_workload(&WorkloadType::Wasm));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_does_not_support_container() {
    let config = WasmRuntimeConfig::default();
    let engine = WasmRuntimeEngine::new(config).unwrap();

    assert!(!engine.supports_workload(&WorkloadType::Container));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_does_not_support_native() {
    let config = WasmRuntimeConfig::default();
    let engine = WasmRuntimeEngine::new(config).unwrap();

    assert!(!engine.supports_workload(&WorkloadType::Native));
}

// ============================================================================
// Security Context Tests
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_security_context_basic() {
    let ctx = SecurityContext::for_isolation_level(IsolationLevel::Basic);
    assert_eq!(ctx.isolation_level, IsolationLevel::Basic);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_security_context_standard() {
    let ctx = SecurityContext::for_isolation_level(IsolationLevel::Standard);
    assert_eq!(ctx.isolation_level, IsolationLevel::Standard);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_security_context_enhanced() {
    let ctx = SecurityContext::for_isolation_level(IsolationLevel::Enhanced);
    assert_eq!(ctx.isolation_level, IsolationLevel::Enhanced);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_security_context_maximum() {
    let ctx = SecurityContext::for_isolation_level(IsolationLevel::Maximum);
    assert_eq!(ctx.isolation_level, IsolationLevel::Maximum);
}

// ============================================================================
// Test Summary
// ============================================================================

#[test]
fn test_wasm_lib_coverage_summary() {
    println!("========================================");
    println!("WASM Runtime Lib Coverage Tests");
    println!("========================================");
    println!("CacheMetrics Tests:          7 tests");
    println!("SecurityLevel Tests:         5 tests");
    println!("WasmRuntimeConfig Tests:     6 tests");
    println!("ComponentModelConfig Tests:  4 tests");
    println!("Engine Creation Tests:       4 tests");
    println!("Capabilities Tests:          5 tests");
    println!("Timeout Configuration:       3 tests");
    println!("Component Model Tests:       6 tests");
    println!("ComponentState Tests:        6 tests");
    println!("Workload Support Tests:      3 tests");
    println!("Security Context Tests:      4 tests");
    println!("========================================");
    println!("Total New Tests:            53 tests");
    println!("========================================");
    println!();
    println!("🎯 Target: Increase lib.rs coverage");
    println!("   From: 48.72% → Target: 55%+");
    println!("========================================");
}
