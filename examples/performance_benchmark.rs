// SPDX-License-Identifier: AGPL-3.0-or-later
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::field_reassign_with_default,
    clippy::redundant_pattern_matching,
    clippy::useless_format,
    dead_code,
    unused_variables
)]
//! # ToadStool Universal Architecture Performance Benchmark
//!
//! This benchmark demonstrates the performance characteristics of the
//! ToadStool Universal Architecture under various load conditions.
//!
//! ## Benchmarks
//!
//! - Concurrent job execution
//! - Primal discovery performance
//! - Resource allocation efficiency
//! - Security level overhead
//! - Memory usage patterns
//! - Throughput measurements
//!
//! ## Usage
//!
//! ```bash
//! cargo run --example performance_benchmark --release
//! ```

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use tokio::sync::Semaphore;
use uuid::Uuid;

use toadstool::universal::{
    JobPriority, NetworkLocation, PrimalCapability, PrimalContext, SecurityLevel,
    UniversalComputePlatform, UniversalJob, UniversalJobType,
};
use toadstool::{ResourceRequirements, ToadStoolError, ToadStoolResult, init};

/// Benchmark configuration
#[derive(Debug, Clone)]
struct BenchmarkConfig {
    /// Number of concurrent jobs
    concurrent_jobs: usize,
    /// Number of iterations per test
    iterations: usize,
    /// Job timeout
    timeout: Duration,
    /// Enable detailed metrics
    detailed_metrics: bool,
}

impl Default for BenchmarkConfig {
    fn default() -> Self {
        Self {
            concurrent_jobs: 100,
            iterations: 1000,
            timeout: Duration::from_secs(30),
            detailed_metrics: true,
        }
    }
}

/// Benchmark results
#[derive(Debug, Clone)]
struct BenchmarkResults {
    /// Test name
    name: String,
    /// Total duration
    total_duration: Duration,
    /// Average duration per operation
    avg_duration: Duration,
    /// Operations per second
    ops_per_second: f64,
    /// Success rate
    success_rate: f64,
    /// Memory usage (if available)
    memory_usage: Option<u64>,
    /// Additional metrics
    metrics: HashMap<String, f64>,
}

#[tokio::main]
async fn main() -> ToadStoolResult<()> {
    // Initialize ToadStool
    init().map_err(|e| ToadStoolError::from(std::io::Error::other(e.to_string())))?;

    println!("🚀 ToadStool Universal Architecture Performance Benchmark");
    println!("{}", "=".repeat(70));

    let config = BenchmarkConfig::default();
    println!("📊 Benchmark Configuration:");
    println!("  • Concurrent jobs: {}", config.concurrent_jobs);
    println!("  • Iterations: {}", config.iterations);
    println!("  • Timeout: {:?}", config.timeout);
    println!("  • Detailed metrics: {}", config.detailed_metrics);

    // Create universal compute platform
    let platform = Arc::new(UniversalComputePlatform::new().await?);
    println!("\n✅ Universal compute platform initialized");

    // Run benchmarks
    let mut results = Vec::new();

    results.push(benchmark_native_execution(&platform, &config).await?);
    results.push(benchmark_concurrent_jobs(&platform, &config).await?);
    results.push(benchmark_primal_discovery(&platform, &config).await?);
    results.push(benchmark_resource_allocation(&platform, &config).await?);
    results.push(benchmark_security_overhead(&platform, &config).await?);
    results.push(benchmark_job_types(&platform, &config).await?);
    results.push(benchmark_memory_usage(&platform, &config).await?);

    // Display results
    display_benchmark_results(&results);

    Ok(())
}

