# toadStool — Evolution Record

The changelog is sorted by capability surface, not by date.
Each section tells the arc: what emerged, what it replaced, what it became.
The date-sorted session history lives in `CHANGELOG_ARCHIVE.md`.

`H(data|epitope) < H(data|date) < H(data)`

---

## GPU Compute — from vendor tools to sovereign silicon

Started as `nvidia-smi`/`lspci` subprocess probes and OpenCL stubs; evolved through wgpu 22→28, VFIO sovereign dispatch, and a unified WGSL shader library. The compute path now spans portable wgpu, Linux VFIO/DRM, and honest CPU fallbacks — no phantom devices, no marketing-string capability guesses.

- **Native GPU discovery** (S382): Sysfs + procfs + PCI config-space liveness replaces vendor CLI tools; finds vfio-pci and unbound devices that `nvidia-smi` misses.
- **Cross-platform wgpu pipeline** (S342–S343, S380): Adapter enumeration wired into self-knowledge, dispatch capabilities, and system queries on Windows/macOS/Linux.
- **wgpu 28 migration** (S380): MSRV 1.92; async adapter API, `ShaderSource::SpirV`, musl Vulkan loader runtime probe via `vulkan_loader_available()`.
- **VFIO sovereign compute** (S245–S259): `toadstool-cylinder` crate absorbs NVIDIA/AMD backends, warm FECS init, PBDMA/QMD dispatch, `NvVfioComputeDevice` factory.
- **GPU warm-boot orchestration** (S337): Monolithic warm path split into 8 step functions (d3hot→d0 through bar2 remap).
- **Silicon ledger routing** (S380): `SiliconUnitUtilization` + idle-aware `route_multi_unit` for multi-die scheduling.
- **Scientific GPU ops** (Sessions 30–31): WGSL library for linalg (QR, SVD, LU), lattice QCD, bioinformatics, MD forces; `GpuExecutor` wired for 30+ MathOps.
- **SubstrateCapability dispatch** (Session 31d): 12-variant capability model auto-probed from wgpu adapter features.
- **OpenCL excised** (S319): C-dependent dead stub deleted; wgpu is the sole portable GPU framework.
- **Vulkano removed** (S378): `runtime/gpu` vulkan feature enables wgpu backend only.

## Hardware Detection & Silicon Registry — knowing what silicon you have

Began with hardcoded BDFs and fake Intel integrated GPUs; became PCI-class enumeration with liveness probes, auto-registration at IPC handshake, and Node-Atomic silicon discovery.

- **PCI accelerator scan** (S382): `pci_discovery::scan_accelerators` pairs cached sysfs identity with live config-space probe; wedged GPUs report location, not absence.
- **Honest VRAM reporting** (S382): Memory only where measurable (amdgpu sysfs); BAR1 never substituted for capacity; unknown scores neutral in selection.
- **Hardware auto-register** (S309): PCI sysfs GPU/NPU inventory in `ipc.register` and `primal.announce` payloads.
- **Silicon discovery AAR** (S374): Queries coralReef `shader.compile.capabilities` at startup for native silicon registry.
- **Kernel sentinel** (S325): `/dev/kmsg` crash/GPU-warn pattern classifier with 28 unit tests.
- **Fake-data elimination** (S355): `DeviceUsage::default()` documented as unmeasured sentinel; frameworks return `not_supported` instead of zeros.
- **Env-var BDF discovery** (S381): Script hardcoded BDFs → `${AMD_BDF}`, `${NVIDIA_BDF}`, `${AKIDA_BDF}` overrides.

## Workload Orchestration & Dispatch — routing compute across gates

Started as hardcoded native runtime and HTTP coordination stubs; became JSON-RPC dispatch with pipeline DAGs, cross-gate trust verification, fan-out parallelism, and yield-to-owner semantics.

