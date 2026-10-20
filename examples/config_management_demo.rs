// SPDX-License-Identifier: AGPL-3.0-or-later
#![allow(
    clippy::unused_async,
    dead_code,
    deprecated, // This example demonstrates legacy configuration APIs
    unused_variables,
)]
//! Configuration Management Demo
//!
//! This demo showcases ToadStool's comprehensive environment variable configuration system
//! that eliminates hardcoded values and provides configurable deployment options.
//!
//! ## Running the Demo
//!
//! ```bash
//! # Run with default values
//! cargo run --example config_management_demo
//!
//! # Run with custom environment variables
//! TOADSTOOL_ENV=production \
//! TOADSTOOL_SONGBIRD_PORT=9080 \
//! TOADSTOOL_DEBUG=true \
//! TOADSTOOL_MAX_CPU_PERCENT=80 \
//! cargo run --example config_management_demo
//!
//! # Run with a complete production configuration
//! TOADSTOOL_ENV=production \
//! TOADSTOOL_BIND_ADDRESS=0.0.0.0 \
//! TOADSTOOL_TOADSTOOL_PORT=8080 \
//! TOADSTOOL_SONGBIRD_PORT=8081 \
//! TOADSTOOL_BEARDOG_PORT=8082 \
//! TOADSTOOL_NESTGATE_PORT=8083 \
//! TOADSTOOL_SQUIRREL_PORT=8084 \
//! TOADSTOOL_TLS_ENABLED=true \
//! TOADSTOOL_AUTH_ENABLED=true \
//! TOADSTOOL_METRICS_ENABLED=true \
//! TOADSTOOL_LOG_LEVEL=info \
//! TOADSTOOL_MAX_CPU_PERCENT=75 \
//! TOADSTOOL_MAX_MEMORY_BYTES=17179869184 \
//! TOADSTOOL_WORKER_THREADS=16 \
//! cargo run --example config_management_demo
//! ```

use std::collections::HashMap;
use std::time::Duration;
use tracing::{info, warn};

use toadstool_common::primal_discovery::{DiscoveryConfig, PrimalDiscovery};
use toadstool_config::{
    config_utils::ConfigUtils,
    env_config::{
        EnvironmentConfig, MonitoringEnvConfig, NetworkEnvConfig, ResourceEnvConfig,
        SecurityEnvConfig,
    },
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    info!("🚀 ToadStool Configuration Management Demo");
    info!("===========================================");

    // Demonstrate the old way (hardcoded values)
    demonstrate_old_hardcoded_approach();

    // Demonstrate the new way (environment-aware configuration)
    demonstrate_new_environment_aware_approach().await?;

    // Demonstrate different environment configurations
    demonstrate_environment_specific_configs().await?;

    // Demonstrate configuration validation and error handling
    demonstrate_configuration_validation().await?;

    // Demonstrate service discovery with configurable endpoints
    demonstrate_service_discovery().await?;

    // Demonstrate resource limit configuration
    demonstrate_resource_configuration().await?;

    // Show all current configuration values
    demonstrate_configuration_inspection();

    info!("✅ Configuration Management Demo completed successfully!");
    Ok(())
}

/// Demonstrate the old hardcoded approach (anti-pattern — what we're replacing)
///
/// wateringHole standards: No hardcoded primal names or ports in production.
/// Primals discover each other at runtime via capabilities.
fn demonstrate_old_hardcoded_approach() {
    info!("📚 OLD APPROACH (Anti-Pattern): Hardcoded Primal Names & Ports");
    info!("============================================================");

    // ❌ ANTI-PATTERN: Hardcoded primal names and ports (violates wateringHole standards)
    // Production code must NOT do this — use capability-based discovery instead.
    let orchestration_port = 8080u16;
    let security_port = 8081u16;
    let storage_port = 8082u16;
    let localhost = "127.0.0.1";
    let max_cpu = 90.0f64;
    let max_memory = 8 * 1024 * 1024 * 1024u64;
    let request_timeout = Duration::from_secs(30);

    info!(
        "❌ Hardcoded orchestration port (primal-name coupling): {}",
        orchestration_port
    );
    info!(
        "❌ Hardcoded security port (primal-name coupling): {}",
        security_port
    );
    info!(
        "❌ Hardcoded storage port (primal-name coupling): {}",
        storage_port
    );
    info!("❌ Hardcoded localhost: {}", localhost);
    info!("❌ Hardcoded max CPU: {}%", max_cpu);
    info!("❌ Hardcoded max memory: {} bytes", max_memory);
    info!("❌ Hardcoded request timeout: {:?}", request_timeout);

    warn!("🚨 Problems with hardcoded primal names/ports:");
    warn!("   - Violates capability-based discovery (wateringHole/UNIVERSAL_IPC)");
    warn!("   - Cannot adapt to different environments");
    warn!("   - No runtime discovery — compile-time coupling to specific primals");
    warn!("   - Port conflicts in deployments");
    warn!("   - Not suitable for production");

    println!();
}

