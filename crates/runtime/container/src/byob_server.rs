// SPDX-License-Identifier: AGPL-3.0-or-later
//! # Toadstool BYOB Server
//!
//! JSON-RPC server for handling BYOB deployment requests from the coordination service.
//! Uses Unix domain sockets (primary) with TCP fallback — isomorphic IPC per wateringHole standard.
//!
//! ## Evolution
//!
//! Formerly used axum/HTTP; excised in S380 G72 Tier 2 — HTTP belongs to songBird.
//! Now pure JSON-RPC 2.0, newline-delimited, over Unix or TCP sockets.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tracing::info;

use toadstool::{
    ToadStoolError, ToadStoolResult,
    byob::{ByobExecutorConfig, create_byob_executor},
};

use crate::byob_routes::ByobApi;
use toadstool_common::constants::network::LOCALHOST_IPV4;
use toadstool_config::ports::daemon_port;

use crate::ContainerRuntimeEngine;

/// Configuration for the BYOB server
#[derive(Debug, Clone, Default)]
pub struct ByobServerConfig {
    /// Server bind address (TCP fallback — overrides config file if set)
    pub bind_address: Option<String>,

    /// Server port (TCP fallback — overrides config file if set)
    pub port: Option<u16>,

    /// Path to TOML config file (optional; loads `bind_address`, port, `byob_config`)
    pub config_path: Option<String>,
}

/// Config file format (for TOML deserialization)
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ByobServerConfigFile {
    #[serde(default = "default_bind_address")]
    bind_address: String,
    #[serde(default = "default_port")]
    port: u16,
    #[serde(default)]
    byob_config: ByobExecutorConfig,
}

fn default_bind_address() -> String {
    LOCALHOST_IPV4.to_string()
}

fn default_port() -> u16 {
    daemon_port()
}

/// Run the BYOB server. Binds UDS (primary) or TCP (fallback) and serves until shutdown.
///
/// # Errors
///
/// Returns an error if configuration loading, runtime engine creation, or server startup fails.
pub async fn run_byob_server(config: ByobServerConfig) -> ToadStoolResult<()> {
    let runtime_engine = create_runtime_engine().await?;
    let byob_executor = create_byob_executor(runtime_engine);
    let api = Arc::new(ByobApi::new(byob_executor));

    // Transport injection: check TRANSPORT_ENDPOINT first (sourDough standard)
    if let Some(te) =
        toadstool_common::TransportEndpoint::from_env().map_err(ToadStoolError::configuration)?
    {
        match te {
            toadstool_common::TransportEndpoint::Uds { ref path } => {
                info!(
                    "Starting BYOB server on Unix socket {} (transport-injected)",
                    path.display()
                );
                return serve_unix(api, path.clone()).await;
            }
            toadstool_common::TransportEndpoint::Tcp { ref host, port } => {
                let addr: SocketAddr = format!("{host}:{port}").parse().map_err(|e| {
                    ToadStoolError::configuration(format!(
                        "Invalid TRANSPORT_ENDPOINT address: {e}"
                    ))
                })?;
                info!("Starting BYOB server on TCP {} (transport-injected)", addr);
                return serve_tcp(api, addr).await;
            }
            other => {
                info!("TRANSPORT_ENDPOINT={other} not directly applicable, falling back to config");
            }
        }
    }

    // Fallback: self-bind from config (Tier 5: debug/standalone only)
    let loaded = load_config_inner(config.config_path.as_deref())?;
    let bind_address = config
        .bind_address
        .as_deref()
        .unwrap_or(&loaded.bind_address);
    let port = config.port.unwrap_or(loaded.port);

    let addr: SocketAddr = format!("{bind_address}:{port}")
        .parse()
        .map_err(|e| ToadStoolError::configuration(format!("Invalid bind address: {e}")))?;
    info!("Starting BYOB server on TCP {} (self-bind fallback)", addr);

    serve_tcp(api, addr).await
}

/// Serve BYOB API over a Unix domain socket.
#[cfg(unix)]
async fn serve_unix<E: toadstool::byob::ByobExecutor + Send + Sync + 'static>(
    api: Arc<ByobApi<E>>,
    path: PathBuf,
) -> ToadStoolResult<()> {
    use tokio::net::UnixListener;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            ToadStoolError::runtime(format!("Failed to create socket directory: {e}"))
        })?;
    }
    let _ = std::fs::remove_file(&path);

    let listener = UnixListener::bind(&path)
        .map_err(|e| ToadStoolError::runtime(format!("Failed to bind Unix socket: {e}")))?;
    info!("BYOB JSON-RPC server listening: {}", path.display());

    loop {
        match listener.accept().await {
            Ok((stream, _addr)) => {
                let handler = Arc::clone(&api);
                tokio::spawn(async move {
                    if let Err(e) = handle_unix_connection(handler, stream).await {
                        tracing::error!("Unix connection error: {e}");
                    }
                });
            }
            Err(e) => tracing::error!("Unix accept error: {e}"),
        }
    }
}

/// UDS transport not available on this platform — use TCP fallback.
#[cfg(not(unix))]
async fn serve_unix<E: toadstool::byob::ByobExecutor + Send + Sync + 'static>(
    _api: Arc<ByobApi<E>>,
    path: PathBuf,
) -> ToadStoolResult<()> {
    Err(ToadStoolError::runtime(format!(
        "UDS transport not available on this platform (requested: {}). Use TCP fallback.",
        path.display()
    )))
}

