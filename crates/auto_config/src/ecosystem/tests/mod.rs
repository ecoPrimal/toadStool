// SPDX-License-Identifier: AGPL-3.0-or-later
//! Unit tests for ecosystem discovery helpers and [`super::EcosystemDiscoverer`].

use crate::ecosystem_types::ServiceInfo;

mod discoverer_behaviour;
mod env_and_merge;

pub(super) fn sample_service_info(name: &str, endpoint: &str) -> ServiceInfo {
    let mut si = ServiceInfo::new(name, name)
        .with_endpoint(endpoint)
        .with_version("1");
    si.metadata.insert("service_type".into(), "Compute".into());
    si.metadata.insert("discovered_via".into(), "test".into());
    si
}
