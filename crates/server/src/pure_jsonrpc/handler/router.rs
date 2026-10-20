// SPDX-License-Identifier: AGPL-3.0-or-later
//! JSON-RPC method routing tables.
//!
//! Separated from `mod.rs` to keep per-file complexity under 750 lines.
//! Contains `handle_method` (direct literal dispatch) and
//! `dispatch_by_impl_name` (semantic registry dispatch).

use std::path::PathBuf;
use std::sync::atomic::Ordering;

use tracing::debug;

use super::method_gate::CallerContext;
#[cfg(all(target_os = "linux", feature = "display"))]
use super::transport::TransportHandler;
use super::{JsonRpcHandler, core, extract_caller_context};
#[cfg(target_os = "linux")]
use super::{mmio, sovereign};
use crate::pure_jsonrpc::types::{JsonRpcError, JsonRpcErrorExt};

type JsonRpcResult = Result<serde_json::Value, JsonRpcError>;

impl JsonRpcHandler {
    /// Route a method name to its handler.
    ///
    /// Resolution order:
    /// 0. Pre-dispatch gate check (JH-0/JH-2: permissive default, enforcing future).
    /// 1. Direct literal match (backward-compatible `toadstool.*` and `compute.*` names).
    /// 2. Semantic registry lookup: `{domain}.{operation}` → implementation name → handler.
    pub(super) async fn handle_method(
        &self,
        method: &str,
        params: Option<&serde_json::Value>,
        conn: super::ConnectionTrustHints,
    ) -> JsonRpcResult {
        let caller_ctx = extract_caller_context(conn, self.local_gate_id.as_ref());

        self.gate.check_with_context(method, &caller_ctx)?;

        match method {
            "auth.check" => return super::auth::auth_check(&self.gate, params),
            "auth.mode" => return super::auth::auth_mode(&self.gate),
            "auth.peer_info" => return super::auth::auth_peer_info(&caller_ctx),

            "toadstool.submit_workload" => return self.workload.submit_workload(params).await,
            "toadstool.query_status" => return self.job.query_status(params).await,
            "toadstool.cancel_workload" => return self.workload.cancel_workload(params).await,
            "toadstool.list_workloads" => return self.job.list_workloads(params).await,
            "toadstool.validate" => return self.workload.validate(params).await,
            "toadstool.query_capabilities" => return self.workload.query_capabilities().await,
            "health" => {
                return core::health_simple(&self.version).await;
            }
            "toadstool.health" | "health.check" => {
                return core::health(&self.version, self.start_time, &self.error_count).await;
            }
            "health.liveness" => {
                return core::health_liveness().await;
            }
            "health.readiness" => {
                return core::health_readiness(
                    self.version.as_ref(),
                    self.ready.load(Ordering::Relaxed),
                )
                .await;
            }
            "health.version" => {
                return core::health_version(self.version.as_ref()).await;
            }
            "health.drain" => {
                return core::health_drain(&self.draining, &self.ready).await;
            }
            "identity.get" => {
                return core::identity_get(&self.version, &self.semantic_registry).await;
            }
            "toadstool.version" => return core::version_info(&self.version).await,

            "toadstool.resources.estimate"
            | "toadstool.ai.local_inference"
            | "resources.estimate"
            | "ai.local_inference" => return self.resources.resources_estimate(params).await,
            "toadstool.resources.validate_availability"
            | "toadstool.ai.local_execute"
            | "resources.validate_availability"
            | "ai.local_execute" => {
                return self.resources.resources_validate_availability(params).await;
            }
            "toadstool.resources.suggest_optimizations" | "resources.suggest_optimizations" => {
                return self.resources.resources_suggest_optimizations(params).await;
            }

            "compute.health" => {
                return core::health(&self.version, self.start_time, &self.error_count).await;
            }
            "compute.version" => return core::version_info(&self.version).await,
            "capabilities.list" | "capability.list" | "primal.capabilities" => {
                return core::capabilities_list(&self.semantic_registry, &self.version).await;
            }
            "primal.announce" => {
                return core::primal_announce(
                    &self.version,
                    &self.semantic_registry,
                    self.bound_socket_path.as_deref().map(PathBuf::as_path),
                )
                .await;
            }
            "compute.capabilities" => return self.workload.query_capabilities().await,
            "compute.discover_capabilities" => {
                return core::discover_capabilities(&self.semantic_registry, &self.version).await;
            }

            "compute.execute" => return self.workload.submit_workload(params).await,
            "compute.submit" => return self.job.compute_submit(params).await,
            "compute.status" => return self.job.compute_status(params).await,
            "compute.result" => return self.job.compute_result(params).await,
            "compute.cancel" => return self.job.compute_cancel(params).await,
            "compute.list" => return self.job.compute_list(params).await,

            "compute.dispatch" | "compute.dispatch.submit" => {
                return self
                    .dispatch
                    .dispatch_submit_with_context(params, &caller_ctx)
                    .await;
            }
            "compute.fan_out" => {
                return self.dispatch.fan_out(params, &caller_ctx).await;
            }
            "compute.dispatch.status" => return self.dispatch.dispatch_status(params).await,
            "compute.dispatch.result" => return self.dispatch.dispatch_result(params).await,
            "compute.dispatch.forward" => return self.dispatch.dispatch_forward(params).await,
            "dispatch.verify_trust" => {
                return Ok(super::dispatch::trust::verify_trust(&caller_ctx, params));
            }
            "compute.dispatch.capabilities" => {
                return self.dispatch.dispatch_capabilities(params).await;
            }
            "compute.dispatch.pipeline.submit" => {
                return self
                    .dispatch
                    .pipeline_submit_with_context(params, &caller_ctx)
                    .await;
            }
            "compute.dispatch.pipeline.status" => {
                return self.dispatch.pipeline_status(params).await;
            }
            "compute.dispatch.multi_gpu" => {
                return self.dispatch.compute_dispatch_multi_gpu(params).await;
            }
            "compute.dispatch.halo_exchange" => {
                return self.dispatch.compute_dispatch_halo_exchange(params).await;
            }
            "dispatch.telemetry.schema" => {
                return Ok(super::dispatch::telemetry::telemetry_schema());
            }

            "gpu.query_info" | "gpu.info" => return core::gpu_info().await,
            "gpu.query_memory" | "gpu.memory" => return core::gpu_memory().await,
            #[cfg(target_os = "linux")]
            "gpu.query_telemetry" | "gpu.telemetry" => {
                return self.hw_learn.gpu_telemetry(params).await;
            }

            "gate.update" => return self.job.gate_update(params).await,
            "gate.remove" => return self.job.gate_remove(params).await,
            "gate.list" => return self.job.gate_list().await,
            "gate.route" => return self.job.gate_route(params).await,

            #[cfg(target_os = "linux")]
            method if Self::is_linux_only_method(method) => {
                return self.handle_linux_method(method, params, &caller_ctx).await;
            }

            "shader.dispatch" => {
                return self
                    .dispatch
                    .shader_dispatch_with_context(params, &caller_ctx)
                    .await;
            }

            "compute.performance_surface.report" => {
                return self.silicon.report(params).await;
            }
            "compute.performance_surface.query" => {
                return self.silicon.query(params).await;
            }
            "compute.performance_surface.list" => return self.silicon.list().await,
            "compute.route.multi_unit" => {
                return self.silicon.route_multi_unit(params).await;
            }
            "compute.silicon_ledger.report" => {
                return self.silicon.silicon_ledger_report(params).await;
            }
            "compute.silicon_ledger.query" => {
                return self.silicon.silicon_ledger_query(params).await;
            }

            #[cfg(target_os = "linux")]
            "compute.silicon.registry" => {
                return self.silicon_registry_status().await;
            }

            "provenance.query" | "provenance.get" | "toadstool.provenance" => {
                return Self::toadstool_provenance().await;
            }

            _ => {}
        }

        if let Some(impl_name) = self.semantic_registry.resolve(method) {
            debug!("Semantic resolve: {} → {}", method, impl_name);
            return self
                .dispatch_by_impl_name(impl_name, params, &caller_ctx)
                .await;
        }

        Err(JsonRpcError::method_not_found(method))
    }

