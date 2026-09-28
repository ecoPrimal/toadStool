// SPDX-License-Identifier: AGPL-3.0-or-later

//! Sovereign driver rotation CLI.
//!
//! Surfaces the `sovereign.*` JSON-RPC methods, which previously had no CLI
//! and could only be reached by hand-writing socket clients. Every experiment
//! therefore shipped its own throwaway harness, and those harnesses were the
//! least reliable part of the stack: one silently framed requests without the
//! trailing newline the server's `read_line` requires and deadlocked, another
//! reported "halted safely" while the desktop session was already gone.
//!
//! # Why this talks to the daemon instead of calling the library
//!
//! `toadstool_cylinder` exposes the handoff directly, and calling it in-process
//! would be simpler. But the daemon holds the PCIe bridge keepalive that pins
//! upstream hierarchies during rotation. Bypassing it risks the Exp 229 failure
//! mode, where a config read to an unresponsive bridge enters kernel CRS retry
//! holding the global `pci_lock` and deadlocks every PCI operation on the box,
//! including the display GPU. This matters most for the K80s, which sit behind
//! PLX switches.

use crate::Result;

use super::definitions::SovereignCommand;

#[cfg(unix)]
use std::io::{BufRead, BufReader, Write};
#[cfg(unix)]
use std::os::unix::net::UnixStream;
#[cfg(unix)]
use std::time::Duration;

/// Every connection must announce itself per the riboCipher transport
/// standard; the server rejects unsignalled connections.
#[cfg(unix)]
const RIBOCIPHER_PREFIX: [u8; 2] = [0xEC, 0x01];

/// Longer than the server's own 420s warm-handoff timeout, so the server's
/// error surfaces rather than the client giving up first.
#[cfg(unix)]
const RPC_TIMEOUT: Duration = Duration::from_secs(480);

#[cfg(unix)]
fn socket_path() -> String {
    if let Ok(explicit) = std::env::var("TOADSTOOL_COMPUTE_SOCK") {
        return explicit;
    }
    let runtime_dir =
        std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/run/user/1000".to_string());
    format!("{runtime_dir}/biomeos/compute.sock")
}

/// Issue a single JSON-RPC call over the compute socket.
#[cfg(unix)]
fn rpc(method: &str, params: serde_json::Value) -> Result<serde_json::Value> {
    let path = socket_path();
    let stream = UnixStream::connect(&path).map_err(|e| {
        crate::CliError::Other(format!(
            "cannot reach toadStool at {path}: {e}. Is the server running?"
        ))
    })?;
    stream.set_read_timeout(Some(RPC_TIMEOUT)).ok();
    stream.set_write_timeout(Some(RPC_TIMEOUT)).ok();

    let req = serde_json::json!({
        "jsonrpc": "2.0",
        "method": method,
        "params": params,
        "id": 1,
    });

    let mut w = &stream;
    w.write_all(&RIBOCIPHER_PREFIX)
        .and_then(|()| w.write_all(serde_json::to_string(&req)?.as_bytes()))
        // The server frames with read_line. Without this newline it blocks
        // waiting to complete the line while we block waiting for a reply.
        .and_then(|()| w.write_all(b"\n"))
        .and_then(|()| w.flush())
        .map_err(|e| crate::CliError::Other(format!("write to {path} failed: {e}")))?;

    let mut line = String::new();
    BufReader::new(&stream)
        .read_line(&mut line)
        .map_err(|e| crate::CliError::Other(format!("read from {path} failed: {e}")))?;

    if line.trim().is_empty() {
        return Err(crate::CliError::Other(
            "server closed the connection without responding".into(),
        ));
    }

    let resp: serde_json::Value = serde_json::from_str(&line)
        .map_err(|e| crate::CliError::Other(format!("malformed response: {e}: {line}")))?;

    if let Some(err) = resp.get("error") {
        return Err(crate::CliError::Other(format!("server error: {err}")));
    }
    Ok(resp
        .get("result")
        .cloned()
        .unwrap_or(serde_json::Value::Null))
}

