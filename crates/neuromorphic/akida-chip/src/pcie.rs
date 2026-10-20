// SPDX-License-Identifier: AGPL-3.0-or-later

//! `PCIe` identifiers and timing constants.
//!
//! Source: AKD1000 hardware + AKD1500 datasheet v1.2 (June 2025).
//!
//! ## Akida chip family (Oct 2026)
//!
//! | Chip | Generation | Process | Status | Interface |
//! |------|-----------|---------|--------|-----------|
//! | AKD1000 | Akida 1.0 | — | Production | PCIe x1 Gen2 |
//! | AKD1500 | Akida 1.0 | — | Production | PCIe x2 Gen2 / SPI / GPIO |
//! | AKD2500 | Akida 2.0 | TSMC 12nm | MPW tape-out Dec 2026 | TBD |
//! | Akida Pico | Akida Pico | 22nm | IP available | AXI-4 / AHB / SPI / GPIO |

/// `BrainChip` vendor ID (PCI-SIG assigned).
pub const BRAINCHIP_VENDOR_ID: u16 = 0x1E7C;

/// Device IDs for Akida family.
pub mod device_id {
    /// AKD1000 — `PCIe` x1 Gen2 reference board (`lspci: 1e7c:bca1`).
    pub const AKD1000: u16 = 0xBCA1;
    /// AKD1500 — `PCIe` x2 Gen2, 7×7 mm BGA169 (`lspci: 1e7c:a500`).
    pub const AKD1500: u16 = 0xA500;
    /// AKD1500 alternate ID (some revisions).
    pub const AKD1500_ALT: u16 = 0xBCA2;
    /// AKD2500 — Akida 2.0, TSMC 12nm. Device ID placeholder (pre-silicon, Oct 2026).
    ///
    /// BrainChip announced AKD2500 MPW in Feb 2026, tape-out scheduled Dec 2026.
    /// No public device ID yet — this placeholder will be updated when silicon ships.
    /// Source: ASX announcement 2026-02-13, Q2 2026 investor update.
    pub const AKD2500_PLACEHOLDER: u16 = 0xA250;
}

/// All known Akida device IDs (excluding pre-silicon placeholders).
pub const ALL_DEVICE_IDS: &[u16] = &[
    device_id::AKD1000,
    device_id::AKD1500,
    device_id::AKD1500_ALT,
];

/// All Akida device IDs including pre-silicon placeholders.
pub const ALL_DEVICE_IDS_WITH_FUTURE: &[u16] = &[
    device_id::AKD1000,
    device_id::AKD1500,
    device_id::AKD1500_ALT,
    device_id::AKD2500_PLACEHOLDER,
];

/// Measured `PCIe` Gen2 x1 round-trip latency (µs).
///
/// Minimum achievable latency for a write + read cycle over `PCIe` x1 Gen2.
/// This is hardware-limited — no software optimisation can beat it.
/// Source: `BEYOND_SDK.md` Discovery 3.
pub const PCIE_GEN2_X1_ROUNDTRIP_US: u64 = 650;

/// Optimal batch size for `PCIe` amortisation (Discovery 3).
///
/// `batch=8` gives 2.4× throughput over `batch=1` by spreading the
/// ~650 µs `PCIe` round-trip cost across 8 inference samples.
pub const OPTIMAL_BATCH_SIZE: usize = 8;

/// Sustained DMA throughput measured on AKD1000 (MB/s).
///
/// Source: wetSpring Exp 194, hotSpring Exp 022 (Feb 2026; ecosystem context — not a runtime dependency).
pub const MEASURED_DMA_THROUGHPUT_MB_S: u32 = 37;

/// Format a `vendor:device` string for use with `lspci -d`.
#[must_use]
pub fn lspci_filter() -> String {
    format!("{:04x}:{:04x}", BRAINCHIP_VENDOR_ID, device_id::AKD1000)
}

/// Chip variant discovered at runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChipVariant {
    /// AKD1000 — `PCIe` x1 Gen2, 80 NPs, 8 MB SRAM.
    Akd1000,
    /// AKD1500 — `PCIe` x2 Gen2, BGA169, GPIO, SPI, SLEEP pin.
    Akd1500,
    /// AKD2500 — Akida 2.0, TSMC 12nm. Pre-silicon (MPW tape-out Dec 2026).
    ///
    /// Announced Feb 2026. Same IP as Akida 2: 1–128 nodes, 128 MACs/node,
    /// 8/4/1-bit arithmetic, AXI bus, CNN+TENN processing.
    /// Specs are placeholders — update when silicon ships.
    Akd2500,
    /// Akida Pico — ultra-low-power standalone NPU core (µW active).
    ///
    /// 0.145 mm² at 22nm, 1–500 MHz, single Neural Processing Engine,
    /// AXI-4 bus matrix, optional AHB, SPI, GPIO.
    /// Designed for always-on sensing: keyword spotting, vital signs, anomaly detection.
    /// NOT a PCIe device — uses AXI/SPI/AHB interfaces directly.
    Pico,
    /// Unknown / future variant.
    Unknown(u16),
}

