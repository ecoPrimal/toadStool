// SPDX-License-Identifier: AGPL-3.0-or-later
#![allow(clippy::nursery, dead_code)]
#![allow(unused_variables)]
#![allow(clippy::field_reassign_with_default)]
#![allow(clippy::useless_format)]
#![allow(clippy::redundant_pattern_matching)]
#![allow(clippy::items_after_statements)]
//! # Production Universal Architecture Demo
//!
//! This example demonstrates the complete ToadStool Universal Architecture
//! capabilities in a production-ready scenario.
//!
//! ## Features Demonstrated
//!
//! - Universal job execution (Native, WASM, Primal, BiomeOS)
//! - Capability-based primal discovery
//! - Resource allocation and management
//! - Inter-primal communication
//! - Security level enforcement
//! - Context-aware routing
//! - Platform status monitoring
//! - Ecosystem integration
//!
//! ## Usage
//!
//! ```bash
//! cargo run --example production_universal_demo
//! ```

use std::collections::HashMap;
use std::time::{Duration, SystemTime};
use uuid::Uuid;

use toadstool::universal::{
    JobPriority, NetworkLocation, PrimalCapability, PrimalContext, PrimalRequest, SecurityLevel,
    UniversalComputePlatform, UniversalJob, UniversalJobType,
};
use toadstool::{
    CpuRequirements, MemoryRequirements, ResourceRequirements, ToadStoolError, ToadStoolResult,
    init,
};

#[tokio::main]
async fn main() -> ToadStoolResult<()> {
    // Initialize ToadStool
    init().map_err(|e| ToadStoolError::execution(e.to_string()))?;

    println!("🍄 ToadStool Production Universal Architecture Demo");
    println!("{}", "=".repeat(60));

    // Create universal compute platform
    let platform = UniversalComputePlatform::new().await?;
    println!("✅ Universal compute platform initialized");

    // Demonstrate platform capabilities
    demo_platform_status(&platform).await?;
    demo_capability_discovery(&platform).await?;
    demo_native_execution(&platform).await?;
    demo_wasm_execution(&platform).await?;
    demo_primal_routing(&platform).await?;
    demo_biomeos_integration(&platform).await?;
    demo_resource_management(&platform).await?;
    demo_security_levels(&platform).await?;
    demo_ecosystem_integration(&platform).await?;

    println!("\n🎉 Production Universal Architecture Demo Complete!");
    println!("All capabilities demonstrated successfully.");

    Ok(())
}

/// Demonstrate platform status monitoring
async fn demo_platform_status(platform: &UniversalComputePlatform) -> ToadStoolResult<()> {
    println!("\n📊 Platform Status Monitoring");
    println!("{}", "-".repeat(40));

    // Get platform configuration
    let config = platform.get_config();
    println!("🔧 Configuration:");
    println!("  • Recursive hosting: {}", config.recursive_hosting);
    println!(
        "  • Ecosystem integration: {}",
        config.ecosystem_integration
    );
    println!("  • BiomeOS integration: {}", config.biomeos_integration);
    println!("  • Max concurrent jobs: {}", config.max_concurrent_jobs);

    // Get available runtimes
    let runtimes = platform.get_available_runtimes();
    println!("🏃 Available runtimes: {runtimes:?}");

    Ok(())
}

