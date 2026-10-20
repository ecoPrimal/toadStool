// SPDX-License-Identifier: AGPL-3.0-or-later

use super::DispatchHandler;

const CATALYST_WATCHDOG_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(450);
const WARM_HANDOFF_RPC_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(420);
const SBR_RESET_SETTLE: std::time::Duration = std::time::Duration::from_millis(500);

/// Orchestrates the full warm handoff: module patching → insmod →
/// seeder bind → settle → warm swap to vfio-pci → tier classification
/// → rmmod. The operator never touches the kernel.
pub(crate) async fn sovereign_warm_handoff(
    handler: &DispatchHandler,
    params: Option<&serde_json::Value>,
) -> Result<serde_json::Value, crate::pure_jsonrpc::types::JsonRpcError> {
    use crate::pure_jsonrpc::types::{JsonRpcError, JsonRpcErrorExt};
    use toadstool_cylinder::vfio::sovereign_handoff::HandoffConfig;

    let bdf = params
        .and_then(|p| p.get("bdf"))
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| JsonRpcError::invalid_params("Missing 'bdf' string parameter"))?;

    let strategy = params
        .and_then(|p| p.get("strategy"))
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| JsonRpcError::invalid_params("Missing 'strategy' string parameter"))?;

    let mut config = HandoffConfig::from_strategy(strategy, bdf).ok_or_else(|| {
        JsonRpcError::invalid_params(format!(
            "Unknown warm handoff strategy: '{strategy}'. \
             Valid: nouveau_titanv, nouveau_k80, nvidia_titanv, nvidia_patched_titanv, \
             nvidia_catalyst_titanv, nvidia_catalyst_minimal_nop_titanv"
        ))
    })?;

    if let Some(secs) = params
        .and_then(|p| p.get("settle_secs"))
        .and_then(serde_json::Value::as_u64)
    {
        config.settle = std::time::Duration::from_secs(secs);
    }

    if let Some(json) = params
        .and_then(|p| p.get("patch_set_json"))
        .and_then(serde_json::Value::as_str)
    {
        config.patch_set_override = Some(json.to_string());
    }

    if let Some(skip) = params
        .and_then(|p| p.get("skip_preflight"))
        .and_then(serde_json::Value::as_bool)
    {
        config.skip_preflight = skip;
    }

    if let Some(name) = params
        .and_then(|p| p.get("module_name"))
        .and_then(serde_json::Value::as_str)
    {
        config.module_name = name.to_string();
        config.seeder_driver = name.to_string();
    }

    tracing::info!(
        bdf,
        strategy,
        settle_secs = config.settle.as_secs(),
        skip_preflight = config.skip_preflight,
        has_patch_override = config.patch_set_override.is_some(),
        "sovereign.warm_handoff: starting driver rotation pipeline"
    );

    // Read PMC_ENABLE before releasing anchor to detect cold GPU.
    //
    // Wake first. A device in D3hot answers every BAR0 read with all-ones,
    // and 0xFFFF_FFFF has a popcount of 32 — so a sleeping GPU used to be
    // declared warm here, which suppressed SBR and, on the catalyst path,
    // fed the PRI-fault check below with garbage that triggered a spurious
    // bus reset.
    let gpu_warm = {
        use toadstool_cylinder::nv::register_read::RegisterRead;
        use toadstool_cylinder::vfio::device::MappedBar;
        use toadstool_cylinder::vfio::power;

        power::wake_to_d0(bdf);

        if let Ok(bar) = MappedBar::from_sysfs_rw(bdf, 16 * 1024 * 1024) {
            let read = RegisterRead::from_result(bar.read_u32(0x200));
            if let Some(pmc) = read.valid() {
                let popcount = pmc.count_ones();
                tracing::info!(
                    bdf,
                    pmc = format_args!("0x{pmc:08x}"),
                    popcount,
                    "pre-release PMC_ENABLE"
                );
                popcount >= 10
            } else {
                // Unreadable is not warm. Assuming warm here would skip
                // the reset that a genuinely wedged GPU needs.
                tracing::warn!(
                    bdf,
                    read = read.describe(),
                    power_state = power::power_state(bdf).as_str(),
                    "pre-release PMC_ENABLE unreadable — treating GPU as not-warm"
                );
                false
            }
        } else {
            true
        }
    };

    // Catalyst strategies ALWAYS need SBR: RM's rm_init_adapter must
    // probe a clean post-reset GPU to populate the GPU manager's probed
    // table. Without SBR, RM sees stale engine state from previous
    // catalyst cycles and silently skips GPU instance registration —
    // GPU_GET_PROBED_IDS returns empty, device_alloc fails with 0x22.
    // Non-catalyst strategies preserve warm state (FLR/SBR suppressed).
    let is_catalyst_strategy = strategy.contains("catalyst");

    // Exp 229 safety: detect degraded PRI ring from prior catalyst cycle.
    // A GPU with PRI faults (0xbadf reads) can lock the PCI subsystem
    // during no_bus_reset manipulation. If degraded, do an SBR via sysfs
    // to clean the GPU before proceeding.
    if is_catalyst_strategy && gpu_warm {
        use toadstool_cylinder::vfio::device::MappedBar;
        if let Ok(bar) = MappedBar::from_sysfs_rw(bdf, 16 * 1024 * 1024) {
            use toadstool_cylinder::nv::register_read::RegisterRead;

            let pri_read = RegisterRead::from_result(bar.read_u32(0x0012_0058));
            let fecs_read = RegisterRead::from_result(bar.read_u32(0x0040_9100));

            // Only a real non-zero interrupt status counts as a fault. An
            // unreadable device reads all-ones on both registers, which the
            // old `!= 0` test scored as degraded and answered with an SBR.
            let is_pri_faulted =
                pri_read.valid().is_some_and(|v| v != 0) || fecs_read.is_pri_fault();

            let pri_intr = pri_read.raw().unwrap_or(0);
            let fecs_cpuctl = fecs_read.raw().unwrap_or(0);
            if is_pri_faulted {
                tracing::warn!(
                    bdf,
                    pri_intr = format_args!("0x{pri_intr:08x}"),
                    fecs_cpuctl = format_args!("0x{fecs_cpuctl:08x}"),
                    "catalyst re-entry: GPU degraded from prior cycle, forcing SBR cleanup"
                );
                let reset_path =
                    toadstool_cylinder::linux_paths::sysfs_pci_device_file(bdf, "reset");
                if let Err(e) = std::fs::write(&reset_path, "1") {
                    tracing::error!(bdf, error = %e, "SBR cleanup failed — proceeding anyway");
                } else {
                    std::thread::sleep(SBR_RESET_SETTLE);
                    tracing::info!(bdf, "SBR cleanup complete — GPU reset to clean state");
                }
            }
        }
    }

    let suppress_sbr = if is_catalyst_strategy {
        tracing::info!(
            bdf,
            gpu_warm,
            "catalyst strategy: allowing SBR for clean RM probe"
        );
        false
    } else {
        gpu_warm
    };

    // Exp 229 fix: exclude target BDF + IOMMU siblings + entire upstream
    // bridge hierarchy from keepalive config reads BEFORE any PCI state
    // changes. During SBR, BOTH the target device AND its upstream bridges
    // can become momentarily unresponsive. A concurrent keepalive config
    // read to ANY device in the hierarchy enters kernel CRS retry and
    // holds the global pci_lock — deadlocking ALL PCI operations
    // system-wide, including the display GPU.
    let mut excluded_bdfs = vec![bdf.to_string()];
    for sib in toadstool_cylinder::vfio::guarded_sysfs::iommu_group_siblings(bdf) {
        excluded_bdfs.push(sib);
    }
    // Walk sysfs ancestry to find all upstream bridges
    let bridge_chain = toadstool_ember::plx_keepalive::detect_pcie_bridges(bdf);
    for bridge_bdf in &bridge_chain {
        if !excluded_bdfs.contains(bridge_bdf) {
            excluded_bdfs.push(bridge_bdf.clone());
        }
    }
    let _keepalive_exclusion =
        crate::background::pcie_keepalive::HandoffExclusionGuard::new(excluded_bdfs);

    toadstool_cylinder::vfio::guarded_sysfs::prepare_anchor_release(bdf, suppress_sbr);

    // Release VFIO anchor and cached device. The IOMMU group is locked
    // while we hold VFIO container/group FDs — the seeder driver cannot
    // bind until we release them. FLR is already suppressed above.
    {
        let mut anchors = handler.anchor_store.lock().await;
        if let Some(anchor) = anchors.remove(bdf) {
            anchor.release_prepared();
        }
    }
    {
        let mut cache = handler.cached_devices.lock().await;
        if cache.remove(bdf).is_some() {
            tracing::info!(bdf, "released cached device for warm handoff");
        }
    }

    // Close any leaked sysfs resource0 fds for this BDF. The sovereign
    // pipeline and health monitoring open BAR0 via sysfs and intentionally
    // leak the fd (MappedBar pattern). The kernel's request_mem_region()
    // in the seeder driver (nvsov/nouveau) will fail if the BAR region
    // is still held open. This was the Exp 219 blocker.
    {
        let bdf_owned = bdf.to_string();
        let closed = toadstool_cylinder::vfio::guarded_sysfs::release_bar0_fds(&bdf_owned);
        if closed > 0 {
            tracing::info!(
                bdf,
                closed,
                "released leaked BAR0 resource0 fds for warm handoff"
            );
        }
    }

    // The handoff changes the GPU's driver binding (vfio → nouveau →
    // vfio), so any pre-existing VFIO BAR0 mapping is invalidated.
    // Pass None — the orchestrator uses sysfs BAR0 for post-handoff
    // tier classification after vfio-pci rebind.
    //
    // Wrapped in tokio::time::timeout to prevent indefinite RPC hangs.
    // The handoff itself has internal deadlines via guarded_sysfs, but
    // this outer timeout is the last line of defense.
    // 420s: catalyst teardown on GV100 needs ~160s for nvidia RM
    // shutdown (HBM2 dealloc, falcon halt) + 15s settle + 30s probe
    // + 30s BAR0 capture margin.
    // Exp 229: activate the catalyst watchdog before entering the blocking
    // handoff. If the pipeline becomes unresponsive (interrupt storm, pci_lock
    // deadlock, etc.), the watchdog will emergency-quench GPU interrupts and
    // kill the ember service to save the system.
    let watchdog_bdf = bdf.to_string();
    let watchdog_profile = toadstool_cylinder::nv::registers::pmc::InterruptProfile::for_sm(
        config.sm_version.unwrap_or(70),
    );
    let _watchdog_guard = crate::background::catalyst_watchdog::activate(
        &watchdog_bdf,
        watchdog_profile,
        Some(CATALYST_WATCHDOG_TIMEOUT),
        &config.module_name,
    );

    let rpc_timeout = WARM_HANDOFF_RPC_TIMEOUT;
    let module_name_for_signal = config.module_name.clone();
    let blocking_future = tokio::task::spawn_blocking(move || {
        toadstool_cylinder::vfio::sovereign_handoff::execute_handoff_with_signals(
            &config,
            None,
            crate::background::catalyst_watchdog::heartbeat,
            {
                let mod_name = module_name_for_signal;
                move |signal| {
                    use toadstool_cylinder::vfio::sovereign_handoff::PipelineSignal;
                    match signal {
                        PipelineSignal::EnterModuleCleanup => {
                            crate::background::catalyst_watchdog::enter_module_cleanup(&mod_name);
                        }
                        PipelineSignal::ExitModuleCleanup => {
                            crate::background::catalyst_watchdog::exit_module_cleanup();
                        }
                    }
                }
            },
        )
    });

    let result = match tokio::time::timeout(rpc_timeout, blocking_future).await {
        Ok(Ok(handoff_result)) => handoff_result,
        Ok(Err(e)) => {
            return Err(JsonRpcError::internal_error(format!(
                "handoff task panicked: {e}"
            )));
        }
        Err(_elapsed) => {
            tracing::error!(
                bdf,
                timeout_s = rpc_timeout.as_secs(),
                "sovereign.warm_handoff RPC timeout — blocking thread abandoned"
            );
            return Err(JsonRpcError::internal_error(format!(
                "warm_handoff timed out after {}s (blocking thread abandoned, \
                 internal guarded operations will self-terminate)",
                rpc_timeout.as_secs(),
            )));
        }
    };

    tracing::info!(
        bdf,
        success = result.success,
        tier = ?result.tier.as_ref().map(|t| t.tier),
        total_ms = result.total_ms,
        "sovereign.warm_handoff: complete"
    );

    serde_json::to_value(&result)
        .map_err(|e| JsonRpcError::internal_error(format!("serialization failed: {e}")))
}
