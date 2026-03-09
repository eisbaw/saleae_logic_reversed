//! Device identification constants and types.
//!
//! Constants here are derived from reverse engineering of Saleae Logic 2
//! v2.4.40's `libgraph_server_shared.so`. See `doc/protocol/fx3-firmware.md`
//! and `re/notes/device-class-hierarchy.md` for source analysis.

use crate::error::{ProtoError, ProtoResult};

// ---------------------------------------------------------------------------
// USB Vendor IDs
// ---------------------------------------------------------------------------

/// Saleae vendor ID (used by all current devices).
pub const VID_SALEAE: u16 = 0x21a9;

/// Legacy Saleae vendor ID (original Logic, FX2-based).
pub const VID_SALEAE_LEGACY: u16 = 0x0925;

// ---------------------------------------------------------------------------
// USB Product IDs
// ---------------------------------------------------------------------------

/// Original Logic (FX2-based).
pub const PID_LOGIC_ORIGINAL: u16 = 0x3881;

/// Logic 16 (FX2LP-based, legacy).
pub const PID_LOGIC_16: u16 = 0x1001;

/// Logic 4 (FX3-based).
pub const PID_LOGIC_4: u16 = 0x1003;

/// Logic 8 (FX3-based).
pub const PID_LOGIC_8: u16 = 0x1004;

/// Logic Pro 8 (FX3-based).
pub const PID_LOGIC_PRO_8: u16 = 0x1005;

/// Logic Pro 16 (FX3-based).
pub const PID_LOGIC_PRO_16: u16 = 0x1006;

// ---------------------------------------------------------------------------
// USB ID pair
// ---------------------------------------------------------------------------

/// A USB Vendor ID / Product ID pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UsbId {
    pub vid: u16,
    pub pid: u16,
}

impl UsbId {
    pub const fn new(vid: u16, pid: u16) -> Self {
        Self { vid, pid }
    }
}

impl std::fmt::Display for UsbId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:04x}:{:04x}", self.vid, self.pid)
    }
}

// ---------------------------------------------------------------------------
// Device type enum
// ---------------------------------------------------------------------------

/// Known Saleae device types.
///
/// The `type_id` values come from the device type table at 0x004718e4 in
/// `libgraph_server_shared.so`. See `doc/protocol/fx3-firmware.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeviceType {
    /// Original Logic (0925:3881, FX2). Type ID 1.
    LogicOriginal,
    /// Logic 16 (21a9:1001, FX2LP). Type ID 4.
    Logic16,
    /// Logic 4 (21a9:1003, FX3). Type ID 5.
    Logic4,
    /// Logic 8 (21a9:1004, FX3). Type ID 6.
    Logic8,
    /// Logic Pro 8 (21a9:1005, FX3). Type ID 7.
    LogicPro8,
    /// Logic Pro 16 (21a9:1006, FX3). Type ID 12.
    LogicPro16,
}

impl DeviceType {
    /// All known device types.
    pub const ALL: &[DeviceType] = &[
        DeviceType::LogicOriginal,
        DeviceType::Logic16,
        DeviceType::Logic4,
        DeviceType::Logic8,
        DeviceType::LogicPro8,
        DeviceType::LogicPro16,
    ];

    /// All VID:PID pairs that identify Saleae devices.
    pub const ALL_USB_IDS: &[UsbId] = &[
        UsbId::new(VID_SALEAE_LEGACY, PID_LOGIC_ORIGINAL),
        UsbId::new(VID_SALEAE, PID_LOGIC_16),
        UsbId::new(VID_SALEAE, PID_LOGIC_4),
        UsbId::new(VID_SALEAE, PID_LOGIC_8),
        UsbId::new(VID_SALEAE, PID_LOGIC_PRO_8),
        UsbId::new(VID_SALEAE, PID_LOGIC_PRO_16),
    ];