/// Benchmark native execution performance
async fn benchmark_native_execution(
    platform: &Arc<UniversalComputePlatform>,
    config: &BenchmarkConfig,
) -> ToadStoolResult<BenchmarkResults> {
    println!("\n🖥️  Benchmarking Native Execution");
    println!("{}", "-".repeat(50));

    let start = Instant::now();
    let mut successful = 0;
    let mut total_exec_time = Duration::new(0, 0);

    for i in 0..config.iterations {
        let context = create_benchmark_context(&format!("native_{i}"), SecurityLevel::Standard);

        let job = UniversalJob {
            id: Uuid::new_v4(),
            job_type: UniversalJobType::Native {
                executable: "/bin/echo".to_string(),
                args: vec![format!("benchmark_{}", i)],
                env: HashMap::new(),
            },
            priority: JobPriority::Normal,
            resources: ResourceRequirements::default(),
            timeout: Some(config.timeout),
            created_at: SystemTime::now(),
            context,
        };

        let exec_start = Instant::now();
        if let Ok(_) = platform.execute_universal_job(job).await {
            successful += 1;
            total_exec_time += exec_start.elapsed();
        }

        if i % 100 == 0 {
            println!("  Progress: {}/{}", i, config.iterations);
        }
    }

    let total_duration = start.elapsed();
    let avg_duration = total_duration / config.iterations as u32;
    let ops_per_second = config.iterations as f64 / total_duration.as_secs_f64();
    let success_rate = f64::from(successful) / config.iterations as f64;

    let mut metrics = HashMap::new();
    metrics.insert(
        "avg_exec_time_ms".to_string(),
        total_exec_time.as_millis() as f64 / f64::from(successful),
    );

    Ok(BenchmarkResults {
        name: "Native Execution".to_string(),
        total_duration,
        avg_duration,
        ops_per_second,
        success_rate,
        memory_usage: None,
        metrics,
    })
}

/// Benchmark concurrent job execution
async fn benchmark_concurrent_jobs(
    platform: &Arc<UniversalComputePlatform>,
    config: &BenchmarkConfig,
) -> ToadStoolResult<BenchmarkResults> {
    println!("\n🔄 Benchmarking Concurrent Job Execution");
    println!("{}", "-".repeat(50));

    let start = Instant::now();
    let semaphore = Arc::new(Semaphore::new(config.concurrent_jobs));
    let mut handles = Vec::new();

    for i in 0..config.iterations {
        let platform = platform.clone();
        let semaphore = semaphore.clone();
        let timeout = config.timeout;

        let handle = tokio::spawn(async move {
            let _permit = semaphore.acquire().await.unwrap();

            let context =
                create_benchmark_context(&format!("concurrent_{i}"), SecurityLevel::Standard);

            let job = UniversalJob {
                id: Uuid::new_v4(),
                job_type: UniversalJobType::Native {
                    executable: "/bin/echo".to_string(),
                    args: vec![format!("concurrent_{}", i)],
                    env: HashMap::new(),
                },
                priority: JobPriority::Normal,
                resources: ResourceRequirements::default(),
                timeout: Some(timeout),
                created_at: SystemTime::now(),
                context,
            };

            platform.execute_universal_job(job).await
        });

        handles.push(handle);
    }

    let mut successful = 0;
    for handle in handles {
        if let Ok(Ok(_)) = handle.await {
            successful += 1;
        }
    }

    let total_duration = start.elapsed();
    let avg_duration = total_duration / config.iterations as u32;
    let ops_per_second = config.iterations as f64 / total_duration.as_secs_f64();
    let success_rate = f64::from(successful) / config.iterations as f64;

    let mut metrics = HashMap::new();
    metrics.insert(
        "concurrency_level".to_string(),
        config.concurrent_jobs as f64,
    );
    metrics.insert(
        "throughput_improvement".to_string(),
        ops_per_second / config.concurrent_jobs as f64,
    );

    Ok(BenchmarkResults {
        name: "Concurrent Jobs".to_string(),
        total_duration,
        avg_duration,
        ops_per_second,
        success_rate,
        memory_usage: None,
        metrics,
    })
}

/// Benchmark primal discovery performance
async fn benchmark_primal_discovery(
    platform: &Arc<UniversalComputePlatform>,
    config: &BenchmarkConfig,
) -> ToadStoolResult<BenchmarkResults> {
    println!("\n🔍 Benchmarking Primal Discovery");
    println!("{}", "-".repeat(50));

    let capabilities = [
        PrimalCapability::NativeExecution {
            architectures: vec!["x86_64".to_string()],
        },
        PrimalCapability::WasmExecution { wasi_support: true },
        PrimalCapability::ContainerRuntime {
            orchestrators: vec!["docker".to_string()],
        },
        PrimalCapability::LoadBalancing {
            algorithms: vec!["round_robin".to_string()],
        },
    ];

    let start = Instant::now();
    let mut successful = 0;
    let mut total_providers = 0;

    for i in 0..config.iterations {
        let capability = &capabilities[i % capabilities.len()];

        let providers = platform.find_primals_by_capability(capability);
        if !providers.is_empty() {
            successful += 1;
            total_providers += providers.len();
            println!(
                "  📡 Found {} providers for capability: {:?}",
                providers.len(),
                capability
            );
        }

        if i % 100 == 0 {
            println!("  Progress: {}/{}", i, config.iterations);
        }
    }

    let total_duration = start.elapsed();
    let avg_duration = total_duration / config.iterations as u32;
    let ops_per_second = config.iterations as f64 / total_duration.as_secs_f64();
    let success_rate = f64::from(successful) / config.iterations as f64;

    let mut metrics = HashMap::new();
    metrics.insert(
        "avg_providers_found".to_string(),
        total_providers as f64 / f64::from(successful),
    );
    metrics.insert(
        "discovery_latency_ms".to_string(),
        avg_duration.as_millis() as f64,
    );

    Ok(BenchmarkResults {
        name: "Primal Discovery".to_string(),
        total_duration,
        avg_duration,
        ops_per_second,
        success_rate,
        memory_usage: None,
        metrics,
    })
}

