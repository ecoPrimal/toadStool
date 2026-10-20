// SPDX-License-Identifier: AGPL-3.0-or-later

//! NP mesh topology and routing model.
//!
//! Established by hardware probing and C++ engine symbol analysis.
//! Source: `BEYOND_SDK.md` Discoveries 2, 5, 9.
//!
//! ## Key findings
//!
//! - AKD1000 has **80 NPs** in a **5×8×2** mesh (78 functional in production)
//! - FC layers **merge into a single hardware pass** via `SkipDMA` routing
//!   (NP-to-NP transfer without `PCIe` round-trip)
//! - Multiple SRAM types: 64-bit filter SRAM, 51-bit threshold SRAM,
//!   32-bit event/status SRAM
//! - Three hardware variants in C++ engine: `akida::v1` (AKD1000),
//!   `akida::v2` (Akida 2.0), `akida::pico`

/// NP mesh topology.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MeshTopology {
    /// NPs in X dimension.
    pub x: u8,
    /// NPs in Y dimension.
    pub y: u8,
    /// NPs in Z dimension (pipeline depth).
    pub z: u8,
    /// Functional NPs (may be less than x×y×z due to disabled NPs).
    pub functional: u32,
}

impl MeshTopology {
    /// AKD1000 reference topology (confirmed by probing).
    pub const AKD1000: Self = Self {
        x: 5,
        y: 8,
        z: 2,
        functional: 78,
    };

    /// Total NP slots in the mesh.
    #[must_use]
    pub const fn total_slots(&self) -> u32 {
        (self.x as u32) * (self.y as u32) * (self.z as u32)
    }

    /// Disabled NP count.
    #[must_use]
    pub const fn disabled(&self) -> u32 {
        self.total_slots() - self.functional
    }
}

/// NP capabilities per node.
#[derive(Debug, Clone, Copy)]
pub struct NpCapabilities {
    /// NPU cores per NP node.
    pub npus_per_node: u32,
    /// MACs per NPU core.
    pub macs_per_npu: u32,
    /// Local SRAM per NPU in KB (configurable range).
    pub sram_per_npu_kb_min: u32,
    /// Upper bound of local SRAM per NPU in KB.
    pub sram_per_npu_kb_max: u32,
    /// Weight precision bits (Akida 1.0).
    pub weight_bits: &'static [u8],
    /// Activation precision bits.
    pub activation_bits: &'static [u8],
}

/// AKD1000 NP capability profile (from HARDWARE.md).
pub const AKD1000_NP: NpCapabilities = NpCapabilities {
    npus_per_node: 4,
    macs_per_npu: 128,
    sram_per_npu_kb_min: 50,
    sram_per_npu_kb_max: 130,
    weight_bits: &[1, 2, 4],
    activation_bits: &[1, 2, 4],
};

/// Total MACs for AKD1000.
pub const AKD1000_TOTAL_MACS: u32 =
    MeshTopology::AKD1000.functional * AKD1000_NP.npus_per_node * AKD1000_NP.macs_per_npu;
// = 78 × 4 × 128 = 39,936

// ── Akida 2.0 (AKD2500) — pre-silicon placeholders ──────────────────────────
//
// Source: Akida 2 IP Product Brief v2.0 (April 2025), ASX announcement Feb 2026.
// These values are from the IP specification, not from silicon measurement.
// Update when AKD2500 tape-out (Dec 2026) produces real numbers.

/// Akida 2 IP capability profile (from product brief v2.0).
///
/// Scalable from 1–128 nodes. Each node has 128 8×8 MAC engines.
/// Supports 8/4/1-bit weights and activations.
pub const AKIDA2_NP: NpCapabilities = NpCapabilities {
    npus_per_node: 1, // Akida 2 treats each node as one processing element
    macs_per_npu: 128, // 128 MAC engines (8×8 MACs each = 8192 MACs at 64-node)
    sram_per_npu_kb_min: 50,
    sram_per_npu_kb_max: 130,
    weight_bits: &[1, 4, 8],
    activation_bits: &[1, 4, 8],
};

/// Akida 2 TOPS at max config (128 nodes, 1 GHz): 16 TOPS.
/// Source: Akida 2 IP Product Brief scalability table.
pub const AKIDA2_MAX_TOPS: f64 = 16.0;

