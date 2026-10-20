// SPDX-License-Identifier: AGPL-3.0-or-later
//! # Modern Rust Patterns Showcase

#![allow(
    clippy::nursery,
    clippy::missing_errors_doc,
    clippy::needless_pass_by_value
)]
//!
//! This example demonstrates the modern, idiomatic patterns we're evolving toward
//! in the ToadStool codebase. Use this as a reference for refactoring.
//!
//! ## Patterns Demonstrated:
//! - Error handling with ? operator (not unwrap)
//! - Zero-copy with Cow and borrowed data
//! - Modern async/await patterns
//! - Builder pattern for configuration
//! - Result propagation
//! - Smart pointer usage

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;

/// Modern error handling - propagate errors, don't panic
///
/// ❌ OLD WAY:
/// ```ignore
/// let value = map.get(key).unwrap();
/// ```
///
/// ✅ NEW WAY:
/// ```ignore
/// let value = map.get(key).ok_or_else(|| Error::NotFound)?;
/// ```
pub fn modern_error_handling_example() -> Result<String, Box<dyn std::error::Error>> {
    let mut config = HashMap::new();
    config.insert("endpoint", "http://localhost:8080");

    // ✅ Use ? operator for error propagation
    let endpoint = config.get("endpoint").ok_or("Endpoint not configured")?;

    // ✅ Convert to owned type at the end, not throughout
    Ok(endpoint.to_string())
}

/// Zero-copy string handling with Cow (Clone on Write)
///
/// ❌ OLD WAY:
/// ```ignore
/// fn process(name: String) {
///     do_work(name.clone()); // Unnecessary clone
/// }
/// ```
///
/// ✅ NEW WAY: Use Cow for flexible borrowing
#[must_use]
pub fn zero_copy_string_example(input: &str) -> Cow<'_, str> {
    // Return borrowed if no modification needed
    if input.len() < 10 {
        Cow::Borrowed(input)
    } else {
        // Only clone when modification is needed
        Cow::Owned(format!("{}...", &input[..10]))
    }
}

/// Builder pattern for complex configuration
///
/// ✅ Provides:
/// - Compile-time validation
/// - Fluent API
/// - Default values
/// - Clear intent
#[derive(Debug, Clone)]
pub struct ServiceConfig {
    pub name: String,
    pub endpoint: String,
    pub timeout_secs: u64,
    pub retry_attempts: u32,
}

#[derive(Default)]
pub struct ServiceConfigBuilder {
    name: Option<String>,
    endpoint: Option<String>,
    timeout_secs: Option<u64>,
    retry_attempts: Option<u32>,
}

impl ServiceConfigBuilder {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    #[must_use]
    pub fn endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = Some(endpoint.into());
        self
    }

    #[must_use]
    pub fn timeout_secs(mut self, timeout: u64) -> Self {
        self.timeout_secs = Some(timeout);
        self
    }

    #[must_use]
    pub fn retry_attempts(mut self, retries: u32) -> Self {
        self.retry_attempts = Some(retries);
        self
    }

    /// ✅ Build with validation, return Result
    pub fn build(self) -> Result<ServiceConfig, &'static str> {
        Ok(ServiceConfig {
            name: self.name.ok_or("name is required")?,
            endpoint: self.endpoint.ok_or("endpoint is required")?,
            timeout_secs: self.timeout_secs.unwrap_or(30),
            retry_attempts: self.retry_attempts.unwrap_or(3),
        })
    }
}

/// Capability-based service discovery (wateringHole pattern)
///
/// ✅ Demonstrates:
/// - Discover by capability, not by primal name
/// - Endpoints resolved at runtime (config or mDNS)
/// - No hardcoded primal names or ports
pub async fn discover_service(capability: &str) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    use std::collections::HashMap;
    use toadstool_common::primal_discovery::{DiscoveryConfig, PrimalDiscovery};
    use toadstool_config::config_utils::ConfigUtils;
    use toadstool_config::ports::capability_fallback;

    // Build fallbacks from config — no hardcoded ports
    let bind_host = ConfigUtils::get_bind_address();
    let mut fallbacks = HashMap::new();
    fallbacks.insert(
        "coordination".to_string(),
        std::env::var("TOADSTOOL_COORDINATION_URL").unwrap_or_else(|_| {
            format!("http://{bind_host}:{}", capability_fallback::COORDINATION)
        }),
    );
    fallbacks.insert(
        "storage".to_string(),
        std::env::var("TOADSTOOL_STORAGE_URL")
            .unwrap_or_else(|_| format!("http://{bind_host}:{}", capability_fallback::STORAGE)),
    );

    let discovery = PrimalDiscovery::with_config(DiscoveryConfig {
        fallbacks,
        ..Default::default()
    })?;

    // Discover by capability — returns URL from runtime resolution
    match discovery.find_capability(capability).await {
        Ok(endpoint) => Ok(vec![endpoint.url().to_string()]),
        Err(_) => Ok(vec![]),
    }
}