    async fn dispatch_by_impl_name(
        &self,
        impl_name: &str,
        params: Option<&serde_json::Value>,
        ctx: &CallerContext,
    ) -> JsonRpcResult {
        match impl_name {
            "execute_workload" | "submit_workload" => self.workload.submit_workload(params).await,
            "get_workload_status" | "query_status" => self.job.query_status(params).await,
            "cancel_workload" => self.workload.cancel_workload(params).await,
            "list_workloads" => self.job.list_workloads(params).await,
            "validate" => self.workload.validate(params).await,
            "query_capabilities" => self.workload.query_capabilities().await,
            "check_health" => core::health(&self.version, self.start_time, &self.error_count).await,
            "health_version" => core::health_version(self.version.as_ref()).await,
            "health_drain" => core::health_drain(&self.draining, &self.ready).await,
            "dispatch_submit" => {
                self.dispatch
                    .dispatch_submit_with_context(params, ctx)
                    .await
            }
            "compute_fan_out" => self.dispatch.fan_out(params, ctx).await,
            "dispatch_status" => self.dispatch.dispatch_status(params).await,
            "dispatch_result" => self.dispatch.dispatch_result(params).await,
            "dispatch_capabilities" => self.dispatch.dispatch_capabilities(params).await,
            "shader_dispatch" => {
                self.dispatch
                    .shader_dispatch_with_context(params, ctx)
                    .await
            }
            "pipeline_submit" => {
                self.dispatch
                    .pipeline_submit_with_context(params, ctx)
                    .await
            }
            "pipeline_status" => self.dispatch.pipeline_status(params).await,
            "primal_announce" => {
                core::primal_announce(
                    &self.version,
                    &self.semantic_registry,
                    self.bound_socket_path.as_deref().map(PathBuf::as_path),
                )
                .await
            }
            "science_compute_submit" => self.workload.submit_workload(params).await,
            "science_compute_status" => self.job.query_status(params).await,
            "science_compute_result" => self.dispatch.dispatch_result(params).await,
            "science_compute_cancel" => self.workload.cancel_workload(params).await,
            "science_gpu_dispatch" => {
                self.dispatch
                    .shader_dispatch_with_context(params, ctx)
                    .await
            }
            "science_gpu_capabilities" => self.dispatch.dispatch_capabilities(params).await,
            "science_npu_dispatch" => {
                self.dispatch
                    .dispatch_submit_with_context(params, ctx)
                    .await
            }
            "science_npu_capabilities" => self.dispatch.dispatch_capabilities(params).await,
            "science_substrate_discover" => self.workload.query_capabilities().await,
            "science_substrate_probe" => self.workload.query_capabilities().await,
            "inference_list_models" => self.resources.resources_estimate(params).await,
            "inference_execute" => self.resources.resources_estimate(params).await,
            "inference_load_model" => self.resources.resources_estimate(params).await,
            "inference_unload_model" => self.resources.resources_estimate(params).await,
            "toadstool_provenance" => Self::toadstool_provenance().await,
            "gpu_info" => core::gpu_info().await,
            "gpu_memory" => core::gpu_memory().await,
            #[cfg(target_os = "linux")]
            impl_name if Self::is_linux_only_impl(impl_name) => {
                self.dispatch_linux_impl(impl_name, params, ctx).await
            }
            "performance_surface_report" => self.silicon.report(params).await,
            "performance_surface_query" => self.silicon.query(params).await,
            "performance_surface_list" => self.silicon.list().await,
            "route_multi_unit" => self.silicon.route_multi_unit(params).await,
            "silicon_ledger_report" => self.silicon.silicon_ledger_report(params).await,
            "silicon_ledger_query" => self.silicon.silicon_ledger_query(params).await,
            "auth_check" => super::auth::auth_check(&self.gate, params),
            "auth_mode" => super::auth::auth_mode(&self.gate),
            "auth_peer_info" => super::auth::auth_peer_info(ctx),
            _ => Err(JsonRpcError::method_not_found(impl_name)),
        }
    }

