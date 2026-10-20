// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::pure_jsonrpc::types::{JsonRpcError, JsonRpcErrorExt};
use serde::Deserialize;
use serde_json::Value;
use toadstool_cylinder::vfio::sovereign_init::SovereignInitOptions;
use tracing::{debug, info};

use super::DEFAULT_BAR0_SIZE;

/// `sovereign.init` — Run the full sovereign init pipeline on a GPU.
///
/// Opens BAR0 via sysfs or VFIO device, runs `cylinder::sovereign_init`
/// stages, returns per-stage results.
///
/// Params:
/// - `bdf` (required): PCI BDF address (e.g. `"0000:4b:00.0"`)
/// - `bar0_source` (optional): `"sysfs"` (default) or `"vfio"`. Use `"vfio"` for
///   GPUs bound to vfio-pci where sysfs resource0 is not accessible (e.g. K80).
///   VFIO path uses `open_no_busmaster` to avoid PFIFO DMA faults on Kepler.
/// - `halt_before` (optional): Stop before a stage (`"pmc_enable"`, `"cg_sweep"`,
///   `"pgob_ungate"`, `"memory_training"`, `"engine_ungate"`, `"falcon_boot"`,
///   `"gr_init"`, `"verify"`)
/// - `skip_gr_init` (optional, default false): Skip GR init stage
/// - `golden_state_path` (optional): Path to golden-state JSON for HBM2 replay
/// - `engine_init_path` (optional): Path to `GrInitSequence` JSON for silicon-deistic
///   engine replay (captured via `WarmStateCapture`, replayed without vendor driver)
/// - `vbios_rom_path` (optional): Path to raw VBIOS ROM dump
/// - `sm_version` (optional): SM version override (auto-detected if omitted)
/// - `fbpa_count` (optional): FBPA partition count override (auto-detected)
#[expect(
    clippy::collection_is_never_read,
    reason = "VfioDevice anchor must outlive MappedBar — keeps mmap valid"
)]
pub fn sovereign_init(params: Option<&Value>) -> Result<Value, JsonRpcError> {
    let bdf = params
        .and_then(|p| p.get("bdf"))
        .and_then(Value::as_str)
        .ok_or_else(|| JsonRpcError::invalid_params("Missing 'bdf' string parameter"))?;

    let bar0_source = params
        .and_then(|p| p.get("bar0_source"))
        .and_then(Value::as_str)
        .unwrap_or("auto");

    info!(bdf = %bdf, bar0_source, "sovereign.init: opening BAR0");

    // VfioDevice must outlive MappedBar (mmap borrows device fd)
    let mut _vfio_anchor = None;
    let mut dma_backend_for_opts = None;

    let _gate_bypass = toadstool_cylinder::vfio::ember_gate::EmberGateBypass::enter();

    #[expect(
        clippy::single_match_else,
        reason = "bar0_source may grow more variants"
    )]
    let bar0 = match bar0_source {
        "vfio" => {
            let dev = toadstool_cylinder::vfio::VfioDevice::open_no_busmaster(bdf)
                .map_err(|e| {
                    JsonRpcError::internal_error(format!(
                        "VFIO device open failed for {bdf}: {e}. Ensure vfio-pci is bound and IOMMU is configured."
                    ))
                })?;
            let bar = dev.map_bar(0).map_err(|e| {
                JsonRpcError::internal_error(format!("VFIO BAR0 map failed for {bdf}: {e}"))
            })?;
            dma_backend_for_opts = Some(dev.dma_backend());
            info!(bdf = %bdf, "sovereign.init: VFIO opened (DMA backend available)");
            _vfio_anchor = Some(dev);
            bar
        }
        _ => {
            let bar = toadstool_cylinder::vfio::device::MappedBar::from_sysfs_rw(bdf, DEFAULT_BAR0_SIZE)
                .map_err(|e| {
                    JsonRpcError::internal_error(format!(
                        "BAR0 open failed for {bdf}: {e}. Ensure vfio-pci is bound and resource0 is accessible."
                    ))
                })?;
            // Try to acquire a DMA backend via iommufd cdev even when using
            // sysfs BAR0 — this enables ACR falcon boot on warm GPUs where
            // the VFIO group may be busy but the cdev is accessible.
            match toadstool_cylinder::vfio::VfioDevice::open_no_busmaster(bdf) {
                Ok(dev) => {
                    dma_backend_for_opts = Some(dev.dma_backend());
                    info!(bdf = %bdf, "sovereign.init: DMA backend acquired via VFIO cdev (sysfs BAR0)");
                    _vfio_anchor = Some(dev);
                }
                Err(e) => {
                    info!(bdf = %bdf, err = %e, "sovereign.init: no DMA backend available (sysfs-only)");
                }
            }
            bar
        }
    };

    let mut opts: SovereignInitOptions = if let Some(p) = params {
        SovereignInitOptions::deserialize(p).unwrap_or_default()
    } else {
        SovereignInitOptions::default()
    };

    if let Some(path) = opts.golden_state_path.as_ref() {
        match std::fs::read_to_string(path) {
            Ok(json_str) => {
                if let Ok(pairs) = serde_json::from_str::<Vec<(usize, u32)>>(&json_str) {
                    opts.golden_state = Some(pairs);
                }
            }
            Err(e) => {
                info!(path = %path, error = %e, "golden_state_path read failed, continuing without");
            }
        }
    }

    if let Some(path) = opts.engine_init_path.as_ref() {
        match std::fs::read_to_string(path) {
            Ok(json_str) => {
                match toadstool_cylinder::nv::gr_init::GrInitSequence::from_json(&json_str) {
                    Ok(seq) => {
                        let engine = seq.chip.engine_label();
                        info!(path = %path, engine = %engine, writes = seq.len(),
                              "loaded golden-state GrInitSequence for engine replay");
                        opts.engine_init_sequences.push((engine, seq, None));
                    }
                    Err(e) => {
                        info!(path = %path, error = %e, "engine_init_path JSON parse failed, continuing without");
                    }
                }
            }
            Err(e) => {
                info!(path = %path, error = %e, "engine_init_path read failed, continuing without");
            }
        }
    }

    if let Some(path) = opts.vbios_rom_path.as_ref() {
        match std::fs::read(path) {
            Ok(rom) => {
                opts.vbios_rom = Some(rom);
            }
            Err(e) => {
                info!(path = %path, error = %e, "vbios_rom_path read failed, continuing without");
            }
        }
    }

    opts.dma_backend = dma_backend_for_opts;

    let sm = opts.sm_version.unwrap_or_else(|| {
        let boot0 = bar0.read_u32(0).unwrap_or(0);
        let chip_id = (boot0 >> 20) & 0x1FF;
        let synthetic = chip_id << 20;
        toadstool_cylinder::nv::identity::boot0_to_sm(synthetic).unwrap_or(70)
    });
    let chip = toadstool_cylinder::nv::identity::chip_name(sm);

    let bridge: std::sync::Arc<dyn toadstool_cylinder::nv::gsp_bridge::GspBridge> = {
        let nv = toadstool_cylinder::nv::nv_gsp_bridge::NvGspBridge::new(chip);
        if nv.has_gr_firmware() {
            info!(chip, "sovereign.init: using NvGspBridge (firmware found)");
            std::sync::Arc::new(nv)
        } else {
            info!(
                chip,
                "sovereign.init: using NoopGspBridge — firmware-less init active \
                 (non-firmware stages only; ACR/FECS require NvGspBridge or warm-handoff)"
            );
            std::sync::Arc::new(toadstool_cylinder::nv::gsp_bridge::NoopGspBridge::default())
        }
    };

    let profile = toadstool_cylinder::nv::generation::profile_for_sm(sm);
    let strategy =
        toadstool_cylinder::vfio::sovereign_strategy::strategy_for_profile(profile, bridge, sm);

    info!(bdf = %bdf, halt_before = ?opts.halt_before, "sovereign.init: starting pipeline");

    let result =
        toadstool_cylinder::vfio::sovereign_init::sovereign_init(&bar0, bdf, &opts, &*strategy);

    info!(
        bdf = %bdf,
        all_ok = result.all_ok,
        compute_ready = result.compute_ready,
        total_ms = result.total_ms,
        stages = result.stages.len(),
        warm_detected = result.warm_detected,
        "sovereign.init: pipeline complete"
    );

    serde_json::to_value(&result)
        .map_err(|e| JsonRpcError::internal_error(format!("serialization failed: {e}")))
}

