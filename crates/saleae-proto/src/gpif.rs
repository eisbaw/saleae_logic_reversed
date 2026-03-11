//! GPIF state machine configuration for Saleae devices.
//!
//! The Cypress FX3 GPIF (General Programmable Interface) handles the physical
//! data transfer between the FPGA and the USB bus. Different capture
//! configurations require different GPIF state machines optimized for the
//! data width and USB speed.
//!
//! The GPIF state index is sent to the FX3 firmware during capture
//! initialization. The FX3 loads the corresponding state machine from its
//! internal table.
//!
//! Source: `_INIT_55` at ELF VA 0x1dd9630 in `libgraph_server_shared.so`
//! (27-entry lookup table). See `re/notes/usb2-fallback.md` for the full
//! table.

use crate::device::{DeviceType, UsbSpeed};

// ---------------------------------------------------------------------------
// Bus width enum
// ---------------------------------------------------------------------------

/// Logical bus width for the GPIF data path.
///
/// This is the logical data width between the FPGA and FX3, not the
/// physical bus width (which is always 32-bit on FX3). The width
/// determines how data is packed into GPIF transfers:
///
/// - `Narrow` (4-bit): Digital-only, few channels. Most bandwidth-limited.
/// - `Medium` (8-bit): Digital with more channels, USB 3.0 only for basic devices.
/// - `Wide` (14-bit): Analog data path. Speed-independent.
///
/// See `re/notes/gpif-bus-width.md` for details on logical vs physical width.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GpifBusWidth {
    /// 4-bit narrow: digital-only, few channels.
    Narrow,
    /// 8-bit medium: digital, more channels (USB 3.0 on basic devices).
    Medium,
    /// 14-bit wide: analog data path.
    Wide,
}

impl std::fmt::Display for GpifBusWidth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GpifBusWidth::Narrow => f.write_str("4-bit (narrow)"),
            GpifBusWidth::Medium => f.write_str("8-bit (medium)"),
            GpifBusWidth::Wide => f.write_str("14-bit (wide/analog)"),
        }
    }
}

// ---------------------------------------------------------------------------
// GPIF configuration
// ---------------------------------------------------------------------------

/// GPIF state machine configuration.
///
/// Selects which FX3 GPIF state machine to use for a given capture
/// configuration. The state index is sent to the firmware during
/// initialization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GpifConfig {
    /// GPIF state machine index (0-3).
    pub gpif_index: u8,
    /// Human-readable description of this state machine mode.
    pub description: &'static str,
}

// ---------------------------------------------------------------------------
// GPIF state machine descriptions
// ---------------------------------------------------------------------------

/// State 0: USB 2.0, narrow bus, basic devices.
///
/// Most bandwidth-limited mode. Used for USB 2.0 high-speed with a narrow
/// digital data path on legacy/basic devices (Logic, Logic16, Logic8).
const GPIF_STATE_0: GpifConfig = GpifConfig {
    gpif_index: 0,
    description: "USB 2.0 narrow (4-bit, basic devices)",
};

/// State 1: Logic16 only, USB 3.0, narrow bus.
///
/// Used only by Logic 16 at USB 3.0 with 4-bit bus. Specific optimization
/// for the legacy Logic 16 device.
const GPIF_STATE_1: GpifConfig = GpifConfig {
    gpif_index: 1,
    description: "USB 3.0 narrow (4-bit, Logic16 only)",
};

/// State 2: Standard USB 3.0 or wider bus.
///
/// Default mode for USB 3.0 connections or medium bus width. Also used
/// by Pro devices for narrow bus at any speed.
const GPIF_STATE_2: GpifConfig = GpifConfig {
    gpif_index: 2,
    description: "USB 3.0 standard / medium bus",
};

/// State 3: Wide bus (analog data path).
///
/// Used for 14-bit wide bus (analog channels active). Speed-independent.
const GPIF_STATE_3: GpifConfig = GpifConfig {
    gpif_index: 3,
    description: "wide bus (14-bit, analog)",
};

// ---------------------------------------------------------------------------
// Selection function
// ---------------------------------------------------------------------------

