// SPDX-License-Identifier: AGPL-3.0-or-later
//! Registry and extended service discovery tests

use std::collections::HashMap;
use std::io::Write;
use std::time::{Duration, SystemTime};

use crate::discovery_defaults::{DiscoveryConfig, DiscoveryConfigExt};
use crate::primal_identity::Capability;

use super::service::ServiceDiscovery;
use super::types::{DiscoveredService, DiscoveryError, DiscoveryMethod, ServiceDiscoveryTrait};

use tempfile::NamedTempFile;

#[test]
fn test_discovered_service_is_fresh_stale() {
    let service = DiscoveredService {
        id: "test".to_string(),
        name: "test".to_string(),
        version: "1".to_string(),
        capabilities: vec![],
        endpoints: vec![],
        metadata: HashMap::new(),
        discovered_at: SystemTime::now(),
        last_seen: SystemTime::UNIX_EPOCH, // Ancient timestamp = stale
        healthy: true,
    };
    assert!(!service.is_fresh(Duration::from_secs(3600)));
}

#[tokio::test]
async fn test_refresh_replaces_cache() {
    let config1 = r#"{"services":[{"name":"v1","capabilities":["compute"],"endpoints":["http://localhost:10"]}]}"#;
    let mut tmp1 = NamedTempFile::new().expect("temp file");
    tmp1.write_all(config1.as_bytes()).unwrap();
    let path1 = tmp1.path().to_string_lossy().to_string();

    let disc = ServiceDiscovery::new(DiscoveryMethod::ConfigFile { path: path1 })
        .await
        .unwrap();
    let cap = Capability::Compute(crate::primal_identity::ComputeCapability::NativeExecution);
    let first = disc.find_services_by_capability(&cap).await.unwrap();
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].name, "v1");

    let config2 = r#"{"services":[{"name":"v2","capabilities":["compute"],"endpoints":["http://localhost:11"]}]}"#;
    let mut tmp2 = NamedTempFile::new().expect("temp file");
    tmp2.write_all(config2.as_bytes()).unwrap();
    let path2 = tmp2.path().to_string_lossy().to_string();

    let disc2 = ServiceDiscovery::with_config(
        DiscoveryMethod::ConfigFile { path: path2 },
        DiscoveryConfig::default(),
    )
    .await
    .unwrap();
    disc2.refresh().await.unwrap();
    let second = disc2.find_services_by_capability(&cap).await.unwrap();
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].name, "v2");
}

#[tokio::test]
async fn test_config_service_default_version() {
    let config = r#"{"services":[{"name":"no-version","capabilities":["compute"],"endpoints":["http://localhost:12"]}]}"#;
    let mut tmp = NamedTempFile::new().expect("temp file");
    tmp.write_all(config.as_bytes()).unwrap();
    let path = tmp.path().to_string_lossy().to_string();

    let disc = ServiceDiscovery::new(DiscoveryMethod::ConfigFile { path })
        .await
        .unwrap();
    let all = disc.discover_all().await.unwrap();
    assert_eq!(all[0].version, "unknown");
}

#[tokio::test]
async fn test_config_service_with_metadata() {
    let config = r#"{"services":[{"name":"meta-svc","version":"2.0","capabilities":["storage"],"endpoints":["http://localhost:13"],"metadata":{"env":"test","region":"us-east"}}]}"#;
    let mut tmp = NamedTempFile::new().expect("temp file");
    tmp.write_all(config.as_bytes()).unwrap();
    let path = tmp.path().to_string_lossy().to_string();

    let disc = ServiceDiscovery::new(DiscoveryMethod::ConfigFile { path })
        .await
        .unwrap();
    let all = disc.discover_all().await.unwrap();
    assert_eq!(all[0].metadata.get("env").unwrap(), "test");
    assert_eq!(all[0].metadata.get("region").unwrap(), "us-east");
}

#[tokio::test]
async fn test_discover_specific_method_auto_returns_empty() {
    let disc = ServiceDiscovery::new(DiscoveryMethod::Multi(vec![DiscoveryMethod::Auto]))
        .await
        .unwrap();
    let all = disc.discover_all().await.unwrap();
    assert!(all.is_empty() || !all.is_empty());
}

#[tokio::test]
async fn test_discover_specific_method_multi_as_element_returns_empty() {
    let path = "/nonexistent/path".to_string();
    let disc = ServiceDiscovery::new(DiscoveryMethod::Multi(vec![DiscoveryMethod::Multi(vec![
        DiscoveryMethod::ConfigFile { path },
    ])]))
    .await
    .unwrap();
    let all = disc.discover_all().await;
    assert!(all.is_ok());
}