/// Serve BYOB API over TCP.
async fn serve_tcp<E: toadstool::byob::ByobExecutor + Send + Sync + 'static>(
    api: Arc<ByobApi<E>>,
    addr: SocketAddr,
) -> ToadStoolResult<()> {
    let listener = TcpListener::bind(addr)
        .await
        .map_err(|e| ToadStoolError::runtime(format!("Failed to bind TCP: {e}")))?;
    info!("BYOB JSON-RPC server listening on TCP: {addr}");

    loop {
        match listener.accept().await {
            Ok((stream, _peer)) => {
                let handler = Arc::clone(&api);
                tokio::spawn(async move {
                    if let Err(e) = handle_tcp_connection(handler, stream).await {
                        tracing::error!("TCP connection error: {e}");
                    }
                });
            }
            Err(e) => tracing::error!("TCP accept error: {e}"),
        }
    }
}

#[cfg(unix)]
async fn handle_unix_connection<E: toadstool::byob::ByobExecutor + Send + Sync + 'static>(
    api: Arc<ByobApi<E>>,
    stream: tokio::net::UnixStream,
) -> ToadStoolResult<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) => break,
            Ok(_) => {
                let response = api.dispatch(&line).await;
                let response_json = serde_json::to_string(&response)
                    .map_err(|e| ToadStoolError::runtime(format!("Serialization error: {e}")))?;
                writer
                    .write_all(response_json.as_bytes())
                    .await
                    .map_err(|e| ToadStoolError::runtime(format!("Write error: {e}")))?;
                writer
                    .write_all(b"\n")
                    .await
                    .map_err(|e| ToadStoolError::runtime(format!("Write error: {e}")))?;
            }
            Err(e) => return Err(ToadStoolError::runtime(format!("Read error: {e}"))),
        }
    }
    Ok(())
}

async fn handle_tcp_connection<E: toadstool::byob::ByobExecutor + Send + Sync + 'static>(
    api: Arc<ByobApi<E>>,
    stream: TcpStream,
) -> ToadStoolResult<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) => break,
            Ok(_) => {
                let response = api.dispatch(&line).await;
                let response_json = serde_json::to_string(&response)
                    .map_err(|e| ToadStoolError::runtime(format!("Serialization error: {e}")))?;
                writer
                    .write_all(response_json.as_bytes())
                    .await
                    .map_err(|e| ToadStoolError::runtime(format!("Write error: {e}")))?;
                writer
                    .write_all(b"\n")
                    .await
                    .map_err(|e| ToadStoolError::runtime(format!("Write error: {e}")))?;
            }
            Err(e) => return Err(ToadStoolError::runtime(format!("Read error: {e}"))),
        }
    }
    Ok(())
}

struct LoadedConfig {
    bind_address: String,
    port: u16,
}

fn load_config_inner(config_path: Option<&str>) -> ToadStoolResult<LoadedConfig> {
    if let Some(path) = config_path {
        let content = std::fs::read_to_string(path)
            .map_err(|e| ToadStoolError::configuration(format!("Failed to read config: {e}")))?;

        let config: ByobServerConfigFile = toml::from_str(&content)
            .map_err(|e| ToadStoolError::configuration(format!("Failed to parse config: {e}")))?;

        Ok(LoadedConfig {
            bind_address: config.bind_address,
            port: config.port,
        })
    } else {
        Ok(LoadedConfig {
            bind_address: default_bind_address(),
            port: default_port(),
        })
    }
}

#[expect(
    clippy::unused_async,
    reason = "kept for API consistency with async runtime creation"
)]
async fn create_runtime_engine() -> ToadStoolResult<Arc<ContainerRuntimeEngine>> {
    info!("Initializing container runtime engine");
    let engine = ContainerRuntimeEngine::new()?;
    Ok(Arc::new(engine))
}

#[cfg(test)]
mod tests {
    use super::*;
    use toadstool_common::constants::network::LOCALHOST_IPV4;

    #[test]
    fn test_byob_server_config_default() {
        let config = ByobServerConfig::default();
        assert!(config.bind_address.is_none());
        assert!(config.port.is_none());
        assert!(config.config_path.is_none());
    }

    #[test]
    fn test_byob_server_config_clone() {
        let config = ByobServerConfig {
            bind_address: Some(LOCALHOST_IPV4.to_string()),
            port: Some(9999),
            config_path: Some("/tmp/test.toml".to_string()),
        };
        let cloned = config.clone();
        assert_eq!(cloned.bind_address, config.bind_address);
        assert_eq!(cloned.port, config.port);
        assert_eq!(cloned.config_path, config.config_path);
    }

    #[test]
    fn test_byob_server_config_debug() {
        let config = ByobServerConfig::default();
        let _ = format!("{config:?}");
    }

    /// A bad config path must surface as a config error.
    ///
    /// `run_byob_server` builds the container runtime engine before it looks at
    /// the config, and that connects to the Docker socket. On a host without
    /// Docker the call fails with "Socket not found" — which contains neither
    /// "config" nor "read", so this asserted against an error it could never
    /// reach and failed for a reason having nothing to do with config paths.
    #[tokio::test]
    async fn test_run_byob_server_invalid_config_path() {
        if ContainerRuntimeEngine::new().is_err() {
            eprintln!("skipping: no container runtime available on this host");
            return;
        }

        let config = ByobServerConfig {
            bind_address: None,
            port: None,
            config_path: Some("/nonexistent/path/that/does/not/exist/config.toml".to_string()),
        };
        let result = run_byob_server(config).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("config") || err.to_string().contains("read"));
    }
}