/// `sovereign.devinit` — Run PMU FALCON devinit (VBIOS-based memory training).
///
/// Reads VBIOS from PROM, parses BIT table, uploads PMU firmware, executes
/// devinit. Falls back to host-side VBIOS interpreter if FALCON fails.
/// This is the K80/Kepler cold-boot path to bring up GDDR5 VRAM.
///
/// Params:
/// - `bdf` (required): PCI BDF address
/// - `bar0_source` (optional): `"sysfs"` (default) or `"vfio"`. Use `"vfio"` for
///   GPUs bound to vfio-pci where sysfs resource0 is not accessible.
/// - `vbios_path` (optional): Path to pre-dumped VBIOS ROM file
#[expect(
    clippy::collection_is_never_read,
    reason = "VfioDevice anchor must outlive MappedBar — keeps mmap valid"
)]
pub fn sovereign_devinit(params: Option<&Value>) -> Result<Value, JsonRpcError> {
    use toadstool_cylinder::vfio::channel::devinit;

    let bdf = params
        .and_then(|p| p.get("bdf"))
        .and_then(Value::as_str)
        .ok_or_else(|| JsonRpcError::invalid_params("Missing 'bdf' string parameter"))?;

    let bar0_source = params
        .and_then(|p| p.get("bar0_source"))
        .and_then(Value::as_str)
        .unwrap_or("sysfs");

    info!(bdf = %bdf, bar0_source, "sovereign.devinit: opening BAR0");

    let mut _vfio_anchor = None;

    let bar0 = if bar0_source == "vfio" {
        let dev = toadstool_cylinder::vfio::VfioDevice::open_no_busmaster(bdf).map_err(|e| {
            JsonRpcError::internal_error(format!("VFIO device open failed for {bdf}: {e}"))
        })?;
        let bar = dev.map_bar(0).map_err(|e| {
            JsonRpcError::internal_error(format!("VFIO BAR0 map failed for {bdf}: {e}"))
        })?;
        _vfio_anchor = Some(dev);
        bar
    } else {
        toadstool_cylinder::vfio::device::MappedBar::from_sysfs_rw(bdf, DEFAULT_BAR0_SIZE)
            .map_err(|e| JsonRpcError::internal_error(format!("BAR0 open failed for {bdf}: {e}")))?
    };

    let diag = devinit::FalconDiagnostic::probe(&bar0, Some(bdf));

    let diag_json = serde_json::json!({
        "needs_post": diag.status.needs_post,
        "devinit_reg": format!("{:#010x}", diag.status.devinit_reg),
        "pmu_id": format!("{:#010x}", diag.status.pmu_id),
        "pmu_hwcfg": format!("{:#010x}", diag.status.pmu_hwcfg),
        "pmu_ctrl": format!("{:#010x}", diag.status.pmu_ctrl),
        "pmu_mbox0": format!("{:#010x}", diag.status.pmu_mbox0),
        "prom_accessible": diag.prom_accessible,
        "prom_signature": format!("{:#010x}", diag.prom_signature),
        "secure_boot": diag.secure_boot,
        "falcon_halted": diag.falcon_halted,
        "falcon_pc": format!("{:#010x}", diag.falcon_pc),
        "imem_kb": diag.imem_size_kb,
        "dmem_kb": diag.dmem_size_kb,
        "vbios_sources": diag.vbios_sources.iter().map(|(name, ok, detail)| {
            serde_json::json!({"name": name, "available": ok, "detail": detail})
        }).collect::<Vec<_>>(),
    });

    if !diag.status.needs_post {
        return Ok(serde_json::json!({
            "bdf": bdf,
            "action": "none",
            "reason": "devinit already complete",
            "diagnostic": diag_json,
        }));
    }

    info!(bdf = %bdf, prom = diag.prom_accessible, secure = diag.secure_boot,
          "sovereign.devinit: GPU needs POST, attempting devinit");

    match devinit::execute_devinit_with_diagnostics(&bar0, Some(bdf)) {
        Ok(true) => {
            info!(bdf = %bdf, "sovereign.devinit: VRAM alive after devinit");
            Ok(serde_json::json!({
                "bdf": bdf,
                "action": "devinit_executed",
                "vram_alive": true,
                "diagnostic": diag_json,
            }))
        }
        Ok(false) => Ok(serde_json::json!({
            "bdf": bdf,
            "action": "devinit_not_needed",
            "vram_alive": false,
            "diagnostic": diag_json,
        })),
        Err(e) => {
            info!(bdf = %bdf, error = %e, "sovereign.devinit: devinit failed");
            Ok(serde_json::json!({
                "bdf": bdf,
                "action": "devinit_failed",
                "error": e.to_string(),
                "diagnostic": diag_json,
            }))
        }
    }
}

