// SPDX-License-Identifier: AGPL-3.0-or-later

// Canonical SecurityLevel from cellmembrane-types.
// Local enum removed — was (Standard, Enhanced, HardwareSecured).
// Mapping: Standard→Standard, Enhanced→High, HardwareSecured→Maximum.
pub use cellmembrane_types::SecurityLevel;