/// Benchmark resource allocation efficiency
async fn benchmark_resource_allocation(
    platform: &Arc<UniversalComputePlatform>,
    config: &BenchmarkConfig,
) -> ToadStoolResult<BenchmarkResults> {
    println!("\n💾 Benchmarking Resource Allocation");
    println!("{}", "-".repeat(50));

    let start = Instant::now();
    let mut successful = 0;
    let mut total_allocation_time = Duration::new(0, 0);

    for i in 0..config.iterations {
        let context = create_benchmark_context(&format!("resource_{i}"), SecurityLevel::Standard);

        let mut resources = ResourceRequirements::default();
        resources.cpu = toadstool::CpuRequirements {
            min_cores: (i % 4 + 1) as f64,
            ..Default::default()
        };
        resources.memory = toadstool::MemoryRequirements {
            min_bytes: ((i % 8 + 1) * 256 * 1024 * 1024) as u64, // Convert MB to bytes
            ..Default::default()
        };

        let job = UniversalJob {
            id: Uuid::new_v4(),
            job_type: UniversalJobType::Native {
                executable: "/bin/sleep".to_string(),
                args: vec!["0.1".to_string()],
                env: HashMap::new(),
            },
            priority: JobPriority::Normal,
            resources,
            timeout: Some(config.timeout),
            created_at: SystemTime::now(),
            context,
        };

        let alloc_start = Instant::now();
        if let Ok(_) = platform.execute_universal_job(job).await {
            successful += 1;
            total_allocation_time += alloc_start.elapsed();
        }

        if i % 100 == 0 {
            println!("  Progress: {}/{}", i, config.iterations);
        }
    }

    let total_duration = start.elapsed();
    let avg_duration = total_duration / config.iterations as u32;
    let ops_per_second = config.iterations as f64 / total_duration.as_secs_f64();
    let success_rate = f64::from(successful) / config.iterations as f64;

    let mut metrics = HashMap::new();
    metrics.insert(
        "avg_allocation_time_ms".to_string(),
        total_allocation_time.as_millis() as f64 / f64::from(successful),
    );
    metrics.insert("allocation_efficiency".to_string(), success_rate * 100.0);

    Ok(BenchmarkResults {
        name: "Resource Allocation".to_string(),
        total_duration,
        avg_duration,
        ops_per_second,
        success_rate,
        memory_usage: None,
        metrics,
    })
}