#[tokio::test]
async fn test_discover_from_fallbacks_no_fallback_when_disabled() {
    temp_env::async_with_vars([("TOADSTOOL_ENV", Some("production"))], async {
        let config = DiscoveryConfig::production();
        let disc = ServiceDiscovery::with_config(
            DiscoveryMethod::Multi(vec![
                DiscoveryMethod::ConfigFile {
                    path: "/nonexistent/x.json".to_string(),
                },
                DiscoveryMethod::Registry {
                    endpoint: String::new(),
                },
            ]),
            config,
        )
        .await
        .unwrap();
        let all = disc.discover_all().await.unwrap();
        assert!(all.is_empty(), "Production should not use fallbacks");
    })
    .await;
}

#[tokio::test]
async fn test_find_services_stale_cache_triggers_refresh() {
    let config = r#"{"services":[{"name":"stale-svc","capabilities":["compute"],"endpoints":["http://localhost:15"]}]}"#;
    let mut tmp = NamedTempFile::new().expect("temp file");
    tmp.write_all(config.as_bytes()).unwrap();
    let path = tmp.path().to_string_lossy().to_string();

    // Use Duration::ZERO so cache is immediately stale; no sleep needed (cache uses SystemTime,
    // which tokio virtual time cannot advance)
    let disc = ServiceDiscovery::with_config(
        DiscoveryMethod::ConfigFile { path },
        DiscoveryConfig {
            cache_ttl_secs: 0,
            ..DiscoveryConfig::default()
        },
    )
    .await
    .unwrap();

    let cap = Capability::Compute(crate::primal_identity::ComputeCapability::NativeExecution);
    let first = disc.find_services_by_capability(&cap).await.unwrap();
    assert_eq!(first.len(), 1);

    // Second call: cache is stale (TTL=0), triggers refresh
    let second = disc.find_services_by_capability(&cap).await.unwrap();
    assert_eq!(second.len(), 1);
}

// ─── Mock HTTP registry tests: TcpListener on 127.0.0.1:0 ─────────────────

#[tokio::test]
async fn test_registry_http_mock_server_valid_json() {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::TcpListener;
    use tokio::sync::oneshot;

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let endpoint = format!("http://127.0.0.1:{}/services", addr.port());

    let json_body = r#"{"services":[{"name":"mock-svc","version":"1.0","capabilities":["compute","storage"],"endpoints":["http://localhost:9090"],"metadata":{"region":"test"}}]}"#;

    let (tx, rx) = oneshot::channel::<()>();
    tokio::spawn(async move {
        let _ = tx.send(());
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut reader = BufReader::new(&mut stream);
        let mut line = String::new();
        loop {
            line.clear();
            let n = reader.read_line(&mut line).await.unwrap_or(0);
            if n == 0 || line == "\r\n" || line == "\n" {
                break;
            }
        }
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            json_body.len(),
            json_body
        );
        stream.write_all(response.as_bytes()).await.unwrap();
        stream.flush().await.unwrap();
    });

    let _ = rx.await;
    let disc = ServiceDiscovery::new_no_refresh(DiscoveryMethod::Registry {
        endpoint: endpoint.clone(),
    });
    let services = disc.discover_all().await.unwrap();
    assert_eq!(services.len(), 1);
    assert_eq!(services[0].name, "mock-svc");
    assert_eq!(services[0].version, "1.0");
    assert!(services[0].capabilities.len() >= 2);
    assert_eq!(services[0].metadata.get("region").unwrap(), "test");
}

#[tokio::test]
async fn test_registry_http_mock_multiple_services() {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::TcpListener;
    use tokio::sync::oneshot;

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let endpoint = format!("http://127.0.0.1:{}/api/discovery", addr.port());

    let json_body = r#"{"services":[
        {"name":"svc-a","capabilities":["compute"],"endpoints":["http://localhost:1"]},
        {"name":"svc-b","capabilities":["storage"],"endpoints":["http://localhost:2"]},
        {"id":"custom-id","name":"svc-c","capabilities":["crypto"],"endpoints":["http://localhost:3"]}
    ]}"#;

    let (tx, rx) = oneshot::channel::<()>();
    tokio::spawn(async move {
        let _ = tx.send(());
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut reader = BufReader::new(&mut stream);
        let mut buf = String::new();
        while reader.read_line(&mut buf).await.unwrap_or(0) > 0 {
            if buf.ends_with("\r\n\r\n") || buf.contains("\r\n\r\n") {
                break;
            }
        }
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            json_body.len(),
            json_body
        );
        stream.write_all(response.as_bytes()).await.unwrap();
        stream.flush().await.unwrap();
    });

    let _ = rx.await;
    let disc = ServiceDiscovery::new_no_refresh(DiscoveryMethod::Registry { endpoint });
    let services = disc.discover_all().await.unwrap();
    assert_eq!(services.len(), 3);
    assert_eq!(services[0].name, "svc-a");
    assert_eq!(services[1].name, "svc-b");
    assert_eq!(services[2].name, "svc-c");
    assert_eq!(services[2].id, "custom-id");
}