/// Demonstrate capability-based primal discovery
async fn demo_capability_discovery(platform: &UniversalComputePlatform) -> ToadStoolResult<()> {
    println!("\n🔍 Capability-Based Primal Discovery");
    println!("{}", "-".repeat(40));

    // Discover native execution capabilities
    let native_providers =
        platform.find_primals_by_capability(&PrimalCapability::NativeExecution {
            architectures: vec!["x86_64".to_string()],
        });
    println!("🖥️  Native execution providers: {}", native_providers.len());

    // Discover WASM capabilities
    let wasm_providers = platform
        .find_primals_by_capability(&PrimalCapability::WasmExecution { wasi_support: true });
    println!("🕸️  WASM execution providers: {}", wasm_providers.len());

    // Discover container capabilities
    let container_providers =
        platform.find_primals_by_capability(&PrimalCapability::ContainerRuntime {
            orchestrators: vec!["docker".to_string()],
        });
    println!(
        "📦 Container runtime providers: {}",
        container_providers.len()
    );

    // Discover load balancing capabilities
    let lb_providers = platform.find_primals_by_capability(&PrimalCapability::LoadBalancing {
        algorithms: vec!["round_robin".to_string()],
    });
    println!("⚖️  Load balancing providers: {}", lb_providers.len());

    Ok(())
}

/// Demonstrate native execution
async fn demo_native_execution(platform: &UniversalComputePlatform) -> ToadStoolResult<()> {
    println!("\n🖥️  Native Execution Demo");
    println!("{}", "-".repeat(40));

    let context = create_demo_context("native_demo", SecurityLevel::Standard);

    // Create native job
    let job = UniversalJob {
        id: Uuid::new_v4(),
        job_type: UniversalJobType::Native {
            executable: "/bin/echo".to_string(),
            args: vec!["Hello from ToadStool Universal Architecture!".to_string()],
            env: HashMap::new(),
        },
        priority: JobPriority::Normal,
        resources: ResourceRequirements::default(),
        timeout: Some(Duration::from_secs(30)),
        created_at: SystemTime::now(),
        context,
    };

    println!("🚀 Executing native job: {}", job.id);
    let response = platform.execute_universal_job(job).await?;

    println!("✅ Native execution completed:");
    println!("  • Status: {:?}", response.status);
    println!("  • Duration: {:?}", response.duration);
    println!("  • Runtime: {:?}", response.runtime_used);
    if let Some(stdout) = &response.output.stdout {
        println!("  • Output: {stdout}");
    }

    Ok(())
}

/// Demonstrate WASM execution
async fn demo_wasm_execution(platform: &UniversalComputePlatform) -> ToadStoolResult<()> {
    println!("\n🕸️  WASM Execution Demo");
    println!("{}", "-".repeat(40));

    let context = create_demo_context("wasm_demo", SecurityLevel::High);

    // Create simple WASM module (mock)
    let wasm_module = create_mock_wasm_module();

    let job = UniversalJob {
        id: Uuid::new_v4(),
        job_type: UniversalJobType::Wasm {
            module: wasm_module,
            args: vec!["arg1".to_string(), "arg2".to_string()],
            env: HashMap::new(),
        },
        priority: JobPriority::High,
        resources: ResourceRequirements::default(),
        timeout: Some(Duration::from_secs(30)),
        created_at: SystemTime::now(),
        context,
    };

    println!("🚀 Executing WASM job: {}", job.id);
    let response = platform.execute_universal_job(job).await?;

    println!("✅ WASM execution completed:");
    println!("  • Status: {:?}", response.status);
    println!("  • Duration: {:?}", response.duration);
    println!("  • Runtime: {:?}", response.runtime_used);
    if let Some(stdout) = &response.output.stdout {
        println!("  • Output: {stdout}");
    }

    Ok(())
}

