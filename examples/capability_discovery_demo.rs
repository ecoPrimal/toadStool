// SPDX-License-Identifier: AGPL-3.0-or-later
//! Example: Capability-Based Discovery (wateringHole Standard)
//!
//! Demonstrates the correct pattern: discover primals by capability at runtime.
//! No hardcoded primal names or ports — use `ipc.find_capability` / `find_capability`.
//!
//! ## Pattern
//! - Use capability names: `orchestration`, `security`, `storage`, `compute_gpu`, etc.
//! - Fallbacks come from env: `TOADSTOOL_COORDINATION_URL`, `TOADSTOOL_SECURITY_URL`, etc.
//! - Resolve transport at runtime; no compile-time coupling to specific primals.

use std::collections::HashMap;
use toadstool_common::primal_discovery::{DiscoveryConfig, PrimalDiscovery};
use toadstool_config::config_utils::ConfigUtils;
use toadstool_config::ports::capability_fallback;

/// Build discovery fallbacks from configuration — no hardcoded ports.
/// Uses `TOADSTOOL_COORDINATION_URL`, `TOADSTOOL_SECURITY_URL`, `TOADSTOOL_STORAGE_URL`
/// or `capability_fallback` ports with bind host.
fn build_fallbacks_from_config() -> HashMap<String, String> {
    let bind_host = ConfigUtils::get_bind_address();
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
        (
            "TOADSTOOL_PLATFORM_URL",
            &["ai_coordination", "platform"][..],
            capability_fallback::PLATFORM,
        ),
    ];
    let mut fallbacks = HashMap::new();
    for (env_var, capability_keys, port) in specs {
        let url = std::env::var(env_var).unwrap_or_else(|_| format!("http://{bind_host}:{port}"));
        for key in *capability_keys {
            fallbacks.insert((*key).to_string(), url.clone());
        }
    }
    fallbacks
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Capability-based discovery: fallbacks from config/env, not hardcoded constants
    let config = DiscoveryConfig {
        methods: vec!["mdns".into(), "env".into()],
        fallbacks: build_fallbacks_from_config(),
        ..Default::default()
    };

    let discovery = PrimalDiscovery::with_config(config)?;

    // Discover by capability — no primal names! (ipc.find_capability pattern)
    println!("🔍 Discovering primals by capability (no hardcoded names/ports)...\n");

    // Orchestration capability (discovered at runtime)
    match discovery.find_capability("orchestration").await {
        Ok(endpoint) => {
            println!("✅ Found orchestration capability:");
            println!("   URL: {}", endpoint.url());
            println!("   Discovered via: {:?}", endpoint.discovered_via);
            println!("   Trust level: {:?}", endpoint.trust_level);
        }
        Err(e) => println!("❌ Orchestration not found: {e}"),
    }

    println!();

    // Security capability (discovered at runtime)
    match discovery.find_capability("security").await {
        Ok(endpoint) => {
            println!("✅ Found security capability:");
            println!("   URL: {}", endpoint.url());
            println!("   Discovered via: {:?}", endpoint.discovered_via);
        }
        Err(e) => println!("❌ Security not found: {e}"),
    }

    println!();

    // Storage capability (discovered at runtime)
    match discovery.find_capability("storage").await {
        Ok(endpoint) => {
            println!("✅ Found storage capability:");
            println!("   URL: {}", endpoint.url());
            println!("   Discovered via: {:?}", endpoint.discovered_via);
        }
        Err(e) => println!("❌ Storage not found: {e}"),
    }

    println!("\n🎉 Benefits of discovery-based approach:");
    println!("   • Zero compile-time coupling");
    println!("   • Works in any environment");
    println!("   • Handles dynamic topologies");
    println!("   • Container-friendly");
    println!("   • Auto-scales");

    Ok(())
}