/// Select the GPIF state machine configuration for a capture.
///
/// Implements the 27-entry lookup table from `_INIT_55` (ELF VA 0x1dd9630)
/// in `libgraph_server_shared.so`.
///
/// # Arguments
///
/// - `device_type`: The device type being configured.
/// - `bus_width`: The logical bus width for this capture configuration.
/// - `usb_speed`: The negotiated USB connection speed.
///
/// # Returns
///
/// The GPIF configuration to use. Returns `None` for unsupported
/// combinations (e.g., Logic 4 at 8-bit, which is not in the table).
///
/// # Table
///
/// From `re/notes/usb2-fallback.md`:
///
/// | Device    | Bus Width | USB Speed | GPIF State |
/// |-----------|-----------|-----------|------------|
/// | Logic     | Narrow    | USB 2.0   | 0          |
/// | Logic     | Narrow    | USB 3.0   | 2          |
/// | Logic     | Medium    | USB 3.0   | 2          |
/// | Logic     | Wide      | Either    | 3          |
/// | Logic16   | Narrow    | USB 2.0   | 0          |
/// | Logic16   | Narrow    | USB 3.0   | 1          |
/// | Logic16   | Medium    | USB 3.0   | 2          |
/// | Logic16   | Wide      | Either    | 3          |
/// | Logic4    | Narrow    | Either    | 2          |
/// | Logic4    | Wide      | Either    | 3          |
/// | Logic8    | Narrow    | USB 2.0   | 0          |
/// | Logic8    | Narrow    | USB 3.0   | 2          |
/// | Logic8    | Medium    | USB 3.0   | 2          |
/// | Logic8    | Wide      | Either    | 3          |
/// | Pro 8/16  | Narrow    | Either    | 2          |
/// | Pro 8/16  | Wide      | Either    | 3          |
pub fn select_gpif_config(
    device_type: DeviceType,
    bus_width: GpifBusWidth,
    usb_speed: UsbSpeed,
) -> Option<GpifConfig> {
    let is_usb3 = matches!(usb_speed, UsbSpeed::Super | UsbSpeed::SuperPlus);

    match (device_type, bus_width) {
        // Wide bus (analog): always state 3, all devices, all speeds.
        (_, GpifBusWidth::Wide) => Some(GPIF_STATE_3),

        // Pro 8/16: Narrow -> state 2 (always), Medium not in table.
        (DeviceType::LogicPro8 | DeviceType::LogicPro16, GpifBusWidth::Narrow) => {
            Some(GPIF_STATE_2)
        }
        (DeviceType::LogicPro8 | DeviceType::LogicPro16, GpifBusWidth::Medium) => None,

        // Logic 4: Narrow -> state 2 (always), Medium not in table.
        (DeviceType::Logic4, GpifBusWidth::Narrow) => Some(GPIF_STATE_2),
        (DeviceType::Logic4, GpifBusWidth::Medium) => None,

        // Logic 16: special case -- Narrow USB 3.0 uses state 1.
        (DeviceType::Logic16, GpifBusWidth::Narrow) => {
            if is_usb3 {
                Some(GPIF_STATE_1)
            } else {
                Some(GPIF_STATE_0)
            }
        }
        (DeviceType::Logic16, GpifBusWidth::Medium) => {
            if is_usb3 {
                Some(GPIF_STATE_2)
            } else {
                None // Not in the table for USB 2.0
            }
        }

        // Logic Original / Logic 8: Narrow depends on speed.
        (DeviceType::LogicOriginal | DeviceType::Logic8, GpifBusWidth::Narrow) => {
            if is_usb3 {
                Some(GPIF_STATE_2)
            } else {
                Some(GPIF_STATE_0)
            }
        }
        (DeviceType::LogicOriginal | DeviceType::Logic8, GpifBusWidth::Medium) => {
            if is_usb3 {
                Some(GPIF_STATE_2)
            } else {
                None // Not in the table for USB 2.0
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Wide bus (analog) -- always state 3
    // -----------------------------------------------------------------------

    #[test]
    fn wide_bus_always_state_3() {
        for &dt in DeviceType::ALL {
            for &speed in &[UsbSpeed::High, UsbSpeed::Super, UsbSpeed::SuperPlus] {
                let config = select_gpif_config(dt, GpifBusWidth::Wide, speed);
                assert_eq!(
                    config.map(|c| c.gpif_index),
                    Some(3),
                    "wide bus should always be state 3 for {dt} at {speed}"
                );
            }
        }
    }

    // -----------------------------------------------------------------------
    // Pro devices -- narrow always state 2
    // -----------------------------------------------------------------------

    #[test]
    fn pro_narrow_always_state_2() {
        for &dt in &[DeviceType::LogicPro8, DeviceType::LogicPro16] {
            for &speed in &[UsbSpeed::High, UsbSpeed::Super] {
                let config = select_gpif_config(dt, GpifBusWidth::Narrow, speed);
                assert_eq!(
                    config.map(|c| c.gpif_index),
                    Some(2),
                    "Pro {dt} narrow should always be state 2"
                );
            }
        }
    }

    #[test]
    fn pro_medium_not_supported() {
        for &dt in &[DeviceType::LogicPro8, DeviceType::LogicPro16] {
            let config = select_gpif_config(dt, GpifBusWidth::Medium, UsbSpeed::Super);
            assert!(config.is_none(), "Pro {dt} medium should not be in table");
        }
    }

    // -----------------------------------------------------------------------
    // Logic 4 -- narrow always state 2
    // -----------------------------------------------------------------------

    #[test]
    fn logic4_narrow_always_state_2() {
        let config = select_gpif_config(DeviceType::Logic4, GpifBusWidth::Narrow, UsbSpeed::High);
        assert_eq!(config.map(|c| c.gpif_index), Some(2));

        let config = select_gpif_config(DeviceType::Logic4, GpifBusWidth::Narrow, UsbSpeed::Super);
        assert_eq!(config.map(|c| c.gpif_index), Some(2));
    }

    #[test]
    fn logic4_medium_not_supported() {
        let config = select_gpif_config(DeviceType::Logic4, GpifBusWidth::Medium, UsbSpeed::High);
        assert!(config.is_none());
    }

    // -----------------------------------------------------------------------
    // Logic 8 -- narrow depends on speed
    // -----------------------------------------------------------------------

    #[test]
    fn logic8_narrow_usb2_state_0() {
        let config = select_gpif_config(DeviceType::Logic8, GpifBusWidth::Narrow, UsbSpeed::High);
        assert_eq!(config.map(|c| c.gpif_index), Some(0));
    }

    #[test]
    fn logic8_narrow_usb3_state_2() {
        let config = select_gpif_config(DeviceType::Logic8, GpifBusWidth::Narrow, UsbSpeed::Super);
        assert_eq!(config.map(|c| c.gpif_index), Some(2));
    }

    #[test]
    fn logic8_medium_usb3_state_2() {
        let config = select_gpif_config(DeviceType::Logic8, GpifBusWidth::Medium, UsbSpeed::Super);
        assert_eq!(config.map(|c| c.gpif_index), Some(2));
    }

    #[test]
    fn logic8_medium_usb2_not_supported() {
        let config = select_gpif_config(DeviceType::Logic8, GpifBusWidth::Medium, UsbSpeed::High);
        assert!(config.is_none());
    }

    // -----------------------------------------------------------------------
    // Logic 16 -- special state 1 for narrow USB 3.0
    // -----------------------------------------------------------------------

    #[test]
    fn logic16_narrow_usb2_state_0() {
        let config = select_gpif_config(DeviceType::Logic16, GpifBusWidth::Narrow, UsbSpeed::High);
        assert_eq!(config.map(|c| c.gpif_index), Some(0));
    }

    #[test]
    fn logic16_narrow_usb3_state_1() {
        let config = select_gpif_config(DeviceType::Logic16, GpifBusWidth::Narrow, UsbSpeed::Super);
        assert_eq!(
            config.map(|c| c.gpif_index),
            Some(1),
            "Logic16 narrow USB 3.0 should use state 1 (unique)"
        );
    }

    #[test]
    fn logic16_medium_usb3_state_2() {
        let config = select_gpif_config(DeviceType::Logic16, GpifBusWidth::Medium, UsbSpeed::Super);
        assert_eq!(config.map(|c| c.gpif_index), Some(2));
    }

    // -----------------------------------------------------------------------
    // Logic Original -- same pattern as Logic 8
    // -----------------------------------------------------------------------

    #[test]
    fn logic_original_narrow_usb2_state_0() {
        let config = select_gpif_config(
            DeviceType::LogicOriginal,
            GpifBusWidth::Narrow,
            UsbSpeed::High,
        );
        assert_eq!(config.map(|c| c.gpif_index), Some(0));
    }

    #[test]
    fn logic_original_narrow_usb3_state_2() {
        let config = select_gpif_config(
            DeviceType::LogicOriginal,
            GpifBusWidth::Narrow,
            UsbSpeed::Super,
        );
        assert_eq!(config.map(|c| c.gpif_index), Some(2));
    }

    // -----------------------------------------------------------------------
    // GpifConfig struct
    // -----------------------------------------------------------------------

    #[test]
    fn gpif_config_display() {
        assert!(GpifBusWidth::Narrow.to_string().contains("4-bit"));
        assert!(GpifBusWidth::Medium.to_string().contains("8-bit"));
        assert!(GpifBusWidth::Wide.to_string().contains("14-bit"));
    }

    #[test]
    fn gpif_state_indices_in_range() {
        // All returned states should be 0-3.
        for &dt in DeviceType::ALL {
            for &bw in &[
                GpifBusWidth::Narrow,
                GpifBusWidth::Medium,
                GpifBusWidth::Wide,
            ] {
                for &speed in &[UsbSpeed::High, UsbSpeed::Super] {
                    if let Some(config) = select_gpif_config(dt, bw, speed) {
                        assert!(
                            config.gpif_index <= 3,
                            "GPIF index out of range: {} for {dt} {bw} {speed}",
                            config.gpif_index
                        );
                    }
                }
            }
        }
    }
}