/// Demonstrate primal routing
async fn demo_primal_routing(platform: &UniversalComputePlatform) -> ToadStoolResult<()> {
    println!("\n🎯 Primal Routing Demo");
    println!("{}", "-".repeat(40));

    let context = create_demo_context("primal_demo", SecurityLevel::Standard);

    // Create primal request
    let request = PrimalRequest {
        id: Uuid::new_v4(),
        source: "toadstool-demo".to_string(),
        target: "toadstool".to_string(),
        request_type: "compute_request".to_string(),
        payload: serde_json::json!({
            "operation": "status_check",
            "parameters": {
                "include_metrics": true
            }
        }),
        context: context.clone(),
        metadata: HashMap::new(),
        timestamp: SystemTime::now(),
    };

    println!("🚀 Routing primal request: {}", request.id);
    let response = platform.route_primal_request(request).await?;

    println!("✅ Primal routing completed:");
    println!("  • Status: {:?}", response.status);
    println!("  • Response: {}", response.payload);

    // Create primal job — endpoint from capability discovery, not hardcoded
    let compute_endpoint = discover_compute_endpoint().await;
    let job = UniversalJob {
        id: Uuid::new_v4(),
        job_type: UniversalJobType::Primal {
            primal_type: "compute".to_string(),
            endpoint: compute_endpoint,
            payload: serde_json::json!({
                "task": "process_data",
                "data": "sample_data"
            }),
        },
        priority: JobPriority::Normal,
        resources: ResourceRequirements::default(),
        timeout: Some(Duration::from_secs(30)),
        created_at: SystemTime::now(),
        context,
    };

    println!("🚀 Executing primal job: {}", job.id);
    let response = platform.execute_universal_job(job).await?;

    println!("✅ Primal execution completed:");
    println!("  • Status: {:?}", response.status);
    println!("  • Duration: {:?}", response.duration);

    Ok(())
}

/// Demonstrate BiomeOS integration
async fn demo_biomeos_integration(platform: &UniversalComputePlatform) -> ToadStoolResult<()> {
    println!("\n🌱 BiomeOS Integration Demo");
    println!("{}", "-".repeat(40));

    let context = create_demo_context("biomeos_demo", SecurityLevel::Maximum);

    // Create BiomeOS job
    let job = UniversalJob {
        id: Uuid::new_v4(),
        job_type: UniversalJobType::BiomeOS {
            biome_manifest: serde_json::json!({
                "version": "1.0",
                "team": "demo-team",
                "services": {
                    "web-service": {
                        "image": "nginx:latest",
                        "ports": ["80:8080"],
                        "environment": {
                            "ENV": "production"
                        }
                    }
                },
                "resources": {
                    "cpu": "2",
                    "memory": "4Gi"
                }
            }),
            team_id: "demo-team".to_string(),
        },
        priority: JobPriority::High,
        resources: ResourceRequirements::default(),
        timeout: Some(Duration::from_mins(1)),
        created_at: SystemTime::now(),
        context,
    };

    println!("🚀 Executing BiomeOS job: {}", job.id);
    let response = platform.execute_universal_job(job).await?;

    println!("✅ BiomeOS execution completed:");
    println!("  • Status: {:?}", response.status);
    println!("  • Duration: {:?}", response.duration);
    println!("  • Team: demo-team");
    if let Some(stdout) = &response.output.stdout {
        println!("  • Output: {stdout}");
    }

    Ok(())
}

/// Demonstrate resource management
async fn demo_resource_management(platform: &UniversalComputePlatform) -> ToadStoolResult<()> {
    println!("\n💾 Resource Management Demo");
    println!("{}", "-".repeat(40));

    let context = create_demo_context("resource_demo", SecurityLevel::Standard);

    // Create resource-intensive job
    let mut resources = ResourceRequirements::default();
    resources.cpu = CpuRequirements {
        min_cores: 2.0,
        max_cores: Some(2.0),
        architecture: None,
    };
    resources.memory = MemoryRequirements {
        min_bytes: 1024 * 1024 * 1024, // 1GB
        max_bytes: Some(1024 * 1024 * 1024),
    };

    let job = UniversalJob {
        id: Uuid::new_v4(),
        job_type: UniversalJobType::Native {
            executable: "/bin/sleep".to_string(),
            args: vec!["2".to_string()],
            env: HashMap::new(),
        },
        priority: JobPriority::Normal,
        resources,
        timeout: Some(Duration::from_secs(30)),
        created_at: SystemTime::now(),
        context,
    };

    println!("🚀 Executing resource-intensive job: {}", job.id);
    println!("  • CPU cores: {}", job.resources.cpu.min_cores);
    println!(
        "  • Memory: {} MB",
        job.resources.memory.min_bytes / (1024 * 1024)
    );

    let response = platform.execute_universal_job(job).await?;

    println!("✅ Resource management completed:");
    println!("  • Status: {:?}", response.status);
    println!("  • Duration: {:?}", response.duration);
    println!("  • CPU usage: {:.2}%", response.metrics.cpu.usage_percent);
    println!(
        "  • Memory usage: {} MB",
        response.metrics.memory.used_bytes / (1024 * 1024)
    );

    Ok(())
}