    #[expect(
        clippy::unused_async,
        reason = "async for JSON-RPC handler trait consistency"
    )]
    async fn toadstool_provenance() -> JsonRpcResult {
        Ok(toadstool::cross_spring_provenance::provenance_json())
    }
}

#[cfg(target_os = "linux")]
impl JsonRpcHandler {
    #[cfg(feature = "display")]
    fn is_transport_method(method: &str) -> bool {
        matches!(
            method,
            "transport.discover"
                | "transport.list"
                | "transport.route"
                | "transport.open"
                | "transport.stream"
                | "transport.status"
        )
    }

    fn is_linux_only_method(method: &str) -> bool {
        #[cfg(feature = "display")]
        if Self::is_transport_method(method) {
            return true;
        }
        matches!(
            method,
            "compute.hardware.observe"
                | "compute.hardware.distill"
                | "compute.hardware.apply"
                | "compute.hardware.share_recipe"
                | "compute.hardware.auto_init"
                | "compute.hardware.auto_init_all"
                | "compute.hardware.status"
                | "compute.hardware.vfio_devices"
                | "ember.list"
                | "ember.status"
                | "ember.reacquire"
                | "ember.warm_cycle"
                | "ember.prepare_dma"
                | "ember.cleanup_dma"
                | "ember.adopt_device"
                | "device.swap"
                | "device.warm_catch"
                | "device.get"
                | "device.experiment_lifecycle"
                | "device.reset"
                | "device.resurrect"
                | "device.health"
                | "device.vfio.open"
                | "device.vfio.roundtrip"
                | "device.gr.init"
                | "compute.context.init"
                | "sovereign.init"
                | "sovereign.boot"
                | "sovereign.profile"
                | "sovereign.warm_status"
                | "sovereign.defense_status"
                | "sovereign.watchdog_status"
                | "sovereign.ce_validate"
                | "ce.validate"
                | "sovereign.pmu_investigate"
                | "pmu.investigate"
                | "sovereign.warm_handoff"
                | "sovereign.catalyst_boot"
                | "sovereign.runlist_diagnostic"
                | "sovereign.classify_tier"
                | "sovereign.experiment"
                | "sovereign.devinit"
                | "sovereign.kernel_health"
                | "sovereign.snapshot"
                | "sovereign.compare"
                | "sovereign.catalyst_diff"
                | "sovereign.reagent_capture"
                | "sovereign.recipe_replay"
                | "sovereign.runtime_services_probe"
                | "mmio.read32"
                | "mmio.write32"
                | "mmio.batch"
                | "mmio.pramin.read32"
                | "mmio.bar0.probe"
                | "mmio.falcon.status"
                | "ember.falcon.upload_imem"
                | "ember.falcon.upload_dmem"
                | "ember.falcon.start_cpu"
                | "ember.falcon.poll"
                | "ember.pramin.write"
                | "ember.pramin.read"
                | "ember.fecs.state"
                | "ember.device.health"
                | "ember.device.recover"
        )
    }