#[cfg(unix)]
fn print_steps(result: &serde_json::Value) {
    let Some(steps) = result.get("steps").and_then(|s| s.as_array()) else {
        return;
    };
    for step in steps {
        let ok = step.get("ok").and_then(serde_json::Value::as_bool) == Some(true);
        let name = step.get("name").and_then(|v| v.as_str()).unwrap_or("?");
        let ms = step
            .get("duration_ms")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        let detail = step.get("detail").and_then(|v| v.as_str()).unwrap_or("");
        println!(
            "  [{}] {name:<18} {ms:>6}ms  {detail}",
            if ok { "ok  " } else { "FAIL" }
        );
    }
}

/// Report the tier without letting an unmeasured device look like a verdict.
#[cfg(unix)]
fn print_tier(result: &serde_json::Value) {
    let Some(tier) = result.get("tier") else {
        return;
    };
    if tier.is_null() {
        println!("\n  tier: none (classification did not run)");
        return;
    }

    let b = |k: &str| tier.get(k).and_then(serde_json::Value::as_bool);
    let u = |k: &str| tier.get(k).and_then(serde_json::Value::as_u64);

    println!(
        "\n  tier: {}",
        tier.get("tier").and_then(|v| v.as_str()).unwrap_or("?")
    );

    if b("bus_readable") == Some(false) {
        println!("  WARNING: BAR0 returned all-ones — the device did not answer.");
        println!("           This tier is not evidence; no measurement took place.");
        return;
    }

    if let (Some(pmc), Some(pc)) = (u("pmc_enable"), u("pmc_popcount")) {
        println!("  pmc_enable: {pmc:#010x} ({pc} engines)");
    }
    println!(
        "  dispatch path: fecs={} tpc={} pramin={}",
        b("fecs_alive").map_or("?", |v| if v { "alive" } else { "DEAD" }),
        b("tpc_alive").map_or("?", |v| if v { "alive" } else { "dead" }),
        b("pramin_accessible").map_or("?", |v| if v { "yes" } else { "no" }),
    );
    if b("fecs_alive") == Some(false)
        && let Some(pc) = u("fecs_pc")
    {
        println!("  blocker: FECS not live (fecs_pc={pc:#010x}) — no shader dispatch");
    }
}

#[cfg(not(unix))]
pub async fn execute_sovereign_command(_cmd: SovereignCommand) -> Result<()> {
    Err(crate::CliError::Other(
        "Sovereign GPU commands require Unix domain sockets — not available on Windows. \
         Use TCP-based toadstool-server RPC instead."
            .into(),
    ))
}