/// Akida platform generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AkidaGeneration {
    /// Akida 1.0 — AKD1000, AKD1500. Event-based SNN + CNN processing.
    V1,
    /// Akida 2.0 — AKD2500. CNN + TENN, 8/4/1-bit, skip connections in hardware.
    V2,
    /// Akida Pico — ultra-low-power single-NPE core. TENN-focused.
    Pico,
}

impl ChipVariant {
    /// Identify variant from PCI device ID.
    #[must_use]
    pub const fn from_device_id(id: u16) -> Self {
        match id {
            device_id::AKD1000 => Self::Akd1000,
            device_id::AKD1500 | device_id::AKD1500_ALT => Self::Akd1500,
            device_id::AKD2500_PLACEHOLDER => Self::Akd2500,
            other => Self::Unknown(other),
        }
    }

    /// Platform generation.
    #[must_use]
    pub const fn generation(&self) -> AkidaGeneration {
        match self {
            Self::Akd1000 | Self::Akd1500 => AkidaGeneration::V1,
            Self::Akd2500 => AkidaGeneration::V2,
            Self::Pico => AkidaGeneration::Pico,
            Self::Unknown(_) => AkidaGeneration::V1, // conservative default
        }
    }

    /// Typical NP count for this variant.
    #[must_use]
    pub const fn np_count(&self) -> u32 {
        match self {
            Self::Akd1000 | Self::Akd1500 => 80,
            // Akida 2 IP supports 1–128 nodes. AKD2500 config TBD.
            Self::Akd2500 => 128, // placeholder: max supported by Akida 2 IP
            Self::Pico => 1, // single Neural Processing Engine
            Self::Unknown(_) => 0,
        }
    }

    /// On-chip SRAM in megabytes.
    #[must_use]
    pub const fn sram_mb(&self) -> u32 {
        match self {
            Self::Akd1000 => 8,
            Self::Akd1500 => 10,
            // Akida 2 IP: 50–130K local SRAM per node. 128 nodes × 130K = ~16 MB max.
            Self::Akd2500 => 16, // placeholder: upper range
            Self::Pico => 0, // 74K word local RAM (< 1 MB; rounds to 0 in MB)
            Self::Unknown(_) => 0,
        }
    }

    /// Pico local RAM in kilobytes (only meaningful for Pico).
    #[must_use]
    pub const fn local_ram_kb(&self) -> u32 {
        match self {
            Self::Pico => 296, // 74K words × 4 bytes = 296 KB
            _ => self.sram_mb() * 1024,
        }
    }

    /// `PCIe` lane count. 0 for non-PCIe devices (Pico).
    #[must_use]
    pub const fn pcie_lanes(&self) -> u8 {
        match self {
            Self::Akd1500 => 2,
            Self::Akd2500 => 2, // placeholder: likely x2 or x4 Gen3
            Self::Pico => 0, // not a PCIe device
            Self::Akd1000 | Self::Unknown(_) => 1,
        }
    }

    /// Whether this is a PCIe-attached device.
    #[must_use]
    pub const fn is_pcie(&self) -> bool {
        !matches!(self, Self::Pico)
    }

    /// Whether silicon exists (vs pre-silicon placeholder).
    #[must_use]
    pub const fn is_production(&self) -> bool {
        matches!(self, Self::Akd1000 | Self::Akd1500)
    }