- **Pipeline dispatch** (S199): `compute.dispatch.pipeline.submit` executes multi-stage DAGs via Kahn topological sort with per-stage result forwarding.
- **Dispatch telemetry schema** (S286): 36-dim `DispatchTelemetryRecord` + `dispatch.telemetry.schema` RPC for ml.mlp_train consumption.
- **Cross-gate trust** (S286): `dispatch.verify_trust` pre-validates provenance; `_dispatch_trust.source_gate_id` injected on forward.
- **Fan-out dispatch** (S263, S269): `compute.fan_out` for DAG-aware clone-level parallelism with substrate filtering.
- **Yield-to-owner** (S274): `max_guest_load` semantics wired into local dispatch for shared-hardware covalent deployments.
- **Runtime hint inference** (S379): `infer_runtime_type(&workload)` replaces hardcoded `RuntimeType::Native`.
- **WASM/container conversion** (S379): `ExecutionSpec::Wasm` and `Container` wired through `conversion.rs`.
- **Hot-path allocation elimination** (S337): `detect_dispatch_mode` returns `Cow<'static, str>` — zero heap allocs per submit.
- **StubRuntimeEngine** (S333): Fail-fast diagnostics listing available WGPU/VFIO/WASM backends.
- **Wire Standard L3** (S191): `capabilities.list` returns cost estimates and operation dependency chains.

## Transport & IPC — Unix sockets, riboCipher, and dual-socket C2

Evolved from HTTP reqwest calls and plaintext Unix sockets to JSON-RPC 2.0 over capability-discovered paths with riboCipher transport signals and BTSP encrypted channels.

- **Pure Rust IPC core** (v4.9–v4.10): reqwest excised from 30+ crates; all primal communication via Unix JSON-RPC.
- **riboCipher transport** (S311–S315): Tier 1 CLEAR (`0xEC`) on all accept loops; unsignalled connections REJECT with `-32600`; MitoBeacon `0xED` accepted (S320).
- **BTSP encrypted channel** (S205, S215): ChaCha20-Poly1305 handshake with timeout and connection reuse (PG-46).
- **Dual-socket C2** (S354): `compute.tarpc.sock` naming per Wave 156j; `identity.get` and `primal.announce` advertise both JSON-RPC and tarpc paths.
- **Early health responder** (S277, S315): Startup-window `"health"` method before full JSON-RPC dispatch; riboCipher prefix stripped.
- **Socket path centralization** (S308, S328): `BIOMEOS_SOCKET_DIR` wired everywhere; zero `/tmp` writes under `ProtectSystem=strict`.
- **axum excised** (S380): BYOB container server rewritten as UDS JSON-RPC; HTTP is songBird's domain.
- **gRPC deleted** (S319): Dead tarpc/JSON-RPC is canonical; `TOADSTOOL_ENABLE_GRPC` constant removed.
- **126-method RPC audit** (S372): `DIRECT_JSONRPC_METHODS`, `ANNOUNCED_METHODS`, and capability registry aligned.

## Platform Abstraction — hw-safe containment and fleet portability

Started with `rustix`/`libc` scattered across 40+ crates; became G68 five-layer containment with 16/16 native target checks and Windows cross-compile.

- **G68 hw-safe complete** (S365): Zero `rustix` outside `toadstool-hw-safe`; 20+ new trait-backed APIs (DeviceIo, PrivilegeProbe, FilesystemIsolation, vfio DMA).
- **Cross-arch fleet ready** (S369): 16/16 native targets via `.cargo/config.toml` + `scripts/cross-arch-check.sh`.
- **Silicon atheism** (S329–S332): Linux-only hardware crates cfg-gated; `WgpuGpuDiscovery` and `PortableResourceHandle` make every platform first-class.
- **Windows cross-compile** (S363): `cargo check --target x86_64-pc-windows-gnu` succeeds with gated Linux IPC stubs.
- **Musl portability** (S366–S368): ioctl opcode portability, seccompiler gating, rustix arch fallbacks.
- **libc eliminated from cylinder** (S281): Last direct C crate on core hardware path; RM ioctl via `rustix::ioctl::Ioctl` trait.
- **Akida DeviceFile migration** (S362–S363): NPU backends through hw-safe `DeviceFile` trait, not raw syscalls.

## WASM Compute Kernel — types without tokio

Tier 3 goal: compile the orchestration logic on `wasm32-unknown-unknown` as a pure compute kernel, separate from the native deployment layer.

- **Runtime feature gate** (S374): `tokio` optional in core `toadstool`; crate compiles as types+logic library without `runtime`.
- **WASM coverage arc** (S370→S376): 10 → 24 → 31 → 38 of 48 workspace crates WASM-capable via `default-features = false` pattern.
- **Types extraction** (S372): ~5K lines (workload, resources, security, execution) moved to `toadstool-core` for clean downstream.
- **wasmi runtime** (S378): `runtime/wasm` uses wasmi (pure Rust), not wasmtime; cache made synchronous.
- **UUID WASM safety** (S374): `generate_uuid()` returns nil on WASM, v4 on native.

