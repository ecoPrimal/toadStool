// SPDX-License-Identifier: AGPL-3.0-or-later
#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![expect(
    deprecated,
    reason = "IPC addressing requires well-known names during migration"
)]
#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::doc_markdown,
    clippy::missing_errors_doc,
    clippy::uninlined_format_args,
    clippy::must_use_candidate,
    clippy::items_after_statements,
    clippy::map_unwrap_or,
    clippy::redundant_closure_for_method_calls,
    clippy::unused_self,
    clippy::ref_option,
    clippy::explicit_iter_loop,
    clippy::return_self_not_must_use,
    clippy::match_same_arms,
    clippy::unused_async,
    clippy::format_push_string,
    clippy::used_underscore_binding,
    clippy::unnecessary_wraps,
    clippy::struct_excessive_bools,
    clippy::single_match_else,
    clippy::needless_continue,
    clippy::manual_let_else,
    clippy::similar_names,
    clippy::cast_lossless,
    clippy::implicit_clone,
    clippy::implicit_hasher,
    clippy::fn_params_excessive_bools,
    clippy::default_trait_access,
    clippy::case_sensitive_file_extension_comparisons,
    clippy::assigning_clones,
    clippy::needless_raw_string_hashes,
    reason = "CLI crate: pedantic lints suppressed crate-wide; numeric casts bounds-checked"
)]

//! `ToadStool` CLI - Universal Compute Command Center
//!
//! The gateway to SOVEREIGN SCIENCE and universal compute capabilities.
//! Commands for managing biome.yaml manifests and orchestrating distributed workloads.

mod biome_model;
mod cli_root;
mod error;

pub mod commands;

pub use biome_model::{
    BiomeInfo, BiomeManifest, BiomeMetadata, BiomeNetworking, BiomeResources, BiomeSecurity,
    BiomeStatus, BiomeStorage, DatasetConfig, HealthCheckConfig, PortMapping, PrimalConfig,
    ResourceUsage, ServiceConfig, ServiceInfo, ServicePort, ServiceResources, ServiceVolume,
    VolumeConfig, WorkloadSource,
};
pub use cli_root::{Cli, CliContext, load_biome_manifest, validate_manifest};
pub use commands::{
    Commands, EcosystemCommands, ModeCommand, TransportCommands, UniversalCommands,
};
pub use error::{CliContextExt, CliError, Result};

pub mod daemon;
pub mod ecosystem;
pub mod executor;
#[cfg(feature = "cli-monitoring")]
pub mod monitoring;
pub mod network_config;
pub mod setup;
pub mod templates;
pub mod universal;
pub mod utils;
pub mod zero_config;
