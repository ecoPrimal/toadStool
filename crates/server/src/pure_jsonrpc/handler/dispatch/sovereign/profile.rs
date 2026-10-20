// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::Deserialize;

use super::DispatchHandler;

/// Instrumented pipeline with microsecond timing, boot state snapshots,
/// and register captures.
pub(crate) async fn sovereign_profile_ember(
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

    let mut clutch = handler.try_engage_clutch(bdf).await;

    if clutch.is_none() {
        let cache = handler.get_or_create_device(bdf).await.ok_or_else(|| {
            JsonRpcError::internal_error(format!(
                "device {bdf} not available — factory returned None"
            ))
        })?;
        drop(cache);
        clutch = handler.try_engage_clutch(bdf).await;
    }

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
    opts.dma_backend = dma_for_opts;
    opts.skip_cold_memory_training = true;

    let sm = opts.sm_version.unwrap_or_else(|| {
        let boot0 = bar0_ref.read_u32(0).unwrap_or(0);
        let chip_id = (boot0 >> 20) & 0x1FF;
        let synthetic = chip_id << 20;
        toadstool_cylinder::nv::identity::boot0_to_sm(synthetic).unwrap_or(70)
    });
    let chip = toadstool_cylinder::nv::identity::chip_name(sm);

    let bridge: std::sync::Arc<dyn toadstool_cylinder::nv::gsp_bridge::GspBridge> = {
        let nv = toadstool_cylinder::nv::nv_gsp_bridge::NvGspBridge::new(chip);
        if nv.has_gr_firmware() {
            std::sync::Arc::new(nv)
        } else {
            tracing::info!(
                chip,
                bdf,
                "sovereign.profile: using NoopGspBridge — firmware-less init active \
                 (non-firmware stages only; ACR/FECS require NvGspBridge or warm-handoff)"
            );
            std::sync::Arc::new(toadstool_cylinder::nv::gsp_bridge::NoopGspBridge::default())
        }
    };

    let profile = toadstool_cylinder::nv::generation::profile_for_sm(sm);
    let strategy =
        toadstool_cylinder::vfio::sovereign_strategy::strategy_for_profile(profile, bridge, sm);

    tracing::info!(bdf, "sovereign.profile: starting instrumented pipeline");

    let result = toadstool_cylinder::vfio::sovereign_profile::sovereign_profile(
        bar0_ref, bdf, &opts, &*strategy,
    );

    let anchor_held = {
        let store = handler.anchor_store.lock().await;
        store.contains_key(bdf)
    };

    tracing::info!(
        bdf,
        compute_ready = result.result.compute_ready,
        pipeline_us = result.result.total_ms * 1000,
        overhead_us = result.profiling_overhead_us,
        stages = result.stage_timings_us.len(),
        anchor_held,
        "sovereign.profile: complete"
    );

    if let Some(engaged) = clutch {
        engaged.disengage();
    }

    serde_json::to_value(&result)
        .map_err(|e| JsonRpcError::internal_error(format!("serialization failed: {e}")))
}