## Spring Absorption — sovereign compute from hotSpring and allies

Wave 8+ absorbed glowplug, coral-ember, and cylinder into native crates; subsequent waves pulled WGSL shaders and dispatch patterns from hotSpring, wetSpring, and neuralSpring.

- **Compute trio foundation** (S235–S239): glowplug (sysfs swap), coral-ember (VFIO anchor), cylinder (NVIDIA MMIO) unified under toadStool workspace.
- **Phase C backend absorption** (S245–S248): AMD/NVIDIA/VFIO/MMIO backends migrated from hotSpring into `toadstool-cylinder`.
- **Diesel engine / GR context** (S256, S262): FECS warm-state init, `init_gr_context` IPC, shader metadata aliases in QMD path.
- **hotSpring audit responses** (S255–S261): PBDMA dispatch, sovereign init tiers, Kepler wiring, health RPC surface.
- **Cross-spring WGSL** (Session 31d): hotSpring Dirac/CG kernels, wetSpring bio ops (HMM, SNP, DADA2), neuralSpring eigensolver confirmed present.
- **Glowplug discovery split** (S326): PCI/sysfs functions extracted to `glowplug_discovery.rs`; portable swap executor for non-Linux.
- **Inter-primal naming cleanup** (S381): `coral_client` → `shader_service`; capability names, not primal identities.

## Security, Sandboxing & Trust — encrypted dispatch without primal coupling

Migrated from deprecated `distributed::security` to `crypto_integration`; sandbox working directories wired; intrusion detection behind honest feature gates.

- **Crypto integration migration** (S285, S336): Server encrypt/decrypt off deprecated security module; `CryptoServiceClient` is canonical.
- **BTSP transport** (S205–S218): Encrypted compute dispatch with handshake timeout, connection reuse, Phase 3 verification.
- **Sandbox production wiring** (S266): `SandboxSpec.working_directory` creates dirs inside sandbox metadata.
- **Intrusion/rate-limit hardening** (S201): Auto-ban, risk scoring, input validation — behind non-default `hardening` feature (S378).
- **Mock auth gated** (Feb 2026): Production builds require real signing key; `dev-mock-auth` feature for development only.
- **Capability-domain discovery** (S172–S173): `songbird`→`coordination`, `beardog`→`security` env vars with legacy fallbacks.
- **await_holding_lock denied** (S383b): Workspace `clippy::await_holding_lock = deny`; ten `!Send` bugs fixed across orchestrator and backends.

## NUCLEUS & Biome Composition — one manifest, many primals

Five divergent `BiomeManifest` structs consolidated into a single canonical type with composition graphs and gossip event taxonomy.

- **Canonical BiomeManifest** (S375): `toadstool-core/src/manifest.rs` with compositions, gossip events, federation config.
- **Manifest convergence** (S377): 5→2 structs; CLI, daemon, biomeOS, integration-primals all re-export canonical type.
- **Example biome** (S375): `biome-strandgate.yaml` with tower-atomic and node-atomic sub-graphs.
- **Gossip events spec** (S375): `specs/GOSSIP_EVENTS.md` taxonomy for swarmVine hardware/silicon/workload events.
- **Composition graduation** (S322): Pipeline and graph-node coverage tests for multi-primal biomes.

## Neuromorphic / NPU — Akida on PCI

Akida NPU support evolved from empty feature stubs to honest hw-safe MMIO with metalForge-validated capabilities.

- **akida-driver crate** (S353): Moved out of default workspace (biomeGate-local `akida-chip` dep); builds with `-p akida-driver`.
- **MMIO safety** (S282, S288): Panicking `read32/write32` → `try_read32/try_write32` with `?` propagation.
- **metalForge capabilities** (Session 30): Mesh topology, clock mode, batch capabilities, weight mutation — all sysfs-discovered.
- **Honest NPU feature** (S378): Empty `akida` stub removed; CLI `npu` documents last-mile adapter wiring requirement.
- **Platform backends split** (S381): 805-line monolith → 7 focused modules preserving unsafe containment zones.

## Display & Scientific Computing — BarraCuda and DRM