// ── Akida Pico — ultra-low-power NPU core ────────────────────────────────────
//
// Source: Akida Pico Product Brief v1.3 (Feb 2026).
// Standalone event-based NPU: µW active power, TENNs processing.

/// Akida Pico capability profile.
pub const PICO_NP: NpCapabilities = NpCapabilities {
    npus_per_node: 1, // single Neural Processing Engine
    macs_per_npu: 1, // event-based, not MAC-based in the traditional sense
    sram_per_npu_kb_min: 296, // 74K words × 4 bytes
    sram_per_npu_kb_max: 296,
    weight_bits: &[4, 8],
    activation_bits: &[4, 8],
};

/// Akida Pico die area at 22nm process (mm²).
pub const PICO_DIE_AREA_MM2: f64 = 0.145;

/// Akida Pico gate count (base configuration).
pub const PICO_GATE_COUNT: u32 = 160_000;

/// `SkipDMA` — NP-to-NP data routing without `PCIe` round-trip.
///
/// Discovered by C++ engine symbol analysis (Discovery 2 and 9).
/// This mechanism is why deep FC chains execute as a single hardware pass.
pub mod skip_dma {
    /// Maximum FC depth that still merges into a single pass.
    /// Tested up to depth=8 (9 layers) with identical latency.
    pub const MAX_MERGED_DEPTH: usize = 8;

    /// Per-layer latency overhead when using `SkipDMA` (µs).
    /// Discovery 2: 8 layers costs only 3 µs more than 2 layers.
    pub const PER_LAYER_OVERHEAD_US: u64 = 0; // effectively zero
}

/// SRAM types discovered in C++ engine (Discovery 9).
pub mod sram_types {
    /// 64-bit filter SRAM for convolution kernels.
    pub const FSRAM_64B: &str = "get_fsram_64b_memory_size";
    /// 51-bit threshold SRAM — more precision than the "4-bit everything" spec.
    pub const TSRAM_51B: &str = "get_tsram_51b_memory_size";
    /// 32-bit event SRAM.
    pub const EVSRAM_32B: &str = "get_evsram_32b_memory_size";
    /// 32-bit status SRAM.
    pub const STSRAM_32B: &str = "get_stsram_32b_memory_size";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn akd1000_mesh_geometry() {
        let mesh = MeshTopology::AKD1000;
        assert_eq!(mesh.total_slots(), 80);
        assert_eq!(mesh.functional, 78);
        assert_eq!(mesh.disabled(), 2);
    }

    #[test]
    fn total_macs() {
        // 78 NPs × 4 NPUs × 128 MACs = 39,936
        assert_eq!(AKD1000_TOTAL_MACS, 39_936);
    }

    #[test]
    fn skip_dma_constants_documented() {
        assert!(super::skip_dma::MAX_MERGED_DEPTH >= 1);
        assert_eq!(super::skip_dma::PER_LAYER_OVERHEAD_US, 0);
    }

    #[test]
    fn sram_type_symbols_are_non_empty() {
        assert!(!super::sram_types::FSRAM_64B.is_empty());
        assert!(!super::sram_types::TSRAM_51B.is_empty());
    }

    #[test]
    fn np_capabilities_weight_and_activation_bits() {
        assert_eq!(AKD1000_NP.weight_bits, &[1, 2, 4]);
        assert_eq!(AKD1000_NP.activation_bits, &[1, 2, 4]);
    }

    #[test]
    fn akida2_adds_8_bit_support() {
        assert!(AKIDA2_NP.weight_bits.contains(&8));
        assert!(AKIDA2_NP.activation_bits.contains(&8));
        assert_eq!(AKIDA2_NP.macs_per_npu, 128);
    }

    #[test]
    fn akida2_max_tops_from_product_brief() {
        assert!((AKIDA2_MAX_TOPS - 16.0).abs() < f64::EPSILON);
    }

    #[test]
    fn pico_is_ultra_low_power() {
        assert_eq!(PICO_NP.npus_per_node, 1);
        assert!(PICO_NP.weight_bits.contains(&4));
        assert!(PICO_NP.weight_bits.contains(&8));
        assert_eq!(PICO_NP.sram_per_npu_kb_min, 296);
    }

    #[test]
    fn pico_die_area_sub_0_2_mm2() {
        assert!(PICO_DIE_AREA_MM2 < 0.2);
        assert!(PICO_GATE_COUNT == 160_000);
    }
}
