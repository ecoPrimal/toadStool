// SPDX-License-Identifier: AGPL-3.0-or-later
//! Tests for [`crate::ecosystem::helpers::get_capability_endpoint`] and
//! [`crate::ecosystem::helpers::assemble_discovered_services`].

use std::collections::HashMap;

use temp_env::{with_var, with_var_unset, with_vars};

use super::sample_service_info;
use crate::ToadStoolError;
use crate::ecosystem::helpers::{assemble_discovered_services, get_capability_endpoint};

#[test]
fn test_get_capability_endpoint_prefers_capability_key() {
    with_vars(
        vec![
            ("DISCOVERY_ENDPOINT", Some("http://cap.example:1")),
            ("COORDINATION_ENDPOINT", Some("http://legacy.example:2")),
        ],
        || {
            let ep = get_capability_endpoint("discovery", &["COORDINATION"]).expect("endpoint");
            assert_eq!(ep, "http://cap.example:1");
        },
    );
}

#[test]
fn test_get_capability_endpoint_legacy_coordination_when_discovery_unset() {
    with_var_unset("DISCOVERY_ENDPOINT", || {
        with_var(
            "COORDINATION_ENDPOINT",
            Some("http://coord.example:3"),
            || {
                let ep = get_capability_endpoint("discovery", &["COORDINATION"]).expect("endpoint");
                assert_eq!(ep, "http://coord.example:3");
            },
        );
    });
}

#[test]
fn test_get_capability_endpoint_crypto_legacy_pki_second() {
    with_var_unset("CRYPTO_ENDPOINT", || {
        with_var("PKI_ENDPOINT", Some("http://pki.example:4"), || {
            let ep = get_capability_endpoint("crypto", &["CRYPTO", "PKI"]).expect("endpoint");
            assert_eq!(ep, "http://pki.example:4");
        });
    });
}

#[test]
fn test_get_capability_endpoint_storage_legacy_artifact() {
    with_var_unset("STORAGE_ENDPOINT", || {
        with_var(
            "ARTIFACT_ENDPOINT",
            Some("http://artifact.example:5"),
            || {
                let ep = get_capability_endpoint("storage", &["STORAGE", "ARTIFACT"]).expect("ep");
                assert_eq!(ep, "http://artifact.example:5");
            },
        );
    });
}

#[test]
fn test_get_capability_endpoint_returns_none_when_missing() {
    with_var_unset("DISCOVERY_ENDPOINT", || {
        with_var_unset("COORDINATION_ENDPOINT", || {
            assert!(get_capability_endpoint("discovery", &["COORDINATION"]).is_none());
        });
    });
}

#[test]
fn test_assemble_discovered_services_all_ok_merges() {
    let mut local = HashMap::new();
    local.insert("l1".to_string(), sample_service_info("a", "http://a"));
    let mut net = HashMap::new();
    net.insert("n1".to_string(), sample_service_info("b", "http://b"));
    let assembled =
        assemble_discovered_services(Ok(local), Ok(net), Ok(HashMap::new()), Ok(HashMap::new()));
    assert_eq!(assembled.discovered_services.len(), 2);
    assert_eq!(assembled.discovery_summary.total_services_found, 2);
    assert!(
        assembled
            .discovery_summary
            .discovery_methods_used
            .contains(&"local".to_string())
    );
}

#[test]
fn test_assemble_discovered_services_skips_err_sources() {
    let mut local = HashMap::new();
    local.insert(
        "only".to_string(),
        sample_service_info("only", "http://only"),
    );
    let err = Err(ToadStoolError::network("network failed"));
    let assembled = assemble_discovered_services(
        Ok(local),
        err,
        Err(ToadStoolError::network("w")),
        Ok(HashMap::new()),
    );
    assert_eq!(assembled.discovered_services.len(), 1);
    assert_eq!(assembled.discovery_summary.total_services_found, 1);
}

#[test]
fn test_assemble_discovered_services_all_err_yields_empty() {
    let e = || Err(ToadStoolError::network("e"));
    let assembled = assemble_discovered_services(e(), e(), e(), e());
    assert!(assembled.discovered_services.is_empty());
    assert_eq!(assembled.discovery_summary.total_services_found, 0);
}

#[test]
fn test_assemble_discovered_services_later_source_overwrites_duplicate_key() {
    let mut first = HashMap::new();
    first.insert(
        "key".to_string(),
        sample_service_info("first", "http://first"),
    );
    let mut second = HashMap::new();
    second.insert(
        "key".to_string(),
        sample_service_info("second", "http://second"),
    );
    let assembled = assemble_discovered_services(
        Ok(first),
        Ok(second),
        Ok(HashMap::new()),
        Ok(HashMap::new()),
    );
    assert_eq!(
        assembled
            .discovered_services
            .get("key")
            .expect("key")
            .endpoints.first().map(String::as_str).unwrap_or(""),
        "http://second"
    );
}

#[test]
fn test_assemble_discovered_services_includes_mdns_when_ok() {
    let mut mdns = HashMap::new();
    mdns.insert("md".to_string(), sample_service_info("md", "http://md"));
    let assembled = assemble_discovered_services(
        Ok(HashMap::new()),
        Ok(HashMap::new()),
        Ok(HashMap::new()),
        Ok(mdns),
    );
    assert_eq!(assembled.discovered_services.len(), 1);
}