    async fn handle_linux_method(
        &self,
        method: &str,
        params: Option<&serde_json::Value>,
        caller_ctx: &CallerContext,
    ) -> JsonRpcResult {
        match method {
            #[cfg(feature = "display")]
            "transport.discover" => Ok(TransportHandler::transport_discover(params)),
            #[cfg(feature = "display")]
            "transport.list" => self.transport.transport_list().await,
            #[cfg(feature = "display")]
            "transport.route" => self.transport.transport_route(params).await,
            #[cfg(feature = "display")]
            "transport.open" => self.transport.transport_open(params).await,
            #[cfg(feature = "display")]
            "transport.stream" => self.transport.transport_stream(params).await,
            #[cfg(feature = "display")]
            "transport.status" => self.transport.transport_status(params).await,
            "compute.hardware.observe" => self.hw_learn.hw_learn_observe(params).await,
            "compute.hardware.distill" => self.hw_learn.hw_learn_distill(params).await,
            "compute.hardware.apply" => self.hw_learn.hw_learn_apply(params).await,
            "compute.hardware.share_recipe" => self.hw_learn.hw_learn_share_recipe(params).await,
            "compute.hardware.auto_init" => self.hw_learn.hw_learn_auto_init(params).await,
            "compute.hardware.auto_init_all" => self.hw_learn.hw_learn_auto_init_all(params).await,
            "compute.hardware.status" => self.hw_learn.hw_learn_status(params).await,
            "compute.hardware.vfio_devices" => self.hw_learn.hw_learn_vfio_devices(params).await,
            "ember.list" => Ok(self.ember_list()),
            "ember.status" => Ok(self.ember_status()),
            "ember.reacquire" => self.ember_reacquire(params).await,
            "ember.warm_cycle" => self.ember_warm_cycle(params).await,
            "ember.prepare_dma" => self.dispatch.ember_prepare_dma(params).await,
            "ember.cleanup_dma" => self.dispatch.ember_cleanup_dma(params).await,
            "ember.adopt_device" => self.ember_adopt_device(params).await,
            "device.swap" => self.device_swap(params).await,
            "device.warm_catch" => self.device_warm_catch(params),
            "device.get" => self.device_get(params),
            "device.experiment_lifecycle" => self.device_experiment_lifecycle(params),
            "device.reset" => self.device_reset(params),
            "device.resurrect" => self.device_resurrect(params).await,
            "device.health" => mmio::ember_device_health(params),
            "device.vfio.open" => self.dispatch.device_vfio_open(params, caller_ctx).await,
            "device.vfio.roundtrip" => {
                self.dispatch
                    .device_vfio_roundtrip(params, caller_ctx)
                    .await
            }
            "device.gr.init" | "compute.context.init" => self.dispatch.device_gr_init(params).await,
            "sovereign.init" | "sovereign.boot" => self.dispatch.sovereign_init_ember(params).await,
            "sovereign.profile" => self.dispatch.sovereign_profile_ember(params).await,
            "sovereign.warm_status" => self.dispatch.sovereign_warm_status().await,
            "sovereign.defense_status" => {
                Ok(crate::background::catalyst_watchdog::defense_status())
            }
            "sovereign.watchdog_status" => {
                Ok(crate::background::catalyst_watchdog::watchdog_status())
            }
            "sovereign.ce_validate" | "ce.validate" => {
                self.dispatch.sovereign_ce_validate_ember(params).await
            }
            "sovereign.pmu_investigate" | "pmu.investigate" => {
                self.dispatch.sovereign_pmu_investigate(params).await
            }
            "sovereign.warm_handoff" => self.dispatch.sovereign_warm_handoff(params).await,
            "sovereign.catalyst_boot" => self.dispatch.sovereign_catalyst_boot(params).await,
            "sovereign.runlist_diagnostic" => {
                self.dispatch.sovereign_runlist_diagnostic(params).await
            }
            "sovereign.classify_tier" => sovereign::sovereign_classify_tier(params),
            "sovereign.experiment" => sovereign::sovereign_experiment(params),
            "sovereign.devinit" => sovereign::sovereign_devinit(params),
            "sovereign.kernel_health" => sovereign::sovereign_kernel_health(params),
            "sovereign.snapshot" => sovereign::sovereign_snapshot(params),
            "sovereign.compare" => sovereign::sovereign_compare(params),
            "sovereign.catalyst_diff" => sovereign::sovereign_catalyst_diff(params),
            "sovereign.reagent_capture" => sovereign::sovereign_reagent_capture(params),
            "sovereign.recipe_replay" => sovereign::sovereign_recipe_replay(params),
            "sovereign.runtime_services_probe" => {
                sovereign::sovereign_runtime_services_probe(params)
            }
            "mmio.read32" => mmio::mmio_read32(params),
            "mmio.write32" => mmio::mmio_write32(params),
            "mmio.batch" => mmio::mmio_batch(params),
            "mmio.pramin.read32" => mmio::mmio_pramin_read32(params),
            "mmio.bar0.probe" => mmio::mmio_bar0_probe(params),
            "mmio.falcon.status" => mmio::mmio_falcon_status(params),
            "ember.falcon.upload_imem" => mmio::falcon_upload_imem(params),
            "ember.falcon.upload_dmem" => mmio::falcon_upload_dmem(params),
            "ember.falcon.start_cpu" => mmio::falcon_start_cpu(params),
            "ember.falcon.poll" => mmio::falcon_poll(params),
            "ember.pramin.write" => mmio::pramin_write(params),
            "ember.pramin.read" => mmio::pramin_read(params),
            "ember.fecs.state" => mmio::ember_fecs_state(params),
            "ember.device.health" => mmio::ember_device_health(params),
            "ember.device.recover" => mmio::ember_device_recover(params),
            _ => Err(JsonRpcError::method_not_found(method)),
        }
    }