/// Demonstrate security levels
async fn demo_security_levels(platform: &UniversalComputePlatform) -> ToadStoolResult<()> {
    println!("\n🔒 Security Levels Demo");
    println!("{}", "-".repeat(40));

    let security_levels = vec![
        SecurityLevel::None,
        SecurityLevel::Standard,
        SecurityLevel::High,
        SecurityLevel::Maximum,
    ];

    for level in security_levels {
        let context = create_demo_context(&format!("security_{level:?}"), level);

        let job = UniversalJob {
            id: Uuid::new_v4(),
            job_type: UniversalJobType::Native {
                executable: "/bin/echo".to_string(),
                args: vec![format!("Security level: {:?}", level)],
                env: HashMap::new(),
            },
            priority: JobPriority::Normal,
            resources: ResourceRequirements::default(),
            timeout: Some(Duration::from_secs(30)),
            created_at: SystemTime::now(),
            context,
        };

        println!("🚀 Executing job with security level: {level:?}");
        let response = platform.execute_universal_job(job).await?;

        println!("  ✅ Status: {:?}", response.status);
    }

    Ok(())
}

/// Demonstrate ecosystem integration via capability-based discovery
///
/// wateringHole standard: discover primals by capability at runtime.
/// No hardcoded primal names or ports — use `ipc.find_capability` pattern.
async fn demo_ecosystem_integration(platform: &UniversalComputePlatform) -> ToadStoolResult<()> {
    println!("\n🌐 Ecosystem Integration Demo (Capability-Based Discovery)");
    println!("{}", "-".repeat(40));

    // Discover ecosystem
    println!("🔍 Discovering ecosystem...");
    platform.discover_ecosystem().await?;
    println!("✅ Ecosystem discovery completed");

    // Capabilities to test — discover by capability, not by primal name
    let capabilities = vec![
        ("compute", "compute capability (e.g. ToadStool)"),
        ("security", "security capability (e.g. BearDog)"),
        ("storage", "storage capability (e.g. NestGate)"),
        ("ai", "AI capability (e.g. Squirrel)"),
        ("orchestration", "orchestration capability (e.g. Songbird)"),
    ];

    use std::collections::HashMap;
    use toadstool_common::primal_discovery::{DiscoveryConfig, PrimalDiscovery};
    use toadstool_config::config_utils::ConfigUtils;
    use toadstool_config::ports::capability_fallback;

    let bind_host = ConfigUtils::get_bind_address();
    let mut fallbacks = HashMap::new();
    let specs: &[(&str, &str, u16)] = &[
        (
            "TOADSTOOL_COORDINATION_URL",
            "orchestration",
            capability_fallback::COORDINATION,
        ),
        (
            "TOADSTOOL_SECURITY_URL",
            "security",
            capability_fallback::SECURITY,
        ),
        (
            "TOADSTOOL_STORAGE_URL",
            "storage",
            capability_fallback::STORAGE,
        ),
        (
            "TOADSTOOL_PLATFORM_URL",
            "ai",
            capability_fallback::PLATFORM,
        ),
    ];
    for (env_var, cap, port) in specs {
        let url = std::env::var(env_var).unwrap_or_else(|_| format!("http://{bind_host}:{port}"));
        fallbacks.insert((*cap).to_string(), url);
    }
    fallbacks.insert(
        "compute".to_string(),
        std::env::var("TOADSTOOL_COMPUTE_URL")
            .unwrap_or_else(|_| format!("http://{bind_host}:8084")),
    );

    let discovery = PrimalDiscovery::with_config(DiscoveryConfig {
        fallbacks,
        ..Default::default()
    })
    .map_err(|e| ToadStoolError::configuration(e.to_string()))?;

    for (capability, description) in capabilities {
        println!("🎯 Testing {description} (discovered via capability)...");

        let endpoint = match discovery.find_capability(capability).await {
            Ok(ep) => ep.url().to_string(),
            Err(_) => format!("http://{bind_host}:8080/{capability}"), // fallback for demo
        };

        let context =
            create_demo_context(&format!("ecosystem_{capability}"), SecurityLevel::Standard);

        let job = UniversalJob {
            id: Uuid::new_v4(),
            job_type: UniversalJobType::Primal {
                primal_type: capability.to_string(),
                endpoint,
                payload: serde_json::json!({
                    "operation": "health_check",
                    "timestamp": toadstool_common::system_time_serde::format_rfc3339(SystemTime::now())
                }),
            },
            priority: JobPriority::Normal,
            resources: ResourceRequirements::default(),
            timeout: Some(Duration::from_secs(30)),
            created_at: SystemTime::now(),
            context,
        };

        let response = platform.execute_universal_job(job).await?;
        println!("  ✅ {} integration: {:?}", description, response.status);
    }

    Ok(())
}

