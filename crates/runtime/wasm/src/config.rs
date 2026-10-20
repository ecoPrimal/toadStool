// SPDX-License-Identifier: AGPL-3.0-or-later
//! WebAssembly runtime configuration
//!
//! This module defines configuration types for the WASM runtime engine,
//! following modern Rust best practices for type-driven configuration.

use std::time::Duration;

const WASM_MODULE_CACHE_TTL_HOURS: u64 = 24;

use toadstool_common::config_bases::CacheConfig;

// Canonical SecurityLevel from cellmembrane-types.
// Local enum removed — was (None, Basic, Strict, Maximum).
// Mapping: None→None, Basic→Standard, Strict→High, Maximum→Maximum.
// Canonical already provides enforces_memory_limits() and requires_hardware().
pub use cellmembrane_types::SecurityLevel;

/// Configuration for WebAssembly runtime engine
///
/// Follows the builder pattern for ergonomic configuration.
#[derive(Debug, Clone)]
pub struct WasmRuntimeConfig {
    /// Module caching configuration
    pub cache: CacheConfig,

    /// Security isolation level
    pub security_level: SecurityLevel,

    /// Memory limits in megabytes
    pub max_memory_mb: u64,

    /// Maximum memory pages (WASM page = 64KB)
    pub max_pages: u32,

    /// Execution timeout in milliseconds
    pub execution_timeout_ms: u64,

    /// Module load timeout in milliseconds
    pub module_load_timeout_ms: u64,

    /// Fuel limit for execution (None = unlimited)
    pub fuel_limit: Option<u64>,

    /// Component model configuration (optional)
    /// EVOLVED: Added for complete component model support
    pub component_model: Option<crate::component_model::ComponentModelConfig>,
}

impl Default for WasmRuntimeConfig {
    fn default() -> Self {
        Self {
            // Optimize cache for WASM modules - 512 entries, 24 hour TTL
            cache: CacheConfig {
                max_entries: 512,
                ttl: Duration::from_secs(WASM_MODULE_CACHE_TTL_HOURS * 3600),
                ..CacheConfig::default()
            },
            security_level: SecurityLevel::High,
            max_memory_mb: 128,
            max_pages: 2048,
            execution_timeout_ms: 30000,
            module_load_timeout_ms: 10000,
            fuel_limit: Some(1_000_000),
            // Component model disabled by default for backward compatibility
            component_model: None,
        }
    }
}

impl WasmRuntimeConfig {
    /// Create a new configuration builder
    pub fn builder() -> WasmRuntimeConfigBuilder {
        WasmRuntimeConfigBuilder::default()
    }

    /// Validate configuration parameters
    pub fn validate(&self) -> Result<(), String> {
        if self.max_memory_mb == 0 {
            return Err("Memory limit cannot be zero".to_string());
        }

        if self.execution_timeout_ms == 0 {
            return Err("Execution timeout cannot be zero".to_string());
        }

        if self.module_load_timeout_ms == 0 {
            return Err("Module load timeout cannot be zero".to_string());
        }

        if self.max_pages == 0 {
            return Err("Maximum pages cannot be zero".to_string());
        }

        Ok(())
    }
}

/// Builder for `WasmRuntimeConfig`
///
/// Provides a fluent API for constructing runtime configurations.
#[derive(Debug, Default)]
pub struct WasmRuntimeConfigBuilder {
    cache: Option<CacheConfig>,
    security_level: Option<SecurityLevel>,
    max_memory_mb: Option<u64>,
    max_pages: Option<u32>,
    execution_timeout_ms: Option<u64>,
    module_load_timeout_ms: Option<u64>,
    #[expect(
        clippy::option_option,
        reason = "None=unset, Some(None)=explicitly unlimited, Some(Some(n))=limit"
    )]
    fuel_limit: Option<Option<u64>>,
    /// EVOLVED: Component model configuration
    component_model: Option<crate::component_model::ComponentModelConfig>,
}

impl WasmRuntimeConfigBuilder {
    /// Set cache configuration
    #[must_use]
    pub const fn cache(mut self, cache: CacheConfig) -> Self {
        self.cache = Some(cache);
        self
    }

    /// Set security level
    #[must_use]
    pub const fn security_level(mut self, level: SecurityLevel) -> Self {
        self.security_level = Some(level);
        self
    }

    /// Set maximum memory in megabytes
    #[must_use]
    pub const fn max_memory_mb(mut self, mb: u64) -> Self {
        self.max_memory_mb = Some(mb);
        self
    }

    /// Set maximum memory pages
    #[must_use]
    pub const fn max_pages(mut self, pages: u32) -> Self {
        self.max_pages = Some(pages);
        self
    }

    /// Set execution timeout in milliseconds
    #[must_use]
    pub const fn execution_timeout_ms(mut self, ms: u64) -> Self {
        self.execution_timeout_ms = Some(ms);
        self
    }

    /// Set module load timeout in milliseconds
    #[must_use]
    pub const fn module_load_timeout_ms(mut self, ms: u64) -> Self {
        self.module_load_timeout_ms = Some(ms);
        self
    }

    /// Set fuel limit (None = unlimited)
    #[must_use]
    pub const fn fuel_limit(mut self, limit: Option<u64>) -> Self {
        self.fuel_limit = Some(limit);
        self
    }

    /// Enable component model support
    /// EVOLVED: Complete component model integration
    #[must_use]
    pub const fn with_component_model(
        mut self,
        config: crate::component_model::ComponentModelConfig,
    ) -> Self {
        self.component_model = Some(config);
        self
    }

    /// Build the configuration
    pub fn build(self) -> WasmRuntimeConfig {
        let default = WasmRuntimeConfig::default();
        WasmRuntimeConfig {
            cache: self.cache.unwrap_or(default.cache),
            security_level: self.security_level.unwrap_or(default.security_level),
            max_memory_mb: self.max_memory_mb.unwrap_or(default.max_memory_mb),
            max_pages: self.max_pages.unwrap_or(default.max_pages),
            execution_timeout_ms: self
                .execution_timeout_ms
                .unwrap_or(default.execution_timeout_ms),
            module_load_timeout_ms: self
                .module_load_timeout_ms
                .unwrap_or(default.module_load_timeout_ms),
            fuel_limit: self.fuel_limit.unwrap_or(default.fuel_limit),
            component_model: self.component_model.or(default.component_model),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = WasmRuntimeConfig::default();
        assert_eq!(config.security_level, SecurityLevel::High);
        assert_eq!(config.max_memory_mb, 128);
        assert!(config.fuel_limit.is_some());
    }

    #[test]
    fn test_config_validation() {
        let mut config = WasmRuntimeConfig::default();
        assert!(config.validate().is_ok());

        config.max_memory_mb = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_builder() {
        let config = WasmRuntimeConfig::builder()
            .max_memory_mb(256)
            .fuel_limit(Some(5_000_000))
            .build();

        assert_eq!(config.max_memory_mb, 256);
        assert_eq!(config.fuel_limit, Some(5_000_000));
    }
}
