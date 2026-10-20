// SPDX-License-Identifier: AGPL-3.0-or-later

use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[cfg_attr(
    not(feature = "slow-tests"),
    ignore = "slow integration test - runs full system detection"
)]
async fn test_quick_start() {
    // Test that quick_start doesn't panic
    let result = quick_start().await;

    // Should either succeed or fail gracefully
    match result {
        Ok(config) => {
            // Config should have some sensible defaults
            assert!(config.runtime.max_concurrent_executions > 0);
        }
        Err(e) => {
            // Errors should be informative
            assert!(!e.to_string().is_empty());
        }
    }
}

#[test]
fn test_quick_start_sync() {
    // Fast synchronous test of default config generation
    let config = toadstool_config::ToadStoolConfig::default();

    // Verify default config has sensible values
    assert!(config.runtime.max_concurrent_executions > 0);
    assert!(config.runtime.resource_limits.max_cpu_usage > 0.0);
    assert!(config.runtime.resource_limits.max_memory_usage > 0.0);
}

#[test]
fn test_config_builder_creation() {
    let builder = ConfigBuilder::new();

    assert!(builder.enable_hardware_detection);
    assert!(builder.enable_ecosystem_discovery);
    assert!(builder.enable_performance_optimization);
    assert!(builder.enable_usage_learning);
}

#[test]
fn test_config_builder_customization() {
    let builder = ConfigBuilder::new()
        .with_hardware_detection(false)
        .with_ecosystem_discovery(false)
        .with_performance_optimization(true)
        .with_usage_learning(true);

    assert!(!builder.enable_hardware_detection);
    assert!(!builder.enable_ecosystem_discovery);
    assert!(builder.enable_performance_optimization);
    assert!(builder.enable_usage_learning);
}

#[test]
fn test_config_builder_discovery_timeout() {
    let timeout = std::time::Duration::from_secs(10);
    let builder = ConfigBuilder::new().with_discovery_timeout(timeout);
    assert_eq!(builder.discovery_timeout, timeout);
}

#[test]
fn test_config_builder_default() {
    let builder = ConfigBuilder::default();
    assert!(builder.enable_hardware_detection);
    assert_eq!(
        builder.discovery_timeout,
        std::time::Duration::from_secs(30)
    );
}

#[test]
fn test_error_creation() {
    let config_error = ToadStoolError::configuration("test config error");
    assert!(config_error.to_string().contains("Configuration error"));

    let hardware_error = ToadStoolError::hardware("test hardware error");
    assert!(
        hardware_error
            .to_string()
            .contains("Hardware detection error")
    );

    let network_error = ToadStoolError::network("test network error");
    assert!(network_error.to_string().contains("Network error"));

    let ecosystem_error = ToadStoolError::ecosystem_discovery("discovery failed");
    assert!(
        ecosystem_error
            .to_string()
            .contains("Ecosystem discovery error")
    );

    let other_error = ToadStoolError::other("misc error");
    assert!(other_error.to_string().contains("Other error"));
}

#[test]
fn test_external_http_error() {
    let err = ToadStoolError::ExternalHttpNotSupported;
    assert!(err.to_string().contains("External HTTP"));
}

#[test]
fn test_toadstool_error_io_and_json_display() {
    let io_err: ToadStoolError = std::io::Error::other("disk full").into();
    let io_msg = io_err.to_string();
    assert!(io_msg.contains("IO error"), "got: {io_msg}");
    assert!(io_msg.contains("disk full"), "got: {io_msg}");

    let json_err = serde_json::from_str::<serde_json::Value>("not valid json").unwrap_err();
    let json_e: ToadStoolError = json_err.into();
    let json_msg = json_e.to_string();
    assert!(json_msg.contains("JSON"), "got: {json_msg}");
}

#[test]
fn test_config_builder_default_matches_new() {
    let d = ConfigBuilder::default();
    let n = ConfigBuilder::new();
    assert_eq!(d.enable_hardware_detection, n.enable_hardware_detection);
    assert_eq!(d.enable_ecosystem_discovery, n.enable_ecosystem_discovery);
    assert_eq!(
        d.enable_performance_optimization,
        n.enable_performance_optimization
    );
    assert_eq!(d.enable_usage_learning, n.enable_usage_learning);
    assert_eq!(d.discovery_timeout, n.discovery_timeout);
    assert!(d.enable_hardware_detection);
    assert!(d.enable_ecosystem_discovery);
    assert!(d.enable_performance_optimization);
    assert!(d.enable_usage_learning);
    assert_eq!(d.discovery_timeout, std::time::Duration::from_secs(30));
}