/// Demonstrate the new capability-based discovery approach (wateringHole standard)
///
/// Uses `ipc.find_capability` pattern: discover primals by capability at runtime.
/// Ports/endpoints come from configuration (env vars) or runtime discovery, never hardcoded.
async fn demonstrate_new_environment_aware_approach() -> Result<(), Box<dyn std::error::Error>> {
    info!("✨ NEW APPROACH: Capability-Based Discovery");
    info!("==========================================");

    // Build discovery fallbacks from env vars (TOADSTOOL_COORDINATION_URL, etc.)
    // or capability_fallback ports — config-driven, not hardcoded primal names.
    let bind_host = ConfigUtils::get_bind_address();
    let fallbacks = build_capability_fallbacks_from_config(&bind_host);

    let config = DiscoveryConfig {
        fallbacks: fallbacks.clone(),
        ..Default::default()
    };
    let discovery = PrimalDiscovery::with_config(config)?;

    // Discover by capability — no primal names! Use capability names: orchestration, security, storage
    info!("🔍 Discovering primals by capability (ipc.find_capability pattern):");

    for capability in ["orchestration", "security", "storage"] {
        match discovery.find_capability(capability).await {
            Ok(endpoint) => {
                info!(
                    "   ✅ {} capability: {} (discovered via {:?})",
                    capability,
                    endpoint.url(),
                    endpoint.discovered_via
                );
            }
            Err(e) => {
                info!("   ⚠️ {} capability: not found ({e})", capability);
            }
        }
    }

    let bind_address = ConfigUtils::get_bind_address();
    let max_cpu = ConfigUtils::get_max_cpu_usage();
    let max_memory = ConfigUtils::get_max_memory_usage();
    let request_timeout = ConfigUtils::get_request_timeout();

    info!("✅ Config-driven bind address: {}", bind_address);
    info!("✅ Config-driven max CPU: {}%", max_cpu);
    info!("✅ Config-driven max memory: {} bytes", max_memory);
    info!("✅ Config-driven request timeout: {:?}", request_timeout);

    // Self-knowledge: ToadStool's own endpoint (sovereignty)
    let endpoints = ConfigUtils::get_service_endpoints();
    info!("🔗 Self-endpoints (toadstool only):");
    for (service, endpoint) in endpoints {
        info!("   {}: {}", service, endpoint);
    }

    info!("🎯 Benefits of capability-based discovery:");
    info!("   ✅ Zero hardcoded primal names — discover by capability");
    info!("   ✅ Ports from config/env or runtime discovery");
    info!("   ✅ Adapts to different environments");
    info!("   ✅ Resolves transport at runtime (wateringHole/UNIVERSAL_IPC)");

    println!();
    Ok(())
}

