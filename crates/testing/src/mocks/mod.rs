// SPDX-License-Identifier: AGPL-3.0-or-later
// ToadStool - Universal Compute Platform
// Copyright (C) 2025 ToadStool Development Team
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Mock implementations for testing

// Hardware mocks for headless CI parity
pub mod hardware;
pub mod v4l2;
pub mod vfio;

// Resource monitors
pub mod resource_monitors;

// Runtime engines
pub mod runtime_engines;

// Export the successful mocks that compile
pub use hardware::{MockGpuAdapter, MockHardwareFleet, MockNpuBackend, MockNpuInferenceResult};
pub use resource_monitors::MockResourceMonitor;
pub use runtime_engines::MockRuntimeEngine;
pub use v4l2::{CaptureFormat, FramePattern, MockV4l2Config, MockV4l2Device, MockV4l2Error};
pub use vfio::{AccessOp, MockVfioDevice, MockVfioError, RegisterAccessEntry};

pub use cellmembrane_types::SecurityLevel;
pub use lightweight::{
    MockConfigLoader, MockSecurityContext, MockWorkloadSpec, ResourceRequirements, WorkloadType,
};

/// Lightweight test doubles for configuration, security, and workload testing.
pub mod lightweight {
    use serde::{Deserialize, Serialize};
    use std::collections::HashMap;

    /// Mock configuration loader for testing configuration functionality
    #[derive(Debug, Clone)]
    pub struct MockConfigLoader {
        configs: HashMap<String, serde_json::Value>,
    }

    impl Default for MockConfigLoader {
        fn default() -> Self {
            Self::new()
        }
    }

    impl MockConfigLoader {
        /// Create an empty config loader for testing
        #[must_use]
        pub fn new() -> Self {
            Self {
                configs: HashMap::new(),
            }
        }

        /// Add a configuration value for testing
        pub fn add_config(&mut self, key: String, value: serde_json::Value) {
            self.configs.insert(key, value);
        }

        /// Load configuration by key
        #[must_use]
        pub fn load_config(&self, key: &str) -> Option<&serde_json::Value> {
            self.configs.get(key)
        }

        /// Load configuration with default
        pub fn load_config_or_default<T>(&self, key: &str, default: T) -> T
        where
            T: for<'de> Deserialize<'de> + Clone,
        {
            self.configs
                .get(key)
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or(default)
        }
    }

    /// Mock security context for testing security functionality
    #[derive(Debug, Clone)]
    pub struct MockSecurityContext {
        security_level: SecurityLevel,
        permissions: Vec<String>,
        isolation_enabled: bool,
    }

    // Canonical SecurityLevel from cellmembrane-types.
    // Local enum removed — was (Low, Medium, High, Critical).
    // Mapping: Low→None, Medium→Standard, High→High, Critical→Maximum.
    pub use cellmembrane_types::SecurityLevel;

    impl Default for MockSecurityContext {
        fn default() -> Self {
            Self::new()
        }
    }

    impl MockSecurityContext {
        /// Create a default security context (Medium level, read/write/execute)
        #[must_use]
        pub fn new() -> Self {
            Self {
                security_level: SecurityLevel::Standard,
                permissions: vec![
                    "read".to_string(),
                    "write".to_string(),
                    "execute".to_string(),
                ],
                isolation_enabled: true,
            }
        }

        /// Create a high-security context
        #[must_use]
        pub fn new_high_security() -> Self {
            Self {
                security_level: SecurityLevel::High,
                permissions: vec!["read".to_string()],
                isolation_enabled: true,
            }
        }

        /// Create a low-security context
        #[must_use]
        pub fn new_low_security() -> Self {
            Self {
                security_level: SecurityLevel::None,
                permissions: vec![
                    "read".to_string(),
                    "write".to_string(),
                    "execute".to_string(),
                    "network".to_string(),
                    "filesystem".to_string(),
                ],
                isolation_enabled: false,
            }
        }