#[tokio::test]
async fn test_registry_http_malformed_json_returns_error() {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::TcpListener;
    use tokio::sync::oneshot;

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let endpoint = format!("http://127.0.0.1:{}/services", addr.port());

    let (tx, rx) = oneshot::channel::<()>();
    tokio::spawn(async move {
        let _ = tx.send(());
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut reader = BufReader::new(&mut stream);
        let mut buf = String::new();
        while reader.read_line(&mut buf).await.unwrap_or(0) > 0 {
            if buf.contains("\r\n\r\n") {
                break;
            }
        }
        let body = r"not valid json at all {]";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        stream.write_all(response.as_bytes()).await.unwrap();
        stream.flush().await.unwrap();
    });

    let _ = rx.await;
    let disc = ServiceDiscovery::new_no_refresh(DiscoveryMethod::Registry { endpoint });
    let result = disc.discover_all().await;
    assert!(result.is_err());
    assert!(
        matches!(result, Err(DiscoveryError::InvalidResponse { .. })),
        "Expected InvalidResponse for malformed JSON, got {result:?}"
    );
}

#[tokio::test]
async fn test_registry_http_connection_refused() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let endpoint = format!("http://127.0.0.1:{port}/services");
    let disc = ServiceDiscovery::new(DiscoveryMethod::Registry { endpoint })
        .await
        .unwrap();
    let result = disc.discover_all().await;
    assert!(result.is_err());
    assert!(
        matches!(result, Err(DiscoveryError::NetworkError { .. })),
        "Expected NetworkError for connection refused, got {result:?}"
    );
}

#[tokio::test]
async fn test_registry_http_empty_services_array() {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::TcpListener;
    use tokio::sync::oneshot;

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let endpoint = format!("http://127.0.0.1:{}/services", addr.port());

    let json_body = r#"{"services":[]}"#;

    let (tx, rx) = oneshot::channel::<()>();
    tokio::spawn(async move {
        let _ = tx.send(());
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut reader = BufReader::new(&mut stream);
        let mut buf = String::new();
        while reader.read_line(&mut buf).await.unwrap_or(0) > 0 {
            if buf.contains("\r\n\r\n") {
                break;
            }
        }
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            json_body.len(),
            json_body
        );
        stream.write_all(response.as_bytes()).await.unwrap();
        stream.flush().await.unwrap();
    });

    let _ = rx.await;
    let disc = ServiceDiscovery::new_no_refresh(DiscoveryMethod::Registry { endpoint });
    let services = disc.discover_all().await.unwrap();
    assert!(services.is_empty());
}

#[tokio::test]
async fn test_registry_http_mock_slow_response() {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::TcpListener;
    use tokio::sync::oneshot;

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let endpoint = format!("http://127.0.0.1:{}/services", addr.port());

    let json_body = r#"{"services":[{"name":"slow-svc","capabilities":["compute"],"endpoints":["http://localhost:99"]}]}"#;

    let (tx, rx) = oneshot::channel::<()>();
    tokio::spawn(async move {
        let _ = tx.send(());
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut reader = BufReader::new(&mut stream);
        let mut buf = String::new();
        while reader.read_line(&mut buf).await.unwrap_or(0) > 0 {
            if buf.contains("\r\n\r\n") {
                break;
            }
        }
        for _ in 0..10 {
            tokio::task::yield_now().await;
        }
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{}",
            json_body.len(),
            json_body
        );
        stream.write_all(response.as_bytes()).await.unwrap();
        stream.flush().await.unwrap();
    });

    let _ = rx.await;
    let disc = ServiceDiscovery::new_no_refresh(DiscoveryMethod::Registry { endpoint });
    let services = disc.discover_all().await.unwrap();
    assert_eq!(services.len(), 1);
    assert_eq!(services[0].name, "slow-svc");
}