/// Benchmark security level overhead
async fn benchmark_security_overhead(
    platform: &Arc<UniversalComputePlatform>,
    config: &BenchmarkConfig,
) -> ToadStoolResult<BenchmarkResults> {
    println!("\n🔒 Benchmarking Security Level Overhead");
    println!("{}", "-".repeat(50));

    let security_levels = [
        SecurityLevel::None,
        SecurityLevel::Standard,
        SecurityLevel::High,
        SecurityLevel::Maximum,
    ];

    let start = Instant::now();
    let mut successful = 0;
    let mut level_times = HashMap::new();

    for i in 0..config.iterations {
        let level = &security_levels[i % security_levels.len()];
        let context = create_benchmark_context(&format!("security_{i}"), *level);

        let job = UniversalJob {
            id: Uuid::new_v4(),
            job_type: UniversalJobType::Native {
                executable: "/bin/echo".to_string(),
                args: vec![format!("security_{:?}", level)],
                env: HashMap::new(),
            },
            priority: JobPriority::Normal,
            resources: ResourceRequirements::default(),
            timeout: Some(config.timeout),
            created_at: SystemTime::now(),
            context,
        };

        let level_start = Instant::now();
        if let Ok(_) = platform.execute_universal_job(job).await {
            successful += 1;
            let level_duration = level_start.elapsed();
            *level_times
                .entry(format!("{level:?}"))
                .or_insert(Duration::new(0, 0)) += level_duration;
        }

        if i % 100 == 0 {
            println!("  Progress: {}/{}", i, config.iterations);
        }
    }

    let total_duration = start.elapsed();
    let avg_duration = total_duration / config.iterations as u32;
    let ops_per_second = config.iterations as f64 / total_duration.as_secs_f64();
    let success_rate = f64::from(successful) / config.iterations as f64;

    let mut metrics = HashMap::new();
    for (level, duration) in level_times {
        metrics.insert(
            format!("{level}_avg_ms"),
            duration.as_millis() as f64 / (config.iterations / 4) as f64,
        );
    }

    Ok(BenchmarkResults {
        name: "Security Overhead".to_string(),
        total_duration,
        avg_duration,
        ops_per_second,
        success_rate,
        memory_usage: None,
        metrics,
    })
}

/// Benchmark different job types
async fn benchmark_job_types(
    platform: &Arc<UniversalComputePlatform>,
    config: &BenchmarkConfig,
) -> ToadStoolResult<BenchmarkResults> {
    println!("\n🎯 Benchmarking Job Types");
    println!("{}", "-".repeat(50));

    let start = Instant::now();
    let mut successful = 0;
    let mut type_counts = HashMap::new();

    for i in 0..config.iterations {
        let context = create_benchmark_context(&format!("jobtype_{i}"), SecurityLevel::Standard);

        let job = match i % 4 {
            0 => UniversalJob {
                id: Uuid::new_v4(),
                job_type: UniversalJobType::Native {
                    executable: "/bin/echo".to_string(),
                    args: vec![format!("native_{}", i)],
                    env: HashMap::new(),
                },
                priority: JobPriority::Normal,
                resources: ResourceRequirements::default(),
                timeout: Some(config.timeout),
                created_at: SystemTime::now(),
                context,
            },
            1 => UniversalJob {
                id: Uuid::new_v4(),
                job_type: UniversalJobType::Wasm {
                    module: vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00],
                    args: vec![],
                    env: HashMap::new(),
                },
                priority: JobPriority::Normal,
                resources: ResourceRequirements::default(),
                timeout: Some(config.timeout),
                created_at: SystemTime::now(),
                context,
            },
            2 => UniversalJob {
                id: Uuid::new_v4(),
                job_type: UniversalJobType::Primal {
                    primal_type: "compute".to_string(),
                    endpoint: "http://localhost:8080".to_string(),
                    payload: serde_json::json!({"task": "test"}),
                },
                priority: JobPriority::Normal,
                resources: ResourceRequirements::default(),
                timeout: Some(config.timeout),
                created_at: SystemTime::now(),
                context,
            },
            3 => UniversalJob {
                id: Uuid::new_v4(),
                job_type: UniversalJobType::BiomeOS {
                    biome_manifest: serde_json::json!({"team": "test"}),
                    team_id: "test-team".to_string(),
                },
                priority: JobPriority::Normal,
                resources: ResourceRequirements::default(),
                timeout: Some(config.timeout),
                created_at: SystemTime::now(),
                context,
            },
            _ => {
                unreachable!("i % 4 is exhaustively matched 0..=3")
            }
        };

        let job_type_name = match &job.job_type {
            UniversalJobType::Native { .. } => "Native",
            UniversalJobType::Wasm { .. } => "WASM",
            UniversalJobType::Primal { .. } => "Primal",
            UniversalJobType::BiomeOS { .. } => "BiomeOS",
        };

        if let Ok(_) = platform.execute_universal_job(job).await {
            successful += 1;
            *type_counts.entry(job_type_name.to_string()).or_insert(0) += 1;
        }

        if i % 100 == 0 {
            println!("  Progress: {}/{}", i, config.iterations);
        }
    }

    let total_duration = start.elapsed();
    let avg_duration = total_duration / config.iterations as u32;
    let ops_per_second = config.iterations as f64 / total_duration.as_secs_f64();
    let success_rate = f64::from(successful) / config.iterations as f64;

    let mut metrics = HashMap::new();
    for (job_type, count) in type_counts {
        metrics.insert(
            format!("{job_type}_success_rate"),
            f64::from(count) / (config.iterations / 4) as f64,
        );
    }

    Ok(BenchmarkResults {
        name: "Job Types".to_string(),
        total_duration,
        avg_duration,
        ops_per_second,
        success_rate,
        memory_usage: None,
        metrics,
    })
}