/// Build capability fallbacks from env vars or config — no hardcoded primal names.
/// Uses `TOADSTOOL_COORDINATION_URL`, `TOADSTOOL_SECURITY_URL`, `TOADSTOOL_STORAGE_URL`
/// or `capability_fallback` ports with bind host.
fn build_capability_fallbacks_from_config(bind_host: &str) -> HashMap<String, String> {
    use toadstool_config::ports::capability_fallback;
    let mut fallbacks = HashMap::new();
    let specs: &[(&str, &[&str], u16)] = &[
        (
            "TOADSTOOL_COORDINATION_URL",
            &["orchestration", "coordination"][..],
            capability_fallback::COORDINATION,
        ),
        (
            "TOADSTOOL_SECURITY_URL",
            &["security"][..],
            capability_fallback::SECURITY,
        ),
        (
            "TOADSTOOL_STORAGE_URL",
            &["storage"][..],
            capability_fallback::STORAGE,
        ),
    ];
    for (env_var, capability_keys, port) in specs {
        let url = std::env::var(env_var).unwrap_or_else(|_| format!("http://{bind_host}:{port}"));
        for key in *capability_keys {
            fallbacks.insert((*key).to_string(), url.clone());
        }
    }
    fallbacks
}

/// Demonstrate different environment-specific configurations
async fn demonstrate_environment_specific_configs() -> Result<(), Box<dyn std::error::Error>> {
    info!("🌍 Environment-Specific Configuration Examples");
    info!("=============================================");

    // Show current environment
    let current_env = ConfigUtils::get_environment();
    info!("🏷️  Current environment: {}", current_env);

    // Show different environment configurations
    info!("📋 Configuration examples for different environments:");

    println!();
    info!("🛠️  DEVELOPMENT Environment:");
    info!("   TOADSTOOL_ENV=development");
    info!("   TOADSTOOL_DEBUG=true");
    info!("   TOADSTOOL_LOG_LEVEL=debug");
    info!("   TOADSTOOL_BIND_ADDRESS=127.0.0.1");
    info!("   TOADSTOOL_TLS_ENABLED=false");
    info!("   TOADSTOOL_AUTH_ENABLED=false");
    info!("   TOADSTOOL_METRICS_ENABLED=true");

    println!();
    info!("🧪 STAGING Environment:");
    info!("   TOADSTOOL_ENV=staging");
    info!("   TOADSTOOL_DEBUG=false");
    info!("   TOADSTOOL_LOG_LEVEL=info");
    info!("   TOADSTOOL_BIND_ADDRESS=0.0.0.0");
    info!("   TOADSTOOL_TLS_ENABLED=true");
    info!("   TOADSTOOL_AUTH_ENABLED=true");
    info!("   TOADSTOOL_METRICS_ENABLED=true");
    info!("   TOADSTOOL_MAX_CPU_PERCENT=80");

    println!();
    info!("🚀 PRODUCTION Environment:");
    info!("   TOADSTOOL_ENV=production");
    info!("   TOADSTOOL_DEBUG=false");
    info!("   TOADSTOOL_LOG_LEVEL=warn");
    info!("   TOADSTOOL_BIND_ADDRESS=0.0.0.0");
    info!("   TOADSTOOL_TLS_ENABLED=true");
    info!("   TOADSTOOL_AUTH_ENABLED=true");
    info!("   TOADSTOOL_SANDBOXING_ENABLED=true");
    info!("   TOADSTOOL_METRICS_ENABLED=true");
    info!("   TOADSTOOL_MAX_CPU_PERCENT=70");
    info!("   TOADSTOOL_MAX_MEMORY_BYTES=17179869184");
    info!("   TOADSTOOL_WORKER_THREADS=32");

    println!();
    Ok(())
}