        /// Check if permission is granted
        #[must_use]
        pub fn has_permission(&self, permission: &str) -> bool {
            self.permissions.contains(&permission.to_string())
        }

        /// Get security level
        #[must_use]
        pub const fn get_security_level(&self) -> &SecurityLevel {
            &self.security_level
        }

        /// Check if isolation is enabled
        #[must_use]
        pub const fn is_isolation_enabled(&self) -> bool {
            self.isolation_enabled
        }
    }

    /// Mock workload specification for testing workload functionality
    #[derive(Debug, Clone)]
    pub struct MockWorkloadSpec {
        workload_type: WorkloadType,
        resource_requirements: ResourceRequirements,
        environment: HashMap<String, String>,
        command: Option<String>,
        args: Vec<String>,
    }

    /// Workload type for mock workload specs
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub enum WorkloadType {
        /// Native binary execution
        Native,
        /// Container execution (Docker, etc.)
        Container,
        /// WebAssembly module
        Wasm,
        /// GPU-accelerated workload
        Gpu,
    }

    /// Resource requirements for workload specs
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ResourceRequirements {
        /// CPU cores required
        pub cpu_cores: Option<f64>,
        /// Memory in bytes
        pub memory_bytes: Option<u64>,
        /// Storage in bytes
        pub storage_bytes: Option<u64>,
        /// Network bandwidth in bytes/sec
        pub network_bandwidth: Option<u64>,
        /// GPU units required
        pub gpu_units: Option<u32>,
    }

    impl Default for MockWorkloadSpec {
        fn default() -> Self {
            Self::new()
        }
    }

    impl MockWorkloadSpec {
        /// Create a default workload spec (Native, 1 CPU, 1GB memory)
        #[must_use]
        pub fn new() -> Self {
            Self {
                workload_type: WorkloadType::Native,
                resource_requirements: ResourceRequirements {
                    cpu_cores: Some(1.0),
                    memory_bytes: Some(1024 * 1024 * 1024), // 1GB
                    storage_bytes: Some(1024 * 1024 * 1024), // 1GB
                    network_bandwidth: None,
                    gpu_units: None,
                },
                environment: HashMap::new(),
                command: None,
                args: Vec::new(),
            }
        }

        /// Create a container workload spec
        #[must_use]
        pub fn new_container(image: &str) -> Self {
            let mut spec = Self::new();
            spec.workload_type = WorkloadType::Container;
            spec.environment
                .insert("CONTAINER_IMAGE".to_string(), image.to_string());
            spec
        }

        /// Create a WASM workload spec
        #[must_use]
        pub fn new_wasm(module_path: &str) -> Self {
            let mut spec = Self::new();
            spec.workload_type = WorkloadType::Wasm;
            spec.environment
                .insert("WASM_MODULE".to_string(), module_path.to_string());
            spec
        }

        /// Create a GPU workload spec
        #[must_use]
        pub fn new_gpu(gpu_units: u32) -> Self {
            let mut spec = Self::new();
            spec.workload_type = WorkloadType::Gpu;
            spec.resource_requirements.gpu_units = Some(gpu_units);
            spec
        }

        /// Set command and arguments
        #[must_use]
        pub fn with_command(mut self, command: &str, args: Vec<String>) -> Self {
            self.command = Some(command.to_string());
            self.args = args;
            self
        }

        /// Set resource requirements
        #[must_use]
        pub const fn with_resources(mut self, requirements: ResourceRequirements) -> Self {
            self.resource_requirements = requirements;
            self
        }

        /// Add environment variable
        #[must_use]
        pub fn with_env(mut self, key: &str, value: &str) -> Self {
            self.environment.insert(key.to_string(), value.to_string());
            self
        }

        /// Get workload type
        #[must_use]
        pub const fn get_workload_type(&self) -> &WorkloadType {
            &self.workload_type
        }

