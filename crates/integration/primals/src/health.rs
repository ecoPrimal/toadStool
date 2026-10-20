// SPDX-License-Identifier: AGPL-3.0-or-later
//! Health check types.
//!
//! Local definitions replaced by canonical types from cellmembrane-types.
//! HealthStatus: was {healthy, checks, last_check} → canonical {healthy, message, uptime_secs, checks, details}.
//! HealthCheck: was {name, status, message, duration} → canonical {name, passed, message, latency_ms}.
//! The old `last_check` timestamp and `HealthCheckStatus` enum are no longer needed;
//! use `details` map for extra metadata.

pub use cellmembrane_types::{HealthCheck, HealthStatus};