/// Discover compute endpoint via capability — no hardcoded URLs
async fn discover_compute_endpoint() -> String {
    use std::collections::HashMap;
    use toadstool_common::primal_discovery::{DiscoveryConfig, PrimalDiscovery};
    use toadstool_config::config_utils::ConfigUtils;

    let bind_host = ConfigUtils::get_bind_address();
    let mut fallbacks = HashMap::new();
    fallbacks.insert(
        "compute".to_string(),
        std::env::var("TOADSTOOL_COMPUTE_URL")
            .unwrap_or_else(|_| format!("http://{bind_host}:8084")),
    );

    match PrimalDiscovery::with_config(DiscoveryConfig {
        fallbacks,
        ..Default::default()
    }) {
        Ok(discovery) => discovery.find_capability("compute").await.map_or_else(
            |_| format!("http://{bind_host}:8084"),
            |ep| ep.url().to_string(),
        ),
        Err(_) => format!("http://{bind_host}:8084"),
    }
}

/// Create a demo context for testing
fn create_demo_context(demo_name: &str, security_level: SecurityLevel) -> PrimalContext {
    PrimalContext {
        user_id: format!("demo-user-{demo_name}"),
        device_id: "demo-device".to_string(),
        session_id: Uuid::new_v4().to_string(),
        network_location: NetworkLocation {
            ip_address: "127.0.0.1".to_string(),
            subnet: Some("192.168.1.0/24".to_string()),
            network_id: Some("demo-network".to_string()),
            geo_location: Some("localhost".to_string()),
        },
        security_level,
        metadata: {
            let mut metadata = HashMap::new();
            metadata.insert("demo".to_string(), "true".to_string());
            metadata.insert("demo_name".to_string(), demo_name.to_string());
            metadata.insert(
                "timestamp".to_string(),
                toadstool_common::system_time_serde::format_rfc3339(SystemTime::now()),
            );
            metadata
        },
    }
}

/// Create a mock WASM module for demonstration
fn create_mock_wasm_module() -> Vec<u8> {
    // This is a minimal valid WASM module that does nothing
    // In production, this would be actual compiled WASM bytecode
    vec![
        0x00, 0x61, 0x73, 0x6d, // WASM magic number
        0x01, 0x00, 0x00, 0x00, // WASM version
    ]
}