/// `sovereign.classify_tier` — generation-aware sovereignty tier classification.
///
/// Uses `GenerationProfile` offsets instead of hardcoded Volta values.
/// Auto-detects SM version from BOOT0, falls back to SM 70 (Volta).
///
/// Params:
/// - `bdf` (required): PCI BDF address
/// - `sm_version` (optional): override SM version for profile lookup
pub fn sovereign_classify_tier(params: Option<&Value>) -> Result<Value, JsonRpcError> {
    let bdf = params
        .and_then(|p| p.get("bdf"))
        .and_then(Value::as_str)
        .ok_or_else(|| JsonRpcError::invalid_params("Missing 'bdf'"))?;

    let bar0 = toadstool_cylinder::vfio::device::MappedBar::from_sysfs_rw(bdf, DEFAULT_BAR0_SIZE)
        .map_err(|e| {
        JsonRpcError::internal_error(format!("BAR0 open failed for {bdf}: {e}"))
    })?;

    let sm_override = params
        .and_then(|p| p.get("sm_version"))
        .and_then(Value::as_u64)
        .map(|v| v as u32);

    let sm = sm_override.unwrap_or_else(|| {
        let boot0 = bar0.read_u32(0x0000_0000).unwrap_or(0);
        let chip_id = (boot0 >> 20) & 0x1FF;
        let synthetic = chip_id << 20;
        toadstool_cylinder::nv::identity::boot0_to_sm(synthetic).unwrap_or(70)
    });

    let profile = toadstool_cylinder::nv::generation::profile_for_sm(sm);

    let evidence =
        toadstool_cylinder::vfio::sovereign_tiers::classify_tier_for_profile(&bar0, profile);

    debug!(bdf = %bdf, sm = sm, tier = ?evidence.tier, generation = profile.name,
           "sovereign.classify_tier");

    Ok(serde_json::json!({
        "bdf": bdf,
        "sm_version": sm,
        "generation": profile.name,
        "ce_class": format!("{:#06X}", profile.ce_class),
        "tier": evidence.tier,
        "tier_level": evidence.tier.level(),
        "tier_description": evidence.tier.description(),
        "evidence": {
            "pmc_enable": format!("{:#010x}", evidence.pmc_enable),
            "pmc_popcount": evidence.pmc_popcount,
            "pramin_accessible": evidence.pramin_accessible,
            "fecs_pc": evidence.fecs_pc.map(|v| format!("{:#010x}", v)),
            "gpc_enables": evidence.gpc_enables.map(|v| format!("{:#010x}", v)),
            "ce_status": evidence.ce_status.map(|v| format!("{:#010x}", v)),
            "gr_status": evidence.gr_status.map(|v| format!("{:#010x}", v)),
            "pbdma_intr": evidence.pbdma_intr.map(|v| format!("{:#010x}", v)),
            "ce_runlist": evidence.ce_runlist,
            "tpc_status": evidence.tpc_status.map(|v| format!("{:#010x}", v)),
            "tpc_alive": evidence.tpc_alive,
        },
        "profile_offsets": {
            "fecs_pc": format!("{:#010x}", profile.fecs_pc_offset),
            "gpc_broadcast": format!("{:#010x}", profile.gpc_broadcast_offset),
            "ce0_base": format!("{:#010x}", profile.ce0_base_offset),
            "pgraph_status": format!("{:#010x}", profile.pgraph_status_offset),
        }
    }))
}