/// Demonstrate configuration validation and error handling
async fn demonstrate_configuration_validation() -> Result<(), Box<dyn std::error::Error>> {
    info!("🔍 Configuration Validation & Error Handling");
    info!("===========================================");

    // Test various configuration scenarios
    info!("🧪 Testing configuration validation...");

    // Test valid configuration (capability bootstrap ports — prefer capability discovery in production)
    let network_config = NetworkEnvConfig::from_env();
    info!("✅ Network configuration loaded successfully");
    info!(
        "   Coordination bootstrap port: {}",
        network_config.coordination_port
    );
    info!(
        "   Security bootstrap port: {}",
        network_config.security_port
    );
    info!("   TLS enabled: {}", network_config.tls_enabled);

    // Test resource configuration
    let resource_config = ResourceEnvConfig::from_env();
    info!("✅ Resource configuration loaded successfully");
    info!("   Max CPU: {}%", resource_config.max_cpu_percent);
    info!("   Max Memory: {} bytes", resource_config.max_memory_bytes);
    info!("   Worker threads: {}", resource_config.worker_threads);

    // Test monitoring configuration
    let monitoring_config = MonitoringEnvConfig::from_env();
    info!("✅ Monitoring configuration loaded successfully");
    info!("   Metrics enabled: {}", monitoring_config.metrics_enabled);
    info!("   Log level: {}", monitoring_config.log_level);
    info!(
        "   Health checks enabled: {}",
        monitoring_config.health_checks_enabled
    );

    // Test security configuration
    let security_config = SecurityEnvConfig::from_env();
    info!("✅ Security configuration loaded successfully");
    info!("   Auth enabled: {}", security_config.auth_enabled);
    info!(
        "   Sandboxing enabled: {}",
        security_config.sandboxing_enabled
    );
    info!(
        "   Encryption enabled: {}",
        security_config.encryption_enabled
    );

    // Test comprehensive configuration
    let env_config = EnvironmentConfig::from_env();
    info!("✅ Complete environment configuration loaded successfully");
    info!("   Environment: {}", env_config.environment);
    info!("   Debug mode: {}", env_config.debug);
    info!("   Data directory: {}", env_config.data_dir.display());

    println!();
    Ok(())
}

/// Demonstrate capability-based service discovery (wateringHole pattern)
///
/// Primals discover each other by capability at runtime. Endpoints come from
/// config/env or mDNS — never hardcoded.
async fn demonstrate_service_discovery() -> Result<(), Box<dyn std::error::Error>> {
    info!("🔍 Capability-Based Service Discovery");
    info!("=====================================");

    let fallbacks = build_capability_fallbacks_from_config(&ConfigUtils::get_bind_address());
    let config = DiscoveryConfig {
        fallbacks,
        ..Default::default()
    };
    let discovery = PrimalDiscovery::with_config(config)?;

    let capabilities = ["orchestration", "security", "storage"];
    info!("🔗 Discovering by capability (not by primal name):");
    for cap in capabilities {
        match discovery.find_capability(cap).await {
            Ok(endpoint) => {
                info!("   📡 {} capability:", cap);
                info!("      Endpoint: {}", endpoint.url());
                info!(
                    "      Status: {}",
                    simulate_service_check(endpoint.url()).await
                );
            }
            Err(e) => info!("   ⚠️ {} capability: not found ({e})", cap),
        }
    }

    // Port ranges from config (for container allocation)
    let (container_start, container_end) = ConfigUtils::get_container_port_range();
    let (port_start, port_end) = ConfigUtils::get_port_allocation_range();

    info!(
        "🐳 Container port range (config): {} - {}",
        container_start, container_end
    );
    info!(
        "⚙️  Port allocation range (config): {} - {}",
        port_start, port_end
    );

    println!();
    Ok(())
}

/// Demonstrate resource limit configuration
async fn demonstrate_resource_configuration() -> Result<(), Box<dyn std::error::Error>> {
    info!("💾 Resource Configuration Management");
    info!("===================================");

    // Show resource limits
    let max_cpu = ConfigUtils::get_max_cpu_usage();
    let max_memory = ConfigUtils::get_max_memory_usage();
    let max_storage = ConfigUtils::get_max_storage_usage();
    let worker_threads = ConfigUtils::get_worker_threads();
    let max_concurrent = ConfigUtils::get_max_concurrent_executions();

    info!("📊 Resource limits:");
    info!("   🔢 Max CPU usage: {}%", max_cpu);
    info!(
        "   🧠 Max memory usage: {} bytes ({} GB)",
        max_memory,
        max_memory / (1024 * 1024 * 1024)
    );
    info!(
        "   💾 Max storage usage: {} bytes ({} GB)",
        max_storage,
        max_storage / (1024 * 1024 * 1024)
    );
    info!("   🧵 Worker threads: {}", worker_threads);
    info!("   ⚡ Max concurrent executions: {}", max_concurrent);

    // Show timeout configurations
    let request_timeout = ConfigUtils::get_request_timeout();
    let connection_timeout = ConfigUtils::get_connection_timeout();
    let execution_timeout = ConfigUtils::get_execution_timeout();

    info!("⏱️  Timeout configurations:");
    info!("   🌐 Request timeout: {:?}", request_timeout);
    info!("   🔌 Connection timeout: {:?}", connection_timeout);
    info!("   ⚡ Execution timeout: {:?}", execution_timeout);

    // Show monitoring intervals
    let metrics_interval = ConfigUtils::get_metrics_interval();
    let health_check_interval = ConfigUtils::get_health_check_interval();

    info!("📈 Monitoring intervals:");
    info!("   📊 Metrics collection: {:?}", metrics_interval);
    info!("   🏥 Health check: {:?}", health_check_interval);

    println!();
    Ok(())
}