    fn is_linux_only_impl(impl_name: &str) -> bool {
        matches!(
            impl_name,
            "gpu_telemetry"
                | "hw_learn_observe"
                | "hw_learn_distill"
                | "hw_learn_apply"
                | "hw_learn_share_recipe"
                | "hw_learn_status"
                | "hw_learn_auto_init"
                | "hw_learn_auto_init_all"
                | "hw_learn_vfio_devices"
                | "ember_list"
                | "ember_status"
                | "ember_reacquire"
                | "ember_warm_cycle"
                | "ember_prepare_dma"
                | "ember_cleanup_dma"
                | "ember_adopt_device"
                | "device_swap"
                | "device_warm_catch"
                | "device_get"
                | "device_experiment_lifecycle"
                | "device_reset"
                | "device_resurrect"
                | "ember_device_health"
                | "device_vfio_open"
                | "device_vfio_roundtrip"
                | "device_gr_init"
                | "sovereign_init"
                | "sovereign_init_ember"
                | "sovereign_boot"
                | "sovereign_devinit"
                | "sovereign_defense_status"
                | "sovereign_watchdog_status"
                | "mmio_read32"
                | "mmio_write32"
                | "mmio_batch"
                | "mmio_pramin_read32"
                | "mmio_bar0_probe"
                | "mmio_falcon_status"
                | "falcon_upload_imem"
                | "falcon_upload_dmem"
                | "falcon_start_cpu"
                | "falcon_poll"
                | "pramin_write"
                | "pramin_read"
                | "ember_fecs_state"
                | "ember_device_recover"
        )
    }