    /// Resolve a VID:PID pair to a device type.
    pub fn from_usb_id(id: UsbId) -> ProtoResult<Self> {
        match (id.vid, id.pid) {
            (VID_SALEAE_LEGACY, PID_LOGIC_ORIGINAL) => Ok(DeviceType::LogicOriginal),
            (VID_SALEAE, PID_LOGIC_16) => Ok(DeviceType::Logic16),
            (VID_SALEAE, PID_LOGIC_4) => Ok(DeviceType::Logic4),
            (VID_SALEAE, PID_LOGIC_8) => Ok(DeviceType::Logic8),
            (VID_SALEAE, PID_LOGIC_PRO_8) => Ok(DeviceType::LogicPro8),
            (VID_SALEAE, PID_LOGIC_PRO_16) => Ok(DeviceType::LogicPro16),
            _ => Err(ProtoError::UnknownDevice {
                vid: id.vid,
                pid: id.pid,
            }),
        }
    }

    /// Return the USB VID:PID for this device type.
    pub const fn usb_id(self) -> UsbId {
        match self {
            DeviceType::LogicOriginal => UsbId::new(VID_SALEAE_LEGACY, PID_LOGIC_ORIGINAL),
            DeviceType::Logic16 => UsbId::new(VID_SALEAE, PID_LOGIC_16),
            DeviceType::Logic4 => UsbId::new(VID_SALEAE, PID_LOGIC_4),
            DeviceType::Logic8 => UsbId::new(VID_SALEAE, PID_LOGIC_8),
            DeviceType::LogicPro8 => UsbId::new(VID_SALEAE, PID_LOGIC_PRO_8),
            DeviceType::LogicPro16 => UsbId::new(VID_SALEAE, PID_LOGIC_PRO_16),
        }
    }

    /// Internal type ID from the Saleae device type table.
    pub const fn type_id(self) -> u8 {
        match self {
            DeviceType::LogicOriginal => 1,
            DeviceType::Logic16 => 4,
            DeviceType::Logic4 => 5,
            DeviceType::Logic8 => 6,
            DeviceType::LogicPro8 => 7,
            DeviceType::LogicPro16 => 12,
        }
    }

    /// Human-readable device name (matches Saleae's `DeviceTypeToString()`).
    pub const fn name(self) -> &'static str {
        match self {
            DeviceType::LogicOriginal => "Logic",
            DeviceType::Logic16 => "Logic16",
            DeviceType::Logic4 => "Logic4",
            DeviceType::Logic8 => "Logic8",
            DeviceType::LogicPro8 => "LogicPro8",
            DeviceType::LogicPro16 => "LogicPro16",
        }
    }

    /// Whether this device uses a Cypress FX3 USB controller.
    pub const fn is_fx3(self) -> bool {
        matches!(
            self,
            DeviceType::Logic4
                | DeviceType::Logic8
                | DeviceType::LogicPro8
                | DeviceType::LogicPro16
        )
    }

    /// Whether this device uses a Cypress FX2/FX2LP USB controller.
    pub const fn is_fx2(self) -> bool {
        matches!(self, DeviceType::LogicOriginal | DeviceType::Logic16)
    }

    /// Whether this is a "Pro" device (has analog front-end with VGA).
    pub const fn is_pro(self) -> bool {
        matches!(self, DeviceType::LogicPro8 | DeviceType::LogicPro16)
    }
}

impl std::fmt::Display for DeviceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

// ---------------------------------------------------------------------------
// USB endpoint constants (post-firmware)
// ---------------------------------------------------------------------------

/// USB endpoint addresses after firmware upload.
///
/// Confirmed from live `lsusb -v` of Logic Pro 8 (21a9:1005) and the
/// firmware error string "Endpoint count is not 4".
pub mod endpoint {
    /// Control endpoint (EP0) -- vendor requests, FW upload, register I/O.
    pub const CONTROL: u8 = 0x00;

    /// Bulk OUT endpoint -- commands, config, FPGA bitstream data.
    pub const BULK_OUT: u8 = 0x01;

    /// Bulk IN endpoint -- responses, register reads.
    pub const BULK_IN: u8 = 0x81;

    /// Bulk IN endpoint -- capture data stream (high throughput).
    pub const CAPTURE_IN: u8 = 0x82;

    /// Expected number of endpoints after firmware load (including EP0).
    pub const EXPECTED_COUNT: usize = 4;
}

// ---------------------------------------------------------------------------
// Device state machine
// ---------------------------------------------------------------------------