    /// Supported weight bit-widths.
    #[must_use]
    pub const fn weight_bits(&self) -> &'static [u8] {
        match self {
            Self::Akd1000 | Self::Akd1500 => &[1, 2, 4],
            Self::Akd2500 => &[1, 4, 8], // Akida 2 adds 8-bit
            Self::Pico => &[4, 8], // Pico: 4-bit and 8-bit
            Self::Unknown(_) => &[],
        }
    }

    /// Maximum clock frequency in MHz (0 = unknown).
    #[must_use]
    pub const fn max_clock_mhz(&self) -> u32 {
        match self {
            Self::Akd1000 => 300,
            Self::Akd1500 => 300,
            Self::Akd2500 => 1000, // Akida 2 IP spec: up to 1 GHz
            Self::Pico => 500, // product brief: 1–500 MHz
            Self::Unknown(_) => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::device_id;
    use super::*;

    #[test]
    fn brainchip_vendor_id_matches_pci_sig() {
        assert_eq!(BRAINCHIP_VENDOR_ID, 0x1E7C);
    }

    #[test]
    fn all_device_ids_cover_known_chips() {
        assert!(ALL_DEVICE_IDS.contains(&device_id::AKD1000));
        assert!(ALL_DEVICE_IDS.contains(&device_id::AKD1500));
        assert!(ALL_DEVICE_IDS.contains(&device_id::AKD1500_ALT));
        assert_eq!(ALL_DEVICE_IDS.len(), 3);
    }

    #[test]
    fn future_ids_include_akd2500() {
        assert!(ALL_DEVICE_IDS_WITH_FUTURE.contains(&device_id::AKD2500_PLACEHOLDER));
        assert_eq!(ALL_DEVICE_IDS_WITH_FUTURE.len(), 4);
    }

    #[test]
    fn lspci_filter_formats_vendor_and_default_device() {
        assert_eq!(lspci_filter(), "1e7c:bca1");
    }

    #[test]
    fn chip_variant_from_device_id_maps_akd1000() {
        let v = ChipVariant::from_device_id(device_id::AKD1000);
        assert_eq!(v, ChipVariant::Akd1000);
        assert_eq!(v.np_count(), 80);
        assert_eq!(v.sram_mb(), 8);
        assert_eq!(v.pcie_lanes(), 1);
        assert!(v.is_pcie());
        assert!(v.is_production());
        assert_eq!(v.generation(), AkidaGeneration::V1);
    }

    #[test]
    fn chip_variant_from_device_id_maps_akd1500_and_alt() {
        for id in [device_id::AKD1500, device_id::AKD1500_ALT] {
            let v = ChipVariant::from_device_id(id);
            assert_eq!(v, ChipVariant::Akd1500);
            assert_eq!(v.np_count(), 80);
            assert_eq!(v.sram_mb(), 10);
            assert_eq!(v.pcie_lanes(), 2);
            assert!(v.is_production());
        }
    }

    #[test]
    fn akd2500_pre_silicon_placeholder() {
        let v = ChipVariant::from_device_id(device_id::AKD2500_PLACEHOLDER);
        assert_eq!(v, ChipVariant::Akd2500);
        assert_eq!(v.generation(), AkidaGeneration::V2);
        assert_eq!(v.np_count(), 128);
        assert_eq!(v.sram_mb(), 16);
        assert!(!v.is_production());
        assert!(v.is_pcie());
        assert!(v.weight_bits().contains(&8)); // Akida 2 adds 8-bit
        assert_eq!(v.max_clock_mhz(), 1000);
    }

    #[test]
    fn akida_pico_is_not_pcie() {
        let v = ChipVariant::Pico;
        assert!(!v.is_pcie());
        assert_eq!(v.pcie_lanes(), 0);
        assert_eq!(v.generation(), AkidaGeneration::Pico);
        assert_eq!(v.np_count(), 1);
        assert_eq!(v.local_ram_kb(), 296); // 74K words × 4 bytes
        assert_eq!(v.max_clock_mhz(), 500);
        assert!(v.weight_bits().contains(&4));
        assert!(v.weight_bits().contains(&8));
    }

    #[test]
    fn chip_variant_unknown_preserves_id_and_reports_zero_sram() {
        let v = ChipVariant::from_device_id(0xFFFF);
        assert_eq!(v, ChipVariant::Unknown(0xFFFF));
        assert_eq!(v.np_count(), 0);
        assert_eq!(v.sram_mb(), 0);
        assert_eq!(v.pcie_lanes(), 1);
        assert!(!v.is_production());
    }

    #[test]
    fn pcie_timing_constants_are_positive() {
        assert!(PCIE_GEN2_X1_ROUNDTRIP_US > 0);
        assert_eq!(OPTIMAL_BATCH_SIZE, 8);
        assert!(MEASURED_DMA_THROUGHPUT_MB_S > 0);
    }

    #[test]
    fn local_ram_kb_converts_sram_mb_for_pcie_chips() {
        assert_eq!(ChipVariant::Akd1000.local_ram_kb(), 8 * 1024);
        assert_eq!(ChipVariant::Akd1500.local_ram_kb(), 10 * 1024);
        assert_eq!(ChipVariant::Akd2500.local_ram_kb(), 16 * 1024);
    }

    #[test]
    fn weight_bits_evolution_across_generations() {
        // Akida 1: 1, 2, 4-bit
        assert_eq!(ChipVariant::Akd1000.weight_bits(), &[1, 2, 4]);
        // Akida 2: 1, 4, 8-bit (drops 2-bit, adds 8-bit)
        assert_eq!(ChipVariant::Akd2500.weight_bits(), &[1, 4, 8]);
        // Pico: 4, 8-bit only
        assert_eq!(ChipVariant::Pico.weight_bits(), &[4, 8]);
    }
}