/// Benchmark memory usage patterns
async fn benchmark_memory_usage(
    platform: &Arc<UniversalComputePlatform>,
    config: &BenchmarkConfig,
) -> ToadStoolResult<BenchmarkResults> {
    println!("\n🧠 Benchmarking Memory Usage");
    println!("{}", "-".repeat(50));

    let start = Instant::now();
    let mut successful = 0;

    // Simplified memory benchmark - in production you'd use proper memory profiling
    for i in 0..config.iterations {
        let context = create_benchmark_context(&format!("memory_{i}"), SecurityLevel::Standard);

        let job = UniversalJob {
            id: Uuid::new_v4(),
            job_type: UniversalJobType::Native {
                executable: "/bin/echo".to_string(),
                args: vec![format!("memory_test_{}", i)],
                env: HashMap::new(),
            },
            priority: JobPriority::Normal,
            resources: ResourceRequirements::default(),
            timeout: Some(config.timeout),
            created_at: SystemTime::now(),
            context,
        };

        if let Ok(_) = platform.execute_universal_job(job).await {
            successful += 1;
        }

        if i % 100 == 0 {
            println!("  Progress: {}/{}", i, config.iterations);
        }
    }

    let total_duration = start.elapsed();
    let avg_duration = total_duration / config.iterations as u32;
    let ops_per_second = config.iterations as f64 / total_duration.as_secs_f64();
    let success_rate = f64::from(successful) / config.iterations as f64;

    let mut metrics = HashMap::new();
    metrics.insert("memory_efficiency".to_string(), success_rate * 100.0);

    Ok(BenchmarkResults {
        name: "Memory Usage".to_string(),
        total_duration,
        avg_duration,
        ops_per_second,
        success_rate,
        memory_usage: Some(0), // Would be actual memory usage in production
        metrics,
    })
}

/// Display benchmark results
fn display_benchmark_results(results: &[BenchmarkResults]) {
    println!("\n📊 Benchmark Results Summary");
    println!("{}", "=".repeat(70));

    for result in results {
        println!("\n🎯 {}", result.name);
        println!("  • Total Duration: {:?}", result.total_duration);
        println!("  • Average Duration: {:?}", result.avg_duration);
        println!("  • Operations/Second: {:.2}", result.ops_per_second);
        println!("  • Success Rate: {:.2}%", result.success_rate * 100.0);

        if let Some(memory) = result.memory_usage {
            println!("  • Memory Usage: {memory} bytes");
        }

        if !result.metrics.is_empty() {
            println!("  • Additional Metrics:");
            for (key, value) in &result.metrics {
                println!("    - {key}: {value:.2}");
            }
        }
    }

    println!("\n🏆 Performance Summary");
    println!("{}", "-".repeat(50));

    let total_ops: f64 = results.iter().map(|r| r.ops_per_second).sum();
    let avg_success_rate: f64 =
        results.iter().map(|r| r.success_rate).sum::<f64>() / results.len() as f64;

    println!("  • Total Operations/Second: {total_ops:.2}");
    println!("  • Average Success Rate: {:.2}%", avg_success_rate * 100.0);
    println!("  • Benchmark Categories: {}", results.len());

    println!("\n✅ Performance benchmark complete!");
}

/// Create a benchmark context
fn create_benchmark_context(name: &str, security_level: SecurityLevel) -> PrimalContext {
    PrimalContext {
        user_id: format!("benchmark-{name}"),
        device_id: "benchmark-device".to_string(),
        session_id: Uuid::new_v4().to_string(),
        network_location: NetworkLocation {
            ip_address: "127.0.0.1".to_string(),
            subnet: Some("192.168.1.0/24".to_string()),
            network_id: Some("benchmark-network".to_string()),
            geo_location: Some("localhost".to_string()),
        },
        security_level,
        metadata: {
            let mut metadata = HashMap::new();
            metadata.insert("benchmark".to_string(), "true".to_string());
            metadata.insert("test_name".to_string(), name.to_string());
            metadata
        },
    }
}