        /// Get resource requirements
        #[must_use]
        pub const fn get_resource_requirements(&self) -> &ResourceRequirements {
            &self.resource_requirements
        }

        /// Get environment variables
        #[must_use]
        pub const fn get_environment(&self) -> &HashMap<String, String> {
            &self.environment
        }

        /// Get command
        #[must_use]
        pub const fn get_command(&self) -> &Option<String> {
            &self.command
        }

        /// Get arguments
        #[must_use]
        pub fn get_args(&self) -> &[String] {
            &self.args
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_mock_config_loader() {
        let mut loader = MockConfigLoader::new();

        // Test adding configuration
        loader.add_config("test_key".to_string(), json!({"value": 42}));

        // Test loading configuration
        let config = loader.load_config("test_key");
        assert!(config.is_some());
        assert_eq!(config.unwrap(), &json!({"value": 42}));

        // Test loading non-existent key
        let missing = loader.load_config("missing_key");
        assert!(missing.is_none());

        // Test loading with default
        let default_value = loader.load_config_or_default("missing_key", 100);
        assert_eq!(default_value, 100);
    }

    #[test]
    fn test_mock_security_context() {
        let context = MockSecurityContext::new();

        // Test default permissions
        assert!(context.has_permission("read"));
        assert!(context.has_permission("write"));
        assert!(context.has_permission("execute"));
        assert!(!context.has_permission("network"));

        // Test security level
        matches!(context.get_security_level(), SecurityLevel::Standard);

        // Test isolation
        assert!(context.is_isolation_enabled());

        // Test high security context
        let high_context = MockSecurityContext::new_high_security();
        assert!(high_context.has_permission("read"));
        assert!(!high_context.has_permission("write"));
        matches!(high_context.get_security_level(), SecurityLevel::High);

        // Test low security context
        let low_context = MockSecurityContext::new_low_security();
        assert!(low_context.has_permission("network"));
        assert!(!low_context.is_isolation_enabled());
    }

    #[test]
    fn test_mock_workload_spec() {
        let spec = MockWorkloadSpec::new();

        // Test default values
        matches!(spec.get_workload_type(), WorkloadType::Native);
        assert_eq!(spec.get_resource_requirements().cpu_cores, Some(1.0));
        assert_eq!(
            spec.get_resource_requirements().memory_bytes,
            Some(1024 * 1024 * 1024)
        );
        assert!(spec.get_command().is_none());
        assert!(spec.get_args().is_empty());

        // Test container workload
        let container_spec = MockWorkloadSpec::new_container("ubuntu:latest");
        matches!(container_spec.get_workload_type(), WorkloadType::Container);
        assert_eq!(
            container_spec.get_environment().get("CONTAINER_IMAGE"),
            Some(&"ubuntu:latest".to_string())
        );

        // Test WASM workload
        let wasm_spec = MockWorkloadSpec::new_wasm("module.wasm");
        matches!(wasm_spec.get_workload_type(), WorkloadType::Wasm);
        assert_eq!(
            wasm_spec.get_environment().get("WASM_MODULE"),
            Some(&"module.wasm".to_string())
        );

        // Test GPU workload
        let gpu_spec = MockWorkloadSpec::new_gpu(2);
        matches!(gpu_spec.get_workload_type(), WorkloadType::Gpu);
        assert_eq!(gpu_spec.get_resource_requirements().gpu_units, Some(2));

        // Test builder pattern
        let custom_spec = MockWorkloadSpec::new()
            .with_command("echo", vec!["hello".to_string(), "world".to_string()])
            .with_env("TEST_VAR", "test_value");

        assert_eq!(custom_spec.get_command(), &Some("echo".to_string()));
        assert_eq!(
            custom_spec.get_args(),
            &vec!["hello".to_string(), "world".to_string()]
        );
        assert_eq!(
            custom_spec.get_environment().get("TEST_VAR"),
            Some(&"test_value".to_string())
        );
    }
}
