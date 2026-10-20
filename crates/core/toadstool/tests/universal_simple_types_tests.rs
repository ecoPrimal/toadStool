// SPDX-License-Identifier: AGPL-3.0-or-later
//! Simple type tests for universal.rs
//!
//! Tests cover basic type creation, serialization, and operations

use std::collections::HashMap;
use toadstool::universal::*;

#[cfg(test)]
mod security_tests {
    use super::*;

    #[test]
    fn test_security_level_ordering() {
        assert!(SecurityLevel::None < SecurityLevel::Standard);
        assert!(SecurityLevel::Standard < SecurityLevel::High);
        assert!(SecurityLevel::High < SecurityLevel::Maximum);
    }

    #[test]
    fn test_security_level_equality() {
        assert_eq!(SecurityLevel::None, SecurityLevel::None);
        assert_eq!(SecurityLevel::High, SecurityLevel::High);
        assert_ne!(SecurityLevel::None, SecurityLevel::Maximum);
    }

    #[test]
    fn test_security_level_clone() {
        let level = SecurityLevel::High;
        let cloned = level;
        assert_eq!(level, cloned);
    }

    #[test]
    fn test_all_security_levels() {
        let levels = vec![
            SecurityLevel::None,
            SecurityLevel::Standard,
            SecurityLevel::High,
            SecurityLevel::Maximum,
        ];

        assert_eq!(levels.len(), 4);
        for i in 0..levels.len() - 1 {
            assert!(levels[i] < levels[i + 1]);
        }
    }
}

#[cfg(test)]
mod network_location_tests {
    use super::*;

    #[test]
    fn test_network_location_creation() {
        let location = NetworkLocation {
            ip_address: "192.168.1.100".to_string(),
            subnet: Some("192.168.1.0/24".to_string()),
            network_id: Some("home-network".to_string()),
            geo_location: Some("US-CA-SF".to_string()),
        };

        assert_eq!(location.ip_address, "192.168.1.100");
        assert_eq!(location.subnet, Some("192.168.1.0/24".to_string()));
    }

    #[test]
    fn test_network_location_minimal() {
        let location = NetworkLocation {
            ip_address: "10.0.0.1".to_string(),
            subnet: None,
            network_id: None,
            geo_location: None,
        };

        assert_eq!(location.ip_address, "10.0.0.1");
        assert!(location.subnet.is_none());
    }

    #[test]
    fn test_network_location_ipv6() {
        let location = NetworkLocation {
            ip_address: "2001:0db8:85a3::8a2e:0370:7334".to_string(),
            subnet: Some("2001:0db8:85a3::/48".to_string()),
            network_id: Some("ipv6-net".to_string()),
            geo_location: None,
        };

        assert!(location.ip_address.contains("2001"));
        assert!(location.subnet.is_some());
    }

    #[test]
    fn test_network_location_clone() {
        let location = NetworkLocation {
            ip_address: "172.16.0.1".to_string(),
            subnet: Some("172.16.0.0/16".to_string()),
            network_id: None,
            geo_location: None,
        };

        let cloned = location.clone();
        assert_eq!(location, cloned);
    }
}

#[cfg(test)]
mod primal_context_tests {
    use super::*;

    #[test]
    fn test_primal_context_creation() {
        let mut metadata = HashMap::new();
        metadata.insert("app".to_string(), "web-service".to_string());

        let context = PrimalContext {
            user_id: "user-123".to_string(),
            device_id: "device-456".to_string(),
            session_id: "session-789".to_string(),
            network_location: NetworkLocation {
                ip_address: "192.168.1.1".to_string(),
                subnet: None,
                network_id: None,
                geo_location: None,
            },
            security_level: SecurityLevel::High,
            metadata,
        };

        assert_eq!(context.user_id, "user-123");
        assert_eq!(context.device_id, "device-456");
        assert_eq!(context.security_level, SecurityLevel::High);
    }

    #[test]
    fn test_primal_context_metadata() {
        let mut metadata = HashMap::new();
        metadata.insert("key1".to_string(), "value1".to_string());
        metadata.insert("key2".to_string(), "value2".to_string());

        let context = PrimalContext {
            user_id: "user-1".to_string(),
            device_id: "device-1".to_string(),
            session_id: "session-1".to_string(),
            network_location: NetworkLocation {
                ip_address: "10.0.0.1".to_string(),
                subnet: None,
                network_id: None,
                geo_location: None,
            },
            security_level: SecurityLevel::Standard,
            metadata: metadata.clone(),
        };

        assert_eq!(context.metadata.len(), 2);
        assert_eq!(context.metadata.get("key1"), Some(&"value1".to_string()));
    }