    async fn dispatch_linux_impl(
        &self,
        impl_name: &str,
        params: Option<&serde_json::Value>,
        ctx: &CallerContext,
    ) -> JsonRpcResult {
        match impl_name {
            "gpu_telemetry" => self.hw_learn.gpu_telemetry(params).await,
            "hw_learn_observe" => self.hw_learn.hw_learn_observe(params).await,
            "hw_learn_distill" => self.hw_learn.hw_learn_distill(params).await,
            "hw_learn_apply" => self.hw_learn.hw_learn_apply(params).await,
            "hw_learn_share_recipe" => self.hw_learn.hw_learn_share_recipe(params).await,
            "hw_learn_status" => self.hw_learn.hw_learn_status(params).await,
            "hw_learn_auto_init" => self.hw_learn.hw_learn_auto_init(params).await,
            "hw_learn_auto_init_all" => self.hw_learn.hw_learn_auto_init_all(params).await,
            "hw_learn_vfio_devices" => self.hw_learn.hw_learn_vfio_devices(params).await,
            "ember_list" => Ok(self.ember_list()),
            "ember_status" => Ok(self.ember_status()),
            "ember_reacquire" => self.ember_reacquire(params).await,
            "ember_warm_cycle" => self.ember_warm_cycle(params).await,
            "ember_prepare_dma" => self.dispatch.ember_prepare_dma(params).await,
            "ember_cleanup_dma" => self.dispatch.ember_cleanup_dma(params).await,
            "ember_adopt_device" => self.ember_adopt_device(params).await,
            "device_swap" => self.device_swap(params).await,
            "device_warm_catch" => self.device_warm_catch(params),
            "device_get" => self.device_get(params),
            "device_experiment_lifecycle" => self.device_experiment_lifecycle(params),
            "device_reset" => self.device_reset(params),
            "device_resurrect" => self.device_resurrect(params).await,
            "ember_device_health" => mmio::ember_device_health(params),
            "device_vfio_open" => self.dispatch.device_vfio_open(params, ctx).await,
            "device_vfio_roundtrip" => self.dispatch.device_vfio_roundtrip(params, ctx).await,
            "device_gr_init" => self.dispatch.device_gr_init(params).await,
            "sovereign_init" => sovereign::sovereign_init(params),
            "sovereign_init_ember" | "sovereign_boot" => {
                self.dispatch.sovereign_init_ember(params).await
            }
            "sovereign_devinit" => sovereign::sovereign_devinit(params),
            "sovereign_defense_status" => {
                Ok(crate::background::catalyst_watchdog::defense_status())
            }
            "sovereign_watchdog_status" => {
                Ok(crate::background::catalyst_watchdog::watchdog_status())
            }
            "mmio_read32" => mmio::mmio_read32(params),
            "mmio_write32" => mmio::mmio_write32(params),
            "mmio_batch" => mmio::mmio_batch(params),
            "mmio_pramin_read32" => mmio::mmio_pramin_read32(params),
            "mmio_bar0_probe" => mmio::mmio_bar0_probe(params),
            "mmio_falcon_status" => mmio::mmio_falcon_status(params),
            "falcon_upload_imem" => mmio::falcon_upload_imem(params),
            "falcon_upload_dmem" => mmio::falcon_upload_dmem(params),
            "falcon_start_cpu" => mmio::falcon_start_cpu(params),
            "falcon_poll" => mmio::falcon_poll(params),
            "pramin_write" => mmio::pramin_write(params),
            "pramin_read" => mmio::pramin_read(params),
            "ember_fecs_state" => mmio::ember_fecs_state(params),
            "ember_device_recover" => mmio::ember_device_recover(params),
            _ => Err(JsonRpcError::method_not_found(impl_name)),
        }
    }