/// Efficient data structure pattern
///
/// ❌ OLD: Arc<`RwLock`<`HashMap`<K, V>>> (complex, lock contention)
///
/// ✅ NEW: Consider alternatives:
/// - `DashMap` for concurrent access
/// - mpsc channels with actor pattern
/// - `Arc<T>` for immutable shared data
pub struct ModernDataStore {
    // ✅ Immutable shared config
    config: Arc<ServiceConfig>,

    // ✅ For concurrent mutable access, consider DashMap in production
    // (using std HashMap here for simplicity)
    cache: std::sync::Arc<std::sync::RwLock<HashMap<String, String>>>,
}

impl ModernDataStore {
    #[must_use]
    pub fn new(config: ServiceConfig) -> Self {
        Self {
            config: Arc::new(config),
            cache: Arc::new(std::sync::RwLock::new(HashMap::new())),
        }
    }

    /// ✅ Borrow when possible
    #[must_use]
    pub fn get_config(&self) -> &Arc<ServiceConfig> {
        &self.config
    }

    /// ✅ Proper error handling with Result
    pub fn get_cached(&self, key: &str) -> Result<Option<String>, &'static str> {
        let cache = self.cache.read().map_err(|_| "Cache lock poisoned")?;

        Ok(cache.get(key).cloned())
    }
}

/// Entry API pattern for efficient `HashMap` operations
///
/// ❌ OLD: Multiple lookups
/// ```ignore
/// if !map.contains_key(key) {
///     map.insert(key, default());
/// }
/// let value = map.get(key).unwrap();
/// ```
///
/// ✅ NEW: Single lookup with entry API
#[must_use]
pub fn entry_api_example(key: String) -> String {
    let mut cache: HashMap<String, String> = HashMap::new();

    // ✅ Efficient: single lookup, no unwrap
    cache
        .entry(key.clone())
        .or_insert_with(|| format!("default-{key}"))
        .clone()
}

/// Modern option handling
///
/// ❌ OLD:
/// ```ignore
/// let value = option.unwrap();
/// let value = option.expect("message");
/// ```
///
/// ✅ NEW: Use combinators and ? operator
pub fn modern_option_handling(input: Option<String>) -> Result<String, Box<dyn std::error::Error>> {
    // ✅ Use ok_or for conversion to Result
    let value = input.ok_or("Value not provided")?;

    // ✅ Use map/and_then for transformations
    let processed = Some(value)
        .map(|s| s.to_lowercase())
        .and_then(|s| if s.is_empty() { None } else { Some(s) })
        .ok_or("Value is empty")?;

    Ok(processed)
}

/// Demonstrating the "Know Yourself, Discover Others" pattern
///
/// ✅ Each service knows only itself
/// ✅ Discovers others at runtime
/// ❌ No hardcoded peer knowledge
pub struct SelfAwareService {
    /// ✅ Self-knowledge: own capabilities
    pub name: &'static str,
    pub capabilities: Vec<&'static str>,
    pub port: u16,
    // ❌ NO hardcoded peer endpoints
    // ❌ NO hardcoded peer ports
    // ✅ Discovers peers at runtime via capabilities
}

impl SelfAwareService {
    /// ✅ Constructor with only self-knowledge
    #[must_use]
    pub fn new(name: &'static str, port: u16, capabilities: Vec<&'static str>) -> Self {
        Self {
            name,
            capabilities,
            port,
        }
    }