Display backend foundation for petalTongue collaboration; BarraCuda absorbed as the CPU/GPU scientific computing library with auto-dispatch.

- **Display backend crate** (v4.18): Pure Rust DRM via `linux-drm`, evdev input, XDG capability discovery — zero libgbm.
- **BarraCuda auto-dispatch** (Feb 2026): Per-operation CPU/GPU thresholds with `DispatchTarget` routing.
- **f64 linalg bridges** (Feb 2026): Cholesky, symmetric/generalized eigh, cubic splines, LOO-CV surrogates.
- **FHE GPU validation** (Feb 2026): Homomorphic encryption paths validated on GPU tensor ops.

## Dependencies & Runtime Architecture — tokio blast radius reduction

~35k LOC of vestigial modules feature-gated; tokio surface cut nearly in half; dependency pandemic addressed tier-by-tier.

- **Vestigial segmentation** (S378): `legacy-cloud`, `legacy-security`, `legacy-scheduler`, `hardening`, `background-monitors` — all non-default, `#[deprecated]`.
- **Tokio elimination arc** (S376–S379): `tokio::fs` and `tokio::process` → std equivalents; 118→65 production files using tokio (45% reduction).
- **G72 dependency pandemic** (S379–S380): 7 dead deps removed; 6 promoted to workspace; tokio `signal` scoped to CLI+server only.
- **Runtime edge excluded** (S378): Orphaned crate moved to workspace `exclude`.
- **UniBin architecture** (v4.10): Single `toadstool` binary — CLI + daemon modes; first UniBin primal in ecosystem.
- **Pure Rust core** (v4.9–v4.10): Zero ring/TLS in core; concentrated gap — songBird owns TLS.

## Test Infrastructure — from silent rot to compile gates

21 test targets silently stopped compiling while CI ran `--lib` only; S382–S383 recovered compilation and made `--no-run` a gate.

- **Workspace test compile gate** (S383): `cargo test --workspace --no-run` exits 0 for first time; now CI-enforced.
- **104 integration tests recovered** (S383): detection and GPU coverage suites unblocked after `!Send` fixes.
- **Test extraction waves** (S333–S336): Inline `#[cfg(test)]` modules moved to sibling `_tests.rs` files across 20+ production files.
- **Honest skip accounting** (S382): 510 tests compile-skip on non-default features — build fix, not recovered coverage.
- **Router contract tests** (S326): Duplicate detection, minimum method count, provenance shape validation.
- **Dispatcher E2E** (S324): Mock Unix socket tests for riboCipher + NDJSON full forward path.

## Deep Debt & Code Quality — continuous structural evolution

Recurring theme across 380+ sessions: split oversized files, centralize env vars, replace primal names with capability terms, eliminate production panics.

- **Env centralization** (S280, S321): ~200 raw `std::env::var` sites → `socket_env::` constants (64%+ centralized).
- **Capability-based naming** (S172, S202, S355): ~105 foreign primal references evolved to capability domains.
- **File decomposition** (S173, S281, S373): 650L+ files systematically split at natural module boundaries; 750L gate enforced.
- **Production panic elimination** (S273–S313): `unreachable!()` and panicking MMIO wrappers → typed errors throughout.
- **Unsafe containment** (S173, S281): 101→89 unsafe blocks consolidated into hw-safe with documented SAFETY contracts.
- **Deprecated symbol evolution** (S304–S317): Sync ctors deleted, dead GRPC/WebSocket federation removed, Category A symbols purged.
- **Clippy zero** (S339, recurring): MSRV-safe lint resolution; `-D warnings` maintained across workspace.

## Foundation — self-knowledge and ecosystem position

Production-ready baseline (v0.1.0) established self-knowledge architecture: no hardcoded service dependencies, capability-based discovery, inter-primal showcases.

- **Self-knowledge architecture** (v0.1.0): Runtime capability discovery; zero hardcoded primal endpoints.
- **Inter-primal showcases** (v0.1.0): BearDog encryption, NestGate storage, Songbird coordination, Squirrel AI — all capability-routed.
- **Concentrated gap** (v4.10): 100% pure Rust core communication; TLS only in songBird.
- **18K+ test suite** (v4.10 peak): 87% coverage maintained through evolution; lib tests now ~8,500 with honest integration accounting.

---

*Full date-sorted history: `CHANGELOG_ARCHIVE.md`*
*Last compressed: Wave 172*