/// Device states from the Saleae state machine.
///
/// See `doc/state-machine/device-states.md`. Confirmed from RTTI strings:
/// `DSInitializing`, `DSReady`, `DSError`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeviceState {
    /// Device is initializing (FW upload, FPGA program, ADC init, crypto).
    Initializing,
    /// Device is ready for capture.
    Ready,
    /// Device encountered an unrecoverable error (requires physical reconnect).
    Error,
}

impl std::fmt::Display for DeviceState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeviceState::Initializing => f.write_str("Initializing"),
            DeviceState::Ready => f.write_str("Ready"),
            DeviceState::Error => f.write_str("Error"),
        }
    }
}

// ---------------------------------------------------------------------------
// Discovered device info
// ---------------------------------------------------------------------------

/// Information about a discovered Saleae device on the USB bus.
#[derive(Debug, Clone)]
pub struct SaleaeDevice {
    /// USB vendor/product ID.
    pub usb_id: UsbId,
    /// Resolved device type.
    pub device_type: DeviceType,
    /// USB bus number.
    pub bus: u8,
    /// USB device address.
    pub address: u8,
    /// Serial number string (may be "SERNUM" for Saleae devices).
    pub serial: Option<String>,
}

impl std::fmt::Display for SaleaeDevice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} ({}) at bus {:03} addr {:03}",
            self.device_type, self.usb_id, self.bus, self.address,
        )?;
        if let Some(serial) = &self.serial {
            write!(f, " serial={serial}")?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_device_types_round_trip_through_usb_id() {
        for &dt in DeviceType::ALL {
            let id = dt.usb_id();
            let resolved = DeviceType::from_usb_id(id).unwrap();
            assert_eq!(dt, resolved, "round-trip failed for {dt}");
        }
    }

    #[test]
    fn unknown_device_returns_error() {
        let id = UsbId::new(0x1234, 0x5678);
        assert!(DeviceType::from_usb_id(id).is_err());
    }

    #[test]
    fn fx3_devices_are_correct() {
        assert!(!DeviceType::LogicOriginal.is_fx3());
        assert!(!DeviceType::Logic16.is_fx3());
        assert!(DeviceType::Logic4.is_fx3());
        assert!(DeviceType::Logic8.is_fx3());
        assert!(DeviceType::LogicPro8.is_fx3());
        assert!(DeviceType::LogicPro16.is_fx3());
    }

    #[test]
    fn fx2_devices_are_correct() {
        assert!(DeviceType::LogicOriginal.is_fx2());
        assert!(DeviceType::Logic16.is_fx2());
        assert!(!DeviceType::Logic4.is_fx2());
        assert!(!DeviceType::LogicPro16.is_fx2());
    }

    #[test]
    fn pro_devices_are_correct() {
        assert!(!DeviceType::Logic4.is_pro());
        assert!(!DeviceType::Logic8.is_pro());
        assert!(DeviceType::LogicPro8.is_pro());
        assert!(DeviceType::LogicPro16.is_pro());
    }

    #[test]
    fn type_ids_match_documented_values() {
        assert_eq!(DeviceType::LogicOriginal.type_id(), 1);
        assert_eq!(DeviceType::Logic16.type_id(), 4);
        assert_eq!(DeviceType::Logic4.type_id(), 5);
        assert_eq!(DeviceType::Logic8.type_id(), 6);
        assert_eq!(DeviceType::LogicPro8.type_id(), 7);
        assert_eq!(DeviceType::LogicPro16.type_id(), 12);
    }

    #[test]
    fn usb_id_display() {
        let id = UsbId::new(0x21a9, 0x1005);
        assert_eq!(id.to_string(), "21a9:1005");
    }

    #[test]
    fn device_type_display_matches_saleae_strings() {
        assert_eq!(DeviceType::LogicOriginal.to_string(), "Logic");
        assert_eq!(DeviceType::Logic16.to_string(), "Logic16");
        assert_eq!(DeviceType::Logic4.to_string(), "Logic4");
        assert_eq!(DeviceType::Logic8.to_string(), "Logic8");
        assert_eq!(DeviceType::LogicPro8.to_string(), "LogicPro8");
        assert_eq!(DeviceType::LogicPro16.to_string(), "LogicPro16");
    }

    #[test]
    fn all_usb_ids_length_matches_all_device_types() {
        assert_eq!(DeviceType::ALL.len(), DeviceType::ALL_USB_IDS.len());
    }
}