/// Show all current configuration values
fn demonstrate_configuration_inspection() {
    info!("🔍 Complete Configuration Inspection");
    info!("===================================");

    // Show all TOADSTOOL environment variables
    let env_vars = ConfigUtils::get_all_toadstool_env_vars();
    if env_vars.is_empty() {
        info!("ℹ️  No TOADSTOOL environment variables set (using defaults)");
    } else {
        info!("🌍 Current TOADSTOOL environment variables:");
        for (key, value) in env_vars {
            info!("   {}: {}", key, value);
        }
    }

    println!();
}

/// Simulate a service health check (endpoint from discovery — no hardcoded ports)
async fn simulate_service_check(_endpoint: &str) -> &'static str {
    // In production, this would make an HTTP request to the discovered endpoint.
    // No hardcoded port checks — we use the URL resolved at runtime.
    "🟢 Discovered"
}

/// Demonstrate configuration file loading (bonus feature)
fn demonstrate_configuration_file_loading() {
    info!("📄 Configuration File Loading");
    info!("=============================");

    // Show how environment variables can be combined with config files
    info!("🔄 Loading order (highest to lowest priority):");
    info!("   1. Environment variables (TOADSTOOL_*)");
    info!("   2. Command line arguments");
    info!("   3. Configuration files (.env, toadstool.toml)");
    info!("   4. Default values");

    info!("📝 Example .env file (capability-based, no primal names):");
    info!("   TOADSTOOL_ENV=production");
    info!("   TOADSTOOL_DEBUG=false");
    info!("   TOADSTOOL_COORDINATION_URL=http://localhost:8080");
    info!("   TOADSTOOL_SECURITY_URL=http://localhost:8081");
    info!("   TOADSTOOL_STORAGE_URL=http://localhost:8082");
    info!("   TOADSTOOL_TLS_ENABLED=true");

    info!("📝 Example toadstool.toml file:");
    info!("   [network]");
    info!("   bind_address = \"0.0.0.0\"");
    info!("   tls_enabled = true");
    info!("   [security]");
    info!("   auth_enabled = true");
    info!("   sandboxing_enabled = true");

    println!();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_config_management_demo() {
        temp_env::with_vars(
            [
                ("TOADSTOOL_ENV", Some("test")),
                ("TOADSTOOL_COORDINATION_URL", Some("http://localhost:9080")),
                ("TOADSTOOL_DEBUG", Some("true")),
            ],
            || {
                assert_eq!(ConfigUtils::get_environment(), "test");
                assert!(ConfigUtils::get_debug_mode());

                // Capability-based discovery: find orchestration via config fallback
                let fallbacks = build_capability_fallbacks_from_config("127.0.0.1");
                assert!(
                    fallbacks
                        .get("orchestration")
                        .is_some_and(|u| u.contains("9080")),
                    "orchestration fallback should use TOADSTOOL_COORDINATION_URL"
                );

                // Sovereignty: get_service_endpoints only returns toadstool's own
                let endpoints = ConfigUtils::get_service_endpoints();
                assert!(
                    endpoints.contains_key("toadstool"),
                    "should contain toadstool (self-knowledge)"
                );
            },
        );
    }
}