/// Chains `with_*` toggles (fine-grained “custom” build) and checks final flag state.
#[test]
fn test_config_builder_chained_with_methods() {
    let b = ConfigBuilder::new()
        .with_hardware_detection(true)
        .with_ecosystem_discovery(false)
        .with_performance_optimization(true)
        .with_usage_learning(false)
        .with_discovery_timeout(std::time::Duration::from_secs(7));
    assert!(b.enable_hardware_detection);
    assert!(!b.enable_ecosystem_discovery);
    assert!(b.enable_performance_optimization);
    assert!(!b.enable_usage_learning);
    assert_eq!(b.discovery_timeout, std::time::Duration::from_secs(7));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_config_builder_build_with_hardware_detection_disabled() {
    let builder = ConfigBuilder::new().with_hardware_detection(false);
    let result = builder.build().await;
    assert!(result.is_ok());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_config_builder_build_with_usage_learning_disabled() {
    let builder = ConfigBuilder::new().with_usage_learning(false);
    let result = builder.build().await;
    assert!(result.is_ok());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_config_builder_build_with_performance_optimization_disabled() {
    let builder = ConfigBuilder::new().with_performance_optimization(false);
    let result = builder.build().await;
    assert!(result.is_ok());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_system_summary_creation() {
    let capabilities = SystemCapabilities::default();
    let ecosystem = DiscoveredServices {
        discovered_services: std::collections::HashMap::new(),
        discovery_summary: ecosystem::DiscoverySummary::default(),
        discovery_timestamp: std::time::SystemTime::now(),
    };

    let summary = SystemSummary::from_capabilities(&capabilities, &ecosystem);

    assert!(!summary.cpu_info.is_empty());
    assert!(!summary.memory_info.is_empty());
    assert!(!summary.performance_class.is_empty());
}

#[test]
fn test_system_summary_display() {
    let summary = SystemSummary {
        cpu_info: "Test CPU (4 cores)".to_string(),
        memory_info: "8.0 GB".to_string(),
        gpu_info: "Integrated Graphics".to_string(),
        storage_info: "100.0 GB SSD".to_string(),
        ecosystem_services: vec!["coordination".to_string()],
        performance_class: "Mainstream".to_string(),
        optimal_runtimes: vec!["Native".to_string()],
    };

    // Test that display doesn't panic
    summary.display();
}

#[test]
fn test_system_summary_display_empty_ecosystem_services() {
    let summary = SystemSummary {
        cpu_info: "CPU".to_string(),
        memory_info: "1.0 GB".to_string(),
        gpu_info: "Integrated Graphics".to_string(),
        storage_info: "10.0 GB".to_string(),
        ecosystem_services: vec![],
        performance_class: "LowEnd".to_string(),
        optimal_runtimes: vec!["Native".to_string()],
    };
    summary.display();
}

#[test]
fn test_system_summary_from_capabilities_gpu_branch() {
    let capabilities = SystemCapabilities {
        gpu_count: 2,
        ..SystemCapabilities::default()
    };
    let ecosystem = DiscoveredServices {
        discovered_services: std::collections::HashMap::new(),
        discovery_summary: ecosystem::DiscoverySummary::default(),
        discovery_timestamp: std::time::SystemTime::now(),
    };
    let summary = SystemSummary::from_capabilities(&capabilities, &ecosystem);
    assert!(
        summary.gpu_info.contains("2 GPU"),
        "got: {}",
        summary.gpu_info
    );
}

#[test]
fn test_system_summary_from_capabilities_formatting() {
    let mut capabilities = SystemCapabilities::default();
    capabilities.cpu_info.model_name = "TestModel".to_string();
    capabilities.cpu_cores = 8.0;
    capabilities.memory_gb = 16.0;
    capabilities.performance_class = PerformanceClass::HighEnd;
    let ecosystem = DiscoveredServices {
        discovered_services: std::collections::HashMap::new(),
        discovery_summary: ecosystem::DiscoverySummary::default(),
        discovery_timestamp: std::time::SystemTime::now(),
    };
    let summary = SystemSummary::from_capabilities(&capabilities, &ecosystem);
    assert!(summary.cpu_info.contains("TestModel"));
    assert!(summary.cpu_info.contains('8'));
    assert!(summary.memory_info.contains("16.0"));
    assert!(summary.performance_class.contains("HighEnd"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_config_builder_build_with_all_disabled() {
    let builder = ConfigBuilder::new()
        .with_hardware_detection(false)
        .with_ecosystem_discovery(false)
        .with_performance_optimization(false)
        .with_usage_learning(false);
    let result = builder.build().await;
    assert!(result.is_ok());
    let config = result.unwrap();
    assert!(config.runtime.max_concurrent_executions > 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_config_builder_build_with_ecosystem_disabled() {
    let builder = ConfigBuilder::new().with_ecosystem_discovery(false);
    let result = builder.build().await;
    assert!(result.is_ok());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_get_system_summary() {
    let result = get_system_summary().await;
    assert!(result.is_ok());
    let summary = result.unwrap();
    assert!(!summary.cpu_info.is_empty());
    assert!(!summary.memory_info.is_empty());
    assert!(!summary.performance_class.is_empty());
}

/// Basic smoke: `get_system_summary` returns a summary usable for display.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_get_system_summary_basic_call() {
    let summary = get_system_summary().await.expect("get_system_summary");
    summary.display();
    assert!(!summary.gpu_info.is_empty());
    assert!(!summary.storage_info.is_empty());
}

#[test]
fn test_system_summary_from_capabilities_with_ecosystem_services() {
    let mut discovered = std::collections::HashMap::new();
    let test_endpoint = format!(
        "{}{}:{}",
        toadstool_common::constants::network::HTTP_PROTOCOL,
        toadstool_common::constants::network::LOCALHOST_IPV4,
        8080
    );
    discovered.insert(
        "coordination".to_string(),
        ServiceInfo {
            id: "coordination".to_string(),
            name: "coordination".to_string(),
            version: Some("1.0".to_string()),
            endpoints: vec![test_endpoint],
            capabilities: vec![],
            status: ecosystem::ServiceStatus::Running,
            metadata: Default::default(),
        },
    );
    let ecosystem = DiscoveredServices {
        discovered_services: discovered,
        discovery_summary: ecosystem::DiscoverySummary::default(),
        discovery_timestamp: std::time::SystemTime::now(),
    };
    let capabilities = SystemCapabilities::default();
    let summary = SystemSummary::from_capabilities(&capabilities, &ecosystem);
    assert_eq!(summary.ecosystem_services.len(), 1);
    assert!(
        summary
            .ecosystem_services
            .contains(&"coordination".to_string())
    );
}