/// `sovereign.experiment` — execute a staged warm compute experiment.
///
/// Runs a single experiment stage (1-6) on a VFIO-bound GPU, capturing
/// before/after register snapshots and the writes performed. Designed
/// for interactive exploration of Tier 1 → Tier 2 transitions.
///
/// Stages 4 and 6 now use `NvGspBridge` with real GV100 firmware
/// (`sw_nonctx.bin`) instead of `NoopGspBridge`. Stage 6 is the full
/// 5-phase ungating sequence including PGRAPH reset (Exp 217).
///
/// Params:
/// - `bdf` (required): PCI BDF address (e.g. `"0000:02:00.0"`)
/// - `stage` (required): Stage number 1-6
///
/// Returns: `ExperimentResult` with before/after snapshots, diff, writes, and notes.
pub fn sovereign_experiment(params: Option<&Value>) -> Result<Value, JsonRpcError> {
    let bdf = params
        .and_then(|p| p.get("bdf"))
        .and_then(Value::as_str)
        .ok_or_else(|| JsonRpcError::invalid_params("Missing 'bdf'"))?;

    let stage = params
        .and_then(|p| p.get("stage"))
        .and_then(Value::as_u64)
        .ok_or_else(|| JsonRpcError::invalid_params("Missing 'stage' (1-6)"))?
        as u32;

    let chip = params.and_then(|p| p.get("chip")).and_then(Value::as_str);

    info!(bdf, stage, chip = ?chip, "sovereign.experiment: starting stage");

    let bar0 = toadstool_cylinder::vfio::device::MappedBar::from_sysfs_rw(bdf, DEFAULT_BAR0_SIZE)
        .map_err(|e| {
        JsonRpcError::internal_error(format!("BAR0 open failed for {bdf}: {e}"))
    })?;

    let result =
        toadstool_cylinder::vfio::sovereign_stages::run_experiment_stage(&bar0, stage, chip)
            .map_err(JsonRpcError::invalid_params)?;

    info!(
        bdf,
        stage,
        diff_count = result.diff.len(),
        writes = result.writes.len(),
        "sovereign.experiment: stage complete"
    );

    // Also run tier classification after the experiment
    let tier = toadstool_cylinder::vfio::sovereign_tiers::classify_tier(&bar0);

    serde_json::to_value(serde_json::json!({
        "bdf": bdf,
        "stage": result.stage,
        "stage_name": result.stage_name,
        "diff": result.diff,
        "writes": result.writes,
        "notes": result.notes,
        "before": result.before,
        "after": result.after,
        "tier_after": {
            "tier": tier.tier,
            "tier_level": tier.tier.level(),
            "description": tier.tier.description(),
            "gpc_enables": tier.gpc_enables.map(|v| format!("{v:#010x}")),
            "ce_status": tier.ce_status.map(|v| format!("{v:#010x}")),
            "tpc_status": tier.tpc_status.map(|v| format!("{v:#010x}")),
            "tpc_alive": tier.tpc_alive,
        },
    }))
    .map_err(|e| JsonRpcError::internal_error(format!("serialization failed: {e}")))
}