    /// `compute.silicon.registry` — expose discovered shader compiler capabilities
    /// from the silicon registry (populated by background IPC query).
    #[cfg(target_os = "linux")]
    async fn silicon_registry_status(&self) -> JsonRpcResult {
        let reg = self
            .silicon_registry
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let status = match &reg.status {
            toadstool_integration_primals::shader_compiler::ShaderCompilerStatus::Unknown => {
                "unknown"
            }
            toadstool_integration_primals::shader_compiler::ShaderCompilerStatus::Available(_) => {
                "available"
            }
            toadstool_integration_primals::shader_compiler::ShaderCompilerStatus::Unavailable => {
                "unavailable"
            }
            toadstool_integration_primals::shader_compiler::ShaderCompilerStatus::Error(_) => {
                "error"
            }
        };

        let compiler_version = match &reg.status {
            toadstool_integration_primals::shader_compiler::ShaderCompilerStatus::Available(v) => {
                Some(v.as_str())
            }
            _ => None,
        };

        let error_detail = match &reg.status {
            toadstool_integration_primals::shader_compiler::ShaderCompilerStatus::Error(e) => {
                Some(e.as_str())
            }
            _ => None,
        };

        let (backends, precision_modes, gemm_tiling, integer_subgroup, max_workgroup) =
            if let Some(caps) = &reg.capabilities {
                (
                    serde_json::json!(caps.backends),
                    serde_json::json!(caps.precision_modes),
                    caps.gemm_tiling,
                    caps.integer_subgroup,
                    caps.max_workgroup_size,
                )
            } else {
                (
                    serde_json::json!([]),
                    serde_json::json!([]),
                    false,
                    false,
                    None,
                )
            };

        Ok(serde_json::json!({
            "status": status,
            "compiler_version": compiler_version,
            "error": error_detail,
            "backends": backends,
            "precision_modes": precision_modes,
            "gemm_tiling": gemm_tiling,
            "integer_subgroup": integer_subgroup,
            "max_workgroup_size": max_workgroup,
        }))
    }
}

#[cfg(test)]
#[path = "router_tests.rs"]
mod router_tests;
