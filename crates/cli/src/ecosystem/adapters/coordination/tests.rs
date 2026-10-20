// SPDX-License-Identifier: AGPL-3.0-or-later

use super::*;

#[test]
fn test_service_info_creation() {
    let info = ServiceInfo::new("test-service", "test-service")
        .with_capability("compute.native")
        .with_endpoint(format!(
            "{}{}:{}",
            toadstool_common::constants::network::HTTP_PROTOCOL,
            toadstool_common::constants::DEFAULT_HOSTNAME,
            8080
        ));

    assert_eq!(info.name, "test-service");
    assert_eq!(info.capabilities.len(), 1);
}

#[test]
fn test_service_info_with_metadata() {
    let info = {
        let mut si = ServiceInfo::new("storage-svc", "storage-svc")
            .with_capability("storage.s3")
            .with_capability("storage.kv")
            .with_endpoint("http://localhost:9000");
        si.metadata.insert("version".to_string(), "1.0".to_string());
        si.metadata
            .insert("region".to_string(), "us-east".to_string());
        si
    };
    assert_eq!(info.capabilities.len(), 2);
    assert_eq!(info.metadata.get("version"), Some(&"1.0".to_string()));
}

#[test]
fn test_registration_token() {
    let token = RegistrationToken {
        token: "abc-123-xyz".to_string(),
    };
    assert_eq!(token.token, "abc-123-xyz");
}

#[test]
fn test_registration_token_clone() {
    let token = RegistrationToken {
        token: "test-token".to_string(),
    };
    let cloned = token.clone();
    assert_eq!(token.token, cloned.token);
}

#[test]
fn test_peer_info_creation() {
    let peer = PeerInfo {
        name: "songbird".to_string(),
        endpoint: "http://192.168.1.10:8080".to_string(),
        capabilities: vec!["discovery".to_string(), "coordination".to_string()],
        health: "healthy".to_string(),
    };
    assert_eq!(peer.name, "songbird");
    assert_eq!(peer.capabilities.len(), 2);
    assert_eq!(peer.health, "healthy");
}

#[test]
fn test_lock_handle_creation() {
    let handle = LockHandle {
        lock_name: "migration-lock".to_string(),
        lock_id: "uuid-12345".to_string(),
    };
    assert_eq!(handle.lock_name, "migration-lock");
    assert_eq!(handle.lock_id, "uuid-12345");
}

#[test]
fn test_lock_handle_clone() {
    let handle = LockHandle {
        lock_name: "lock".to_string(),
        lock_id: "id".to_string(),
    };
    let cloned = handle.clone();
    assert_eq!(handle.lock_name, cloned.lock_name);
}

#[test]
fn test_coordination_adapter_new() {
    use crate::ecosystem::adapters::AdapterFactory;
    let factory = AdapterFactory::new();
    let adapter = factory.coordination_adapter().unwrap();
    let _ = adapter;
}

#[tokio::test]
async fn test_register_service_no_coordination_returns_err() {
    use crate::ecosystem::adapters::AdapterFactory;

    let factory = AdapterFactory::new();
    let coordination = factory.coordination_adapter().unwrap();

    let service_info = ServiceInfo::new("test-svc", "test-svc")
        .with_capability("compute.native")
        .with_endpoint("http://127.0.0.1:9999");

    let result = coordination.register_service(service_info).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_discover_peers_no_service_returns_err() {
    use crate::ecosystem::adapters::AdapterFactory;

    let factory = AdapterFactory::new();
    let coordination = factory.coordination_adapter().unwrap();

    let result = coordination.discover_peers(Some("crypto.*")).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_send_heartbeat_no_service_returns_err() {
    use crate::ecosystem::adapters::AdapterFactory;

    let factory = AdapterFactory::new();
    let coordination = factory.coordination_adapter().unwrap();

    let token = RegistrationToken {
        token: "test-token".to_string(),
    };
    let result = coordination.send_heartbeat(&token).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_unregister_service_no_service_returns_err() {
    use crate::ecosystem::adapters::AdapterFactory;

    let factory = AdapterFactory::new();
    let coordination = factory.coordination_adapter().unwrap();

    let token = RegistrationToken {
        token: "test-token".to_string(),
    };
    let result = coordination.unregister_service(&token).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_get_config_no_service_returns_err() {
    use crate::ecosystem::adapters::AdapterFactory;

    let factory = AdapterFactory::new();
    let coordination = factory.coordination_adapter().unwrap();

    let result = coordination.get_config("test_key").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_set_config_no_service_returns_err() {
    use crate::ecosystem::adapters::AdapterFactory;

    let factory = AdapterFactory::new();
    let coordination = factory.coordination_adapter().unwrap();

    let result = coordination
        .set_config("key", serde_json::json!("value"))
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_acquire_lock_no_service_returns_err() {
    use crate::ecosystem::adapters::AdapterFactory;

    let factory = AdapterFactory::new();
    let coordination = factory.coordination_adapter().unwrap();

    let result = coordination.acquire_lock("migration-lock", 30).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_release_lock_no_service_returns_err() {
    use crate::ecosystem::adapters::AdapterFactory;

    let factory = AdapterFactory::new();
    let coordination = factory.coordination_adapter().unwrap();

    let handle = LockHandle {
        lock_name: "lock".to_string(),
        lock_id: "id-123".to_string(),
    };
    let result = coordination.release_lock(handle).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_discover_peers_none_filter() {
    use crate::ecosystem::adapters::AdapterFactory;

    let factory = AdapterFactory::new();
    let coordination = factory.coordination_adapter().unwrap();

    // Same error - no service, but tests the None path for capability_filter
    let result = coordination.discover_peers(None).await;
    assert!(result.is_err());
}