#[cfg(unix)]
pub async fn execute_sovereign_command(cmd: SovereignCommand) -> Result<()> {
    match cmd {
        SovereignCommand::Handoff {
            bdf,
            strategy,
            settle_secs,
            skip_preflight,
            format,
        } => {
            let mut params = serde_json::json!({ "bdf": bdf, "strategy": strategy });
            if let Some(s) = settle_secs {
                params["settle_secs"] = serde_json::json!(s);
            }
            if skip_preflight {
                params["skip_preflight"] = serde_json::json!(true);
            }

            if format != "json" {
                println!("Sovereign handoff: {bdf} via '{strategy}'");
                if skip_preflight {
                    println!("  WARNING: preflight skipped — session safety checks are OFF");
                }
                println!("  (this can take several minutes; the server drives the rotation)");
                println!();
            }

            let result = rpc("sovereign.warm_handoff", params)?;

            if format == "json" {
                println!("{}", serde_json::to_string_pretty(&result)?);
                return Ok(());
            }

            let success = result
                .get("success")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            print_steps(&result);
            print_tier(&result);
            println!(
                "\n  result: {}{}",
                if success { "SUCCESS" } else { "FAILED" },
                result
                    .get("halted_at")
                    .and_then(|v| v.as_str())
                    .map(|h| format!(" (halted at: {h})"))
                    .unwrap_or_default()
            );

            if !success {
                return Err(crate::CliError::Other("handoff did not succeed".into()));
            }
        }

        SovereignCommand::Init {
            bdf,
            sm_version,
            halt_before,
            skip_cold_memory_training,
            skip_gr_init,
            format,
        } => {
            let mut params = serde_json::json!({ "bdf": bdf });
            if let Some(sm) = sm_version {
                params["sm_version"] = serde_json::json!(sm);
            }
            if let Some(h) = halt_before {
                params["halt_before"] = serde_json::json!(h);
            }
            if skip_cold_memory_training {
                params["skip_cold_memory_training"] = serde_json::json!(true);
            }
            if skip_gr_init {
                params["skip_gr_init"] = serde_json::json!(true);
            }

            if format != "text" {
                let result = rpc("sovereign.init", params)?;
                println!("{}", serde_json::to_string_pretty(&result)?);
                return Ok(());
            }

            // "No vendor driver" would be a lie on any path that seeds with
            // nouveau: nouveau is external C, so it is vendor code by the
            // standard this project actually holds. Say what is true of the
            // path being run, and never imply that bring-up means dispatch.
            println!("Sovereign init: {bdf} (VFIO direct, no seeder module)");
            if let Some(sm) = sm_version {
                println!("  sm_version: {sm}");
            }
            println!();

            let result = rpc("sovereign.init", params)?;

            // Stage names vary by generation, so render whatever came back
            // rather than assuming a fixed pipeline shape.
            if let Some(stages) = result.get("stages").and_then(|s| s.as_array()) {
                for st in stages {
                    let name = st.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                    let status = st
                        .get("status")
                        .and_then(|v| v.as_str())
                        .unwrap_or_else(|| {
                            match st.get("ok").and_then(serde_json::Value::as_bool) {
                                Some(true) => "ok",
                                Some(false) => "FAIL",
                                None => "?",
                            }
                        });
                    let detail = st.get("detail").and_then(|v| v.as_str()).unwrap_or("");
                    println!("  [{status:<7}] {name:<22} {detail}");
                }
            }

            for key in [
                "compute_ready",
                "falcon_booted",
                "gr_initialized",
                "halted_at",
            ] {
                if let Some(v) = result.get(key)
                    && !v.is_null()
                {
                    println!("  {key}: {v}");
                }
            }
            print_tier(&result);
        }

        SovereignCommand::Status { format } => {
            let result = rpc("sovereign.warm_status", serde_json::json!({}))?;
            if format == "json" {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                println!("Sovereign anchors:");
                println!(
                    "  anchor_count    : {}",
                    result
                        .get("anchor_count")
                        .unwrap_or(&serde_json::Value::Null)
                );
                println!(
                    "  fd_store_capable: {}",
                    result
                        .get("fd_store_capable")
                        .unwrap_or(&serde_json::Value::Null)
                );
                if let Some(devs) = result.get("devices").and_then(|d| d.as_object()) {
                    if devs.is_empty() {
                        println!("  devices         : none anchored");
                    } else {
                        for (bdf, state) in devs {
                            println!("  {bdf}: {state}");
                        }
                    }
                }
            }
        }

        SovereignCommand::Strategies => {
            println!("Warm handoff strategies:");
            for (name, note) in [
                ("nouveau_titanv", "Titan V (GV100) via patched nouveau"),
                (
                    "nouveau_k80",
                    "Tesla K80 (GK210) via nouveau — unsigned falcons",
                ),
                ("nvidia_titanv", "Titan V via the loaded nvidia driver"),
                (
                    "nvidia_patched_titanv",
                    "Titan V via nvidia with teardown NOPs",
                ),
                (
                    "nvidia_catalyst_titanv",
                    "Titan V catalyst boot (SBR, full RM init)",
                ),
                (
                    "nvidia_catalyst_minimal_nop_titanv",
                    "catalyst with a reduced NOP set",
                ),
            ] {
                println!("  {name:<36} {note}");
            }
        }
    }
    Ok(())
}