    /// ✅ Discover peers by capability, not by name/port
    pub async fn discover_peer(
        &self,
        needed_capability: &str,
    ) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        // This would use multicast/DNS-SD/registry in production
        discover_service(needed_capability).await
    }

    /// ✅ Self-description for discovery by others
    #[must_use]
    pub fn advertise(&self) -> ServiceAdvertisement {
        ServiceAdvertisement {
            name: self.name.to_string(),
            capabilities: self.capabilities.iter().map(|&s| s.to_string()).collect(),
            endpoint: format!("{}:{}", "localhost", self.port),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ServiceAdvertisement {
    pub name: String,
    pub capabilities: Vec<String>,
    pub endpoint: String,
}

/// Example usage of all modern patterns
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Modern Rust Patterns Showcase\n");

    // ✅ 1. Builder pattern
    println!("1. Builder Pattern:");
    let config = ServiceConfigBuilder::new()
        .name("toadstool")
        .endpoint("http://localhost:8084")
        .timeout_secs(60)
        .retry_attempts(5)
        .build()?;
    println!("   Config: {config:?}\n");

    // ✅ 2. Error handling with ?
    println!("2. Modern Error Handling:");
    let endpoint = modern_error_handling_example()?;
    println!("   Endpoint: {endpoint}\n");

    // ✅ 3. Zero-copy with Cow
    println!("3. Zero-Copy String Handling:");
    let short = zero_copy_string_example("short");
    let long = zero_copy_string_example("this is a very long string that will be truncated");
    println!("   Short (borrowed): {short:?}");
    println!("   Long (owned): {long:?}\n");

    // ✅ 4. Async discovery
    println!("4. Async Service Discovery:");
    let coordinators = discover_service("coordination").await?;
    println!("   Found coordinators: {coordinators:?}\n");

    // ✅ 5. Self-aware service
    println!("5. Self-Aware Service Pattern:");
    let service = SelfAwareService::new("toadstool", 8084, vec!["compute", "orchestration"]);
    let ad = service.advertise();
    println!("   Advertisement: {ad:?}");

    let peers = service.discover_peer("storage").await?;
    println!("   Discovered storage services: {peers:?}\n");

    // ✅ 6. Modern data store
    println!("6. Modern Data Store:");
    let store = ModernDataStore::new(config);
    println!("   Store config name: {}", store.get_config().name);

    println!("\n✅ All patterns demonstrated successfully!");
    println!("\n💡 Key Takeaways:");
    println!("   - Use ? operator, not unwrap");
    println!("   - Borrow (&str) when possible, own (String) when needed");
    println!("   - Cow for flexible zero-copy");
    println!("   - Builder pattern for complex types");
    println!("   - Entry API for efficient HashMap ops");
    println!("   - Self-aware: know yourself, discover others");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_pattern() {
        let config = ServiceConfigBuilder::new()
            .name("test")
            .endpoint("http://test:8080")
            .build()
            .unwrap();

        assert_eq!(config.name, "test");
        assert_eq!(config.timeout_secs, 30); // default
        assert_eq!(config.retry_attempts, 3); // default
    }

    #[test]
    fn test_builder_validation() {
        let result = ServiceConfigBuilder::new().name("test").build();

        assert!(result.is_err()); // Missing required endpoint
    }

    #[test]
    fn test_zero_copy_borrowed() {
        let result = zero_copy_string_example("short");
        assert!(matches!(result, Cow::Borrowed(_)));
    }

    #[test]
    fn test_zero_copy_owned() {
        let result = zero_copy_string_example("very long string here");
        assert!(matches!(result, Cow::Owned(_)));
    }

    #[tokio::test]
    async fn test_discover_service() {
        let result = discover_service("coordination").await.unwrap();
        assert!(!result.is_empty());
    }

    #[test]
    fn test_self_aware_service() {
        let service = SelfAwareService::new("test", 8080, vec!["cap1", "cap2"]);

        let ad = service.advertise();
        assert_eq!(ad.name, "test");
        assert_eq!(ad.capabilities.len(), 2);
    }

    #[test]
    fn test_modern_error_handling() {
        let result = modern_error_handling_example();
        assert!(result.is_ok());
    }

    #[test]
    fn test_modern_option_handling() {
        let result = modern_option_handling(Some("TEST".to_string()));
        assert_eq!(result.unwrap(), "test");

        let empty_result = modern_option_handling(None);
        assert!(empty_result.is_err());
    }

    #[test]
    fn test_modern_data_store() {
        let config = ServiceConfigBuilder::new()
            .name("test")
            .endpoint("http://test")
            .build()
            .unwrap();

        let store = ModernDataStore::new(config);
        assert_eq!(store.get_config().name, "test");

        let cached = store.get_cached("nonexistent").unwrap();
        assert!(cached.is_none());
    }
}