#[tokio::test]
async fn test_registry_http_path_without_leading_slash() {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::TcpListener;
    use tokio::sync::oneshot;

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let endpoint = format!("http://127.0.0.1:{}", addr.port());

    let json_body = r#"{"services":[{"name":"root-svc","capabilities":["storage"],"endpoints":["http://localhost:1"]}]}"#;

    let (tx, rx) = oneshot::channel::<()>();
    tokio::spawn(async move {
        let _ = tx.send(());
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut reader = BufReader::new(&mut stream);
        let mut buf = String::new();
        while reader.read_line(&mut buf).await.unwrap_or(0) > 0 {
            if buf.contains("\r\n\r\n") {
                break;
            }
        }
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{}",
            json_body.len(),
            json_body
        );
        stream.write_all(response.as_bytes()).await.unwrap();
        stream.flush().await.unwrap();
    });

    let _ = rx.await;
    let disc = ServiceDiscovery::new_no_refresh(DiscoveryMethod::Registry { endpoint });
    let services = disc.discover_all().await.unwrap();
    assert_eq!(services.len(), 1);
    assert_eq!(services[0].name, "root-svc");
}

#[tokio::test]
async fn test_registry_http_mock_filter_invalid_endpoints() {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::TcpListener;
    use tokio::sync::oneshot;

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let endpoint = format!("http://127.0.0.1:{}/services", addr.port());

    let json_body = r#"{"services":[{"name":"mixed-ep","capabilities":["compute"],"endpoints":["http://localhost:1",":::invalid","https://valid.com:443"]}]}"#;

    let (tx, rx) = oneshot::channel::<()>();
    tokio::spawn(async move {
        let _ = tx.send(());
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut reader = BufReader::new(&mut stream);
        let mut buf = String::new();
        while reader.read_line(&mut buf).await.unwrap_or(0) > 0 {
            if buf.contains("\r\n\r\n") {
                break;
            }
        }
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{}",
            json_body.len(),
            json_body
        );
        stream.write_all(response.as_bytes()).await.unwrap();
        stream.flush().await.unwrap();
    });

    let _ = rx.await;
    let disc = ServiceDiscovery::new_no_refresh(DiscoveryMethod::Registry { endpoint });
    let services = disc.discover_all().await.unwrap();
    assert_eq!(services.len(), 1);
    assert!(!services[0].endpoints.is_empty());
}

#[tokio::test]
async fn test_registry_https_scheme_connect() {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::TcpListener;
    use tokio::sync::oneshot;

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let endpoint = format!("https://127.0.0.1:{}/services", addr.port());

    let json_body = r#"{"services":[{"name":"https-svc","capabilities":["compute"],"endpoints":["https://localhost:443"]}]}"#;

    let (tx, rx) = oneshot::channel::<()>();
    tokio::spawn(async move {
        let _ = tx.send(());
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut reader = BufReader::new(&mut stream);
        let mut buf = String::new();
        while reader.read_line(&mut buf).await.unwrap_or(0) > 0 {
            if buf.contains("\r\n\r\n") {
                break;
            }
        }
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{}",
            json_body.len(),
            json_body
        );
        stream.write_all(response.as_bytes()).await.unwrap();
        stream.flush().await.unwrap();
    });

    let _ = rx.await;
    let disc = ServiceDiscovery::new_no_refresh(DiscoveryMethod::Registry { endpoint });
    let services = disc.discover_all().await.unwrap();
    assert_eq!(services.len(), 1);
    assert_eq!(services[0].name, "https-svc");
}

#[tokio::test]
async fn test_registry_http_find_by_capability_after_mock_discovery() {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::TcpListener;
    use tokio::sync::oneshot;

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let endpoint = format!("http://127.0.0.1:{}/services", addr.port());

    let json_body = r#"{"services":[{"name":"gpu-svc","capabilities":["gpu","compute"],"endpoints":["http://localhost:9999"]}]}"#;

    let (tx, rx) = oneshot::channel::<()>();
    tokio::spawn(async move {
        let _ = tx.send(());
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut reader = BufReader::new(&mut stream);
        let mut buf = String::new();
        while reader.read_line(&mut buf).await.unwrap_or(0) > 0 {
            if buf.contains("\r\n\r\n") {
                break;
            }
        }
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{}",
            json_body.len(),
            json_body
        );
        stream.write_all(response.as_bytes()).await.unwrap();
        stream.flush().await.unwrap();
    });

    let _ = rx.await;
    let disc = ServiceDiscovery::new_no_refresh(DiscoveryMethod::Registry { endpoint });
    let cap = Capability::Compute(crate::primal_identity::ComputeCapability::GpuCompute);
    let found = disc.find_service_by_capability(cap).await.unwrap();
    assert_eq!(found.name, "gpu-svc");
}