    #[test]
    fn test_primal_context_clone() {
        let context = PrimalContext {
            user_id: "user-clone".to_string(),
            device_id: "device-clone".to_string(),
            session_id: "session-clone".to_string(),
            network_location: NetworkLocation {
                ip_address: "127.0.0.1".to_string(),
                subnet: None,
                network_id: None,
                geo_location: None,
            },
            security_level: SecurityLevel::Maximum,
            metadata: HashMap::new(),
        };

        let cloned = context.clone();
        assert_eq!(context, cloned);
    }
}

#[cfg(test)]
mod primal_type_tests {
    use super::*;

    #[test]
    fn test_primal_type_variants() {
        let types = vec![
            PrimalType::Compute,
            PrimalType::Security,
            PrimalType::Storage,
            PrimalType::AI,
            PrimalType::Network,
            PrimalType::OS,
            PrimalType::Custom("CustomService".to_string()),
        ];

        assert_eq!(types.len(), 7);
    }

    #[test]
    fn test_primal_type_equality() {
        assert_eq!(PrimalType::Compute, PrimalType::Compute);
        assert_eq!(PrimalType::Security, PrimalType::Security);
        assert_ne!(PrimalType::Compute, PrimalType::Security);
    }

    #[test]
    fn test_primal_type_custom() {
        let custom1 = PrimalType::Custom("MyService".to_string());
        let custom2 = PrimalType::Custom("MyService".to_string());
        let custom3 = PrimalType::Custom("Other".to_string());

        assert_eq!(custom1, custom2);
        assert_ne!(custom1, custom3);
    }

    #[test]
    fn test_primal_type_clone() {
        let primal_type = PrimalType::AI;
        let cloned = primal_type.clone();
        assert_eq!(primal_type, cloned);
    }
}

#[cfg(test)]
mod platform_status_tests {
    use super::*;

    #[test]
    fn test_platform_status_variants() {
        let statuses = vec![
            PlatformStatus::Initializing,
            PlatformStatus::Running,
            PlatformStatus::Degraded,
            PlatformStatus::Stopped,
        ];

        assert_eq!(statuses.len(), 4);
    }

    #[test]
    fn test_platform_status_equality() {
        assert_eq!(PlatformStatus::Running, PlatformStatus::Running);
        assert_ne!(PlatformStatus::Running, PlatformStatus::Stopped);
    }

    #[test]
    fn test_platform_status_clone() {
        let status = PlatformStatus::Running;
        let cloned = status.clone();
        assert_eq!(status, cloned);
    }

    #[test]
    fn test_platform_status_lifecycle() {
        let lifecycle = vec![
            PlatformStatus::Initializing,
            PlatformStatus::Running,
            PlatformStatus::Stopped,
        ];

        assert_eq!(lifecycle.len(), 3);
        assert_eq!(lifecycle[0], PlatformStatus::Initializing);
        assert_eq!(lifecycle[lifecycle.len() - 1], PlatformStatus::Stopped);
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[test]
    fn test_complete_primal_context_workflow() {
        let mut metadata = HashMap::new();
        metadata.insert("environment".to_string(), "production".to_string());
        metadata.insert("version".to_string(), "1.0.0".to_string());

        let context = PrimalContext {
            user_id: "user-prod-001".to_string(),
            device_id: "device-web-server".to_string(),
            session_id: "sess-abc123".to_string(),
            network_location: NetworkLocation {
                ip_address: "10.100.1.50".to_string(),
                subnet: Some("10.100.0.0/16".to_string()),
                network_id: Some("prod-network".to_string()),
                geo_location: Some("US-WEST-2".to_string()),
            },
            security_level: SecurityLevel::High,
            metadata,
        };

        assert_eq!(context.user_id, "user-prod-001");
        assert_eq!(context.security_level, SecurityLevel::High);
        assert_eq!(context.metadata.len(), 2);
        assert_eq!(context.network_location.ip_address, "10.100.1.50");
    }

    #[test]
    fn test_primal_type_matching() {
        let types = vec![
            (PrimalType::Compute, "compute"),
            (PrimalType::Storage, "storage"),
            (PrimalType::Network, "network"),
            (PrimalType::Custom("Test".to_string()), "custom"),
        ];

        for (primal_type, expected) in types {
            let category = match primal_type {
                PrimalType::Compute => "compute",
                PrimalType::Security => "security",
                PrimalType::Storage => "storage",
                PrimalType::AI => "ai",
                PrimalType::Network => "network",
                PrimalType::OS => "os",
                PrimalType::Custom(_) => "custom",
            };

            assert_eq!(category, expected);
        }
    }

    #[test]
    fn test_security_level_upgrade_path() {
        let start_level = SecurityLevel::None;
        let upgrade_path = vec![
            SecurityLevel::Standard,
            SecurityLevel::High,
            SecurityLevel::Maximum,
        ];

        let mut current = start_level;
        for target in upgrade_path {
            assert!(target > current);
            current = target;
        }

        assert_eq!(current, SecurityLevel::Maximum);
    }
}
