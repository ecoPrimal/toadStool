// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::Deserialize;

use super::DispatchHandler;

/// Path 1 (clutch): if a VfioAnchor exists for this BDF, engage the clutch
/// to get fresh BAR0 + DMA from the anchor's fds. No stale state.
///
/// Path 2 (factory): if no anchor, create device via factory (which also
/// populates the anchor store for future calls), then try clutch again.
///
/// Path 3 (sysfs): last resort — sysfs BAR0 with DMA from cached device.
pub(crate) async fn sovereign_init_ember(
    handler: &DispatchHandler,
    params: Option<&serde_json::Value>,
) -> Result<serde_json::Value, crate::pure_jsonrpc::types::JsonRpcError> {
    use crate::pure_jsonrpc::types::{JsonRpcError, JsonRpcErrorExt};

    let bdf = params
        .and_then(|p| p.get("bdf"))
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| JsonRpcError::invalid_params("Missing 'bdf' string parameter"))?;

    handler.acquire_device_handle(bdf).await;
    crate::background::pcie_keepalive::activity_tracker().record();

    // Wake before anything touches BAR0. vfio-pci lets an idle GPU drop to
    // D3hot, where the device factory cannot probe it and every register
    // reads all-ones — which downstream logic then interprets as data. A
    // cold K80 in D3hot reported "devinit already complete" and had its
    // memory training declined on the strength of a register that was never
    // actually read.
    let wake = toadstool_cylinder::vfio::power::wake_to_d0(bdf);
    if !wake.awake {
        tracing::warn!(
            bdf,
            before = wake.before.as_str(),
            after = wake.after.as_str(),
            "device did not reach D0 — probes below may not be trustworthy"
        );
    }

    // Try clutch from existing anchor
    let mut clutch = handler.try_engage_clutch(bdf).await;

    // No anchor yet — run factory to create device + anchor, then retry
    let used_clutch = if clutch.is_none() {
        let cache = handler.get_or_create_device(bdf).await.ok_or_else(|| {
            JsonRpcError::internal_error(format!(
                "device {bdf} not available — factory returned None"
            ))
        })?;
        drop(cache);
        clutch = handler.try_engage_clutch(bdf).await;
        clutch.is_some()
    } else {
        true
    };

    // Resolve BAR0 + DMA from clutch or sysfs fallback
    let sysfs_bar;
    let (bar0_ref, dma_for_opts): (
        &toadstool_cylinder::vfio::device::MappedBar,
        Option<toadstool_cylinder::vfio::device::DmaBackend>,
    ) = if let Some(ref engaged) = clutch {
        (engaged.bar0(), Some(engaged.dma_backend_clone()))
    } else {
        tracing::warn!(bdf, "no clutch available — sysfs BAR0 fallback");
        let bar = toadstool_cylinder::vfio::device::MappedBar::from_sysfs_rw(bdf, 16 * 1024 * 1024)
            .map_err(|e| {
                JsonRpcError::internal_error(format!("sysfs BAR0 open failed for {bdf}: {e}"))
            })?;
        let dma = {
            let cache = handler.cached_devices.lock().await;
            cache.get(bdf).and_then(|d| d.dma_backend().cloned())
        };
        sysfs_bar = bar;
        (&sysfs_bar, dma)
    };

    let mut opts: toadstool_cylinder::vfio::sovereign_init::SovereignInitOptions = params
        .and_then(|p| {
            toadstool_cylinder::vfio::sovereign_init::SovereignInitOptions::deserialize(p).ok()
        })
        .unwrap_or_default();

    if let Some(path) = opts.vbios_rom_path.as_ref()
        && let Ok(rom) = std::fs::read(path)
    {
        opts.vbios_rom = Some(rom);
    }

    // Load engine_init_path (catalyst replay / golden state) if specified.
    // This was previously only wired in the legacy stateless handler.
    if let Some(ref path) = opts.engine_init_path {
        match std::fs::read_to_string(path) {
            Ok(json_str) => {
                match toadstool_cylinder::nv::gr_init::GrInitSequence::from_json(&json_str) {
                    Ok(seq) => {
                        let engine = seq.chip.engine_label();
                        tracing::info!(
                            bdf,
                            path,
                            writes = seq.len(),
                            engine = engine.as_str(),
                            "sovereign.init(ember): loaded engine init sequence"
                        );
                        opts.engine_init_sequences.push((engine, seq, None));
                    }
                    Err(e) => {
                        tracing::warn!(
                            bdf, path, err = %e,
                            "sovereign.init(ember): failed to parse engine init JSON"
                        );
                    }
                }
            }
            Err(e) => {
                tracing::warn!(
                    bdf, path, err = %e,
                    "sovereign.init(ember): failed to read engine init file"
                );
            }
        }
    }

    opts.dma_backend = dma_for_opts;

    let sm = opts.sm_version.unwrap_or_else(|| {
        let boot0 = bar0_ref.read_u32(0).unwrap_or(0);
        let chip_id = (boot0 >> 20) & 0x1FF;
        let synthetic = chip_id << 20;
        toadstool_cylinder::nv::identity::boot0_to_sm(synthetic).unwrap_or(70)
    });

    // Skip cold memory training only where it is genuinely futile.
    //
    // This used to be unconditional, on the reasoning that HBM2 is trained by
    // on-die sequencers during power-on and cannot be driven from software
    // afterwards. True for the Titan V, but it was applied to every GPU: a
    // GDDR5 K80 was told "HBM2 training requires power-on reset" and halted
    // before attempting a devinit that is ordinary register programming, and
    // which on Kepler *is* the power sequencer.
    let mem = toadstool_cylinder::nv::generation::profile_for_sm(sm).memory_type;
    opts.skip_cold_memory_training = mem.requires_power_on_reset_to_train();
    tracing::info!(
        bdf,
        sm,
        memory = %mem,
        skip_cold_memory_training = opts.skip_cold_memory_training,
        "cold memory training policy resolved from generation profile"
    );
    let chip = toadstool_cylinder::nv::identity::chip_name(sm);

    let bridge: std::sync::Arc<dyn toadstool_cylinder::nv::gsp_bridge::GspBridge> = {
        let nv = toadstool_cylinder::nv::nv_gsp_bridge::NvGspBridge::new(chip);
        if nv.has_gr_firmware() {
            tracing::info!(chip, bdf, "sovereign.init(ember): using NvGspBridge");
            std::sync::Arc::new(nv)
        } else {
            tracing::info!(
                chip,
                bdf,
                "sovereign.init(ember): using NoopGspBridge — firmware-less init active \
                 (non-firmware stages only; ACR/FECS require NvGspBridge or warm-handoff)"
            );
            std::sync::Arc::new(toadstool_cylinder::nv::gsp_bridge::NoopGspBridge::default())
        }
    };

    let profile = toadstool_cylinder::nv::generation::profile_for_sm(sm);
    let strategy =
        toadstool_cylinder::vfio::sovereign_strategy::strategy_for_profile(profile, bridge, sm);

    let pre_channel_stages = strategy.pre_channel_init(bar0_ref);
    if !pre_channel_stages.is_empty() {
        tracing::info!(
            bdf,
            stages = pre_channel_stages.len(),
            "sovereign.init(ember): pre_channel_init complete"
        );
        for s in &pre_channel_stages {
            tracing::info!(
                name = %s.name,
                status = ?s.status,
                detail = ?s.detail,
                ms = s.duration_ms,
                "pre_channel stage"
            );
        }
    }

    tracing::info!(bdf, halt_before = ?opts.halt_before, "sovereign.init(ember): starting pipeline");

    let result =
        toadstool_cylinder::vfio::sovereign_init::sovereign_init(bar0_ref, bdf, &opts, &*strategy);

    // Confirm anchor is live in store for fd persistence across restarts
    let anchor_held = {
        let store = handler.anchor_store.lock().await;
        store.contains_key(bdf)
    };

    tracing::info!(
        bdf,
        all_ok = result.all_ok,
        compute_ready = result.compute_ready,
        total_ms = result.total_ms,
        stages = result.stages.len(),
        warm_detected = result.warm_detected,
        clutch_path = used_clutch,
        anchor_held,
        "sovereign.init(ember): pipeline complete"
    );

    if let Some(engaged) = clutch {
        engaged.disengage();
    }

    serde_json::to_value(&result)
        .map_err(|e| JsonRpcError::internal_error(format!("serialization failed: {e}")))
}
