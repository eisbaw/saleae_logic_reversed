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
// FPGA type enum
// ---------------------------------------------------------------------------

/// FPGA device features class, determines register interface and capabilities.
///
/// From the Saleae class hierarchy (`re/notes/device-class-hierarchy.md`):
///
/// - **NonPro**: `FpgaDeviceFeatures` -- 8-bit register writes, fire-and-forget
///   (no read-back verification), 62-byte FPGA programming chunks.
/// - **Pro**: `ProFpgaDeviceFeatures` -- 16-bit register writes with read-back
///   verification, 508/1020-byte FPGA programming chunks (USB 2.0/3.0).
/// - **Mso**: Separate oscilloscope device path (out of scope).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FpgaType {
    /// FpgaDeviceFeatures: Logic 4, Logic 8.
    /// Fire-and-forget register writes, 62-byte bitstream chunks.
    NonPro,
    /// ProFpgaDeviceFeatures: Logic Pro 8, Logic Pro 16.
    /// Register writes with read-back verification, larger bitstream chunks.
    Pro,
    /// MsoFpga: MSO device (out of scope for this driver).
    Mso,
}

impl std::fmt::Display for FpgaType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FpgaType::NonPro => f.write_str("NonPro (FpgaDeviceFeatures)"),
            FpgaType::Pro => f.write_str("Pro (ProFpgaDeviceFeatures)"),
            FpgaType::Mso => f.write_str("Mso (MsoFpga)"),
        }
    }
}

// ---------------------------------------------------------------------------
// Device profile
// ---------------------------------------------------------------------------

/// Static hardware profile for a device type.
///
/// Contains all hardware-dependent parameters needed for device operation,
/// derived from RE analysis in `re/notes/multi-device-differences.md`.
/// This is a data struct, not a trait -- dispatch is data-driven to keep
/// complexity low. See TASK-0073 for the source analysis.
///
/// # Usage
///
/// ```
/// use saleae_proto::device::DeviceType;
///
/// let profile = DeviceType::LogicPro8.profile().unwrap();
/// assert_eq!(profile.digital_channels, 8);
/// assert!(profile.supports_encryption);
/// ```
#[derive(Debug, Clone)]
pub struct DeviceProfile {
    /// Which device this profile describes.
    pub device_type: DeviceType,

    /// Number of digital channels.
    pub digital_channels: u8,

    /// Number of analog channels.
    pub analog_channels: u8,

    /// FPGA register interface type.
    pub fpga_type: FpgaType,

    /// ADC bit depth (8 for HMCAD1100 Xilinx revisions, 12 for AD9637/ADS528x).
    ///
    /// Note: actual ADC type depends on hardware revision, not device model.
    /// This is the typical/default value for the device.
    pub adc_bits: u8,

    /// GPIF bus width in bits (16 for non-Pro, 32 for Pro).
    ///
    /// Determines deinterleaving packet size (2 or 4 bytes per digital sample)
    /// and analog packing format (6 bytes/4 samples vs 12 bytes/8 samples).
    pub gpif_width: u8,

    /// Whether EP1 commands use full XOR encryption (Pro) or only during
    /// firmware download (non-Pro).
    pub supports_encryption: bool,

    /// Maximum digital sample rate at USB 3.0 (Hz).
    pub max_digital_rate_usb3: u64,

    /// Maximum digital sample rate at USB 2.0 (Hz).
    pub max_digital_rate_usb2: u64,

    /// Maximum analog sample rate at USB 3.0 (Hz).
    /// For Pro devices: 100 MHz at USB 3.0, 50 MHz at USB 2.0.
    pub max_analog_rate_usb3: u64,

    /// Maximum analog sample rate at USB 2.0 (Hz).
    pub max_analog_rate_usb2: u64,
}

impl DeviceProfile {
    /// FPGA bitstream programming chunk size for this device at the given USB speed.
    ///
    /// From `re/notes/usb2-fallback.md`:
    /// - NonPro: always 62 bytes (regardless of USB speed)
    /// - Pro USB 2.0: 508 bytes
    /// - Pro USB 3.0: 1020 bytes
    pub const fn fpga_chunk_size(&self, usb_speed: UsbSpeed) -> usize {
        match self.fpga_type {
            FpgaType::NonPro => 62,
            FpgaType::Pro => match usb_speed {
                UsbSpeed::Super | UsbSpeed::SuperPlus => 1020,
                _ => 508,
            },
            FpgaType::Mso => 508, // placeholder
        }
    }

    /// Whether register writes require read-back verification.
    ///
    /// Pro devices read back a 1-byte ACK after every EP1 register write.
    /// Non-Pro devices fire-and-forget.
    pub const fn register_write_verifies(&self) -> bool {
        matches!(self.fpga_type, FpgaType::Pro)
    }

    /// Default USB speed when the forced-speed flag is set.
    ///
    /// From `re/notes/usb2-fallback.md`:
    /// - Non-Pro: defaults to USB 2.0 (speed=1)
    /// - Pro: defaults to USB 3.0 (speed=2)
    pub const fn default_forced_usb_speed(&self) -> UsbSpeed {
        match self.fpga_type {
            FpgaType::Pro => UsbSpeed::Super,
            _ => UsbSpeed::High,
        }
    }

    /// Maximum digital sample rate for the given USB speed.
    pub const fn max_digital_rate(&self, usb_speed: UsbSpeed) -> u64 {
        match usb_speed {
            UsbSpeed::Super | UsbSpeed::SuperPlus => self.max_digital_rate_usb3,
            _ => self.max_digital_rate_usb2,
        }
    }

    /// Maximum analog sample rate for the given USB speed.
    ///
    /// From `re/notes/usb2-fallback.md`:
    /// - Pro USB 3.0: 100 MHz
    /// - Pro USB 2.0: 50 MHz (100 MHz exceeds USB 2.0 bandwidth)
    /// - Non-Pro: same rate regardless of speed
    pub const fn max_analog_rate(&self, usb_speed: UsbSpeed) -> u64 {
        match usb_speed {
            UsbSpeed::Super | UsbSpeed::SuperPlus => self.max_analog_rate_usb3,
            _ => self.max_analog_rate_usb2,
        }
    }

    /// Number of analog channels available at the given USB speed.
    ///
    /// From `re/notes/usb2-fallback.md`:
    /// - Logic Pro 16 at USB 2.0: channels 8-15 become digital-only, so
    ///   only 8 analog channels are available.
    /// - All other devices: full analog channel count regardless of speed.
    pub const fn available_analog_channels(&self, usb_speed: UsbSpeed) -> u8 {
        match self.device_type {
            DeviceType::LogicPro16 => match usb_speed {
                UsbSpeed::Super | UsbSpeed::SuperPlus => self.analog_channels,
                _ => {
                    // USB 2.0: upper 8 channels are digital-only
                    if self.analog_channels > 8 {
                        8
                    } else {
                        self.analog_channels
                    }
                }
            },
            _ => self.analog_channels,
        }
    }

    /// Number of interleave slots for disaggregator deinterleaving.
    ///
    /// From `re/notes/disaggregator-interleaving.md`:
    /// - Logic Pro 8: 32 slots
    /// - Logic Pro 16: 16 slots
    /// - Non-Pro: not applicable (different deinterleave path)
    pub const fn interleave_slots(&self) -> u8 {
        match self.device_type {
            DeviceType::LogicPro8 => 32,
            DeviceType::LogicPro16 => 16,
            _ => 0,
        }
    }

    /// Disaggregator type enum value for deinterleaving dispatch.
    ///
    /// From `re/notes/disaggregator-interleaving.md`:
    /// - 1 = Logic 8, 2 = Logic 16, 3 = Logic 4, 4 = Logic Pro 8,
    ///   5 = Logic Pro 16
    pub const fn disaggregator_type(&self) -> u8 {
        match self.device_type {
            DeviceType::Logic8 => 1,
            DeviceType::Logic16 => 2,
            DeviceType::Logic4 => 3,
            DeviceType::LogicPro8 => 4,
            DeviceType::LogicPro16 => 5,
            DeviceType::LogicOriginal => 0,
        }
    }

    /// Whether this device has an analog front-end with VGA/attenuators.
    pub const fn has_afe(&self) -> bool {
        matches!(self.fpga_type, FpgaType::Pro)
    }

    /// USB transfer block alignment size for the given USB speed.
    ///
    /// From `re/notes/usb2-fallback.md`:
    /// - USB 2.0: 4096 bytes
    /// - USB 3.0: 16384 bytes
    pub const fn transfer_block_alignment(&self, usb_speed: UsbSpeed) -> usize {
        match usb_speed {
            UsbSpeed::Super | UsbSpeed::SuperPlus => 16384,
            _ => 4096,
        }
    }

    /// Validate a capture configuration against this device's capabilities.
    ///
    /// Checks:
    /// - Digital sample rate does not exceed max for the USB speed.
    /// - Analog sample rate does not exceed max.
    /// - Digital channel count does not exceed device capacity.
    /// - Analog channel count does not exceed device capacity.
    /// - For Logic Pro 16 at USB 2.0: analog channels must be 0-7.
    ///
    /// Returns `Ok(())` if the configuration is valid, or a descriptive
    /// error string if not.
    pub fn validate_config(
        &self,
        digital_sample_rate: u64,
        analog_sample_rate: u64,
        digital_channel_mask: u16,
        analog_channel_mask: u16,
        usb_speed: UsbSpeed,
    ) -> Result<(), String> {
        // Check digital sample rate.
        let max_digital = self.max_digital_rate(usb_speed);
        if digital_sample_rate > max_digital {
            return Err(format!(
                "digital sample rate {} Hz exceeds maximum {} Hz for {} at {usb_speed}",
                digital_sample_rate, max_digital, self.device_type,
            ));
        }

        // Check analog sample rate (speed-dependent for Pro devices).
        let max_analog = self.max_analog_rate(usb_speed);
        if analog_sample_rate > max_analog {
            return Err(format!(
                "analog sample rate {} Hz exceeds maximum {} Hz for {} at {usb_speed}",
                analog_sample_rate, max_analog, self.device_type,
            ));
        }

        // Check digital channel range.
        let max_digital_mask: u16 = if self.digital_channels >= 16 {
            u16::MAX
        } else {
            (1u16 << self.digital_channels) - 1
        };
        if digital_channel_mask & !max_digital_mask != 0 {
            return Err(format!(
                "digital channel mask {:#06x} exceeds {} channels for {}",
                digital_channel_mask, self.digital_channels, self.device_type,
            ));
        }

        // Check analog channel range.
        let max_analog_mask: u16 = if self.analog_channels >= 16 {
            u16::MAX
        } else if self.analog_channels > 0 {
            (1u16 << self.analog_channels) - 1
        } else {
            0
        };
        if analog_channel_mask & !max_analog_mask != 0 {
            return Err(format!(
                "analog channel mask {:#06x} exceeds {} channels for {}",
                analog_channel_mask, self.analog_channels, self.device_type,
            ));
        }

        // Logic Pro 16 at USB 2.0: channels 8-15 are digital-only.
        if self.device_type == DeviceType::LogicPro16
            && !matches!(usb_speed, UsbSpeed::Super | UsbSpeed::SuperPlus)
            && analog_channel_mask & 0xFF00 != 0
        {
            return Err(format!(
                "analog channels 8-15 are not available on {} at {usb_speed} \
                 (digital-only at USB 2.0)",
                self.device_type,
            ));
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Static device profiles
// ---------------------------------------------------------------------------

/// Device profile for Logic 4 (21a9:1003).
///
/// FX2LP-based (NOT FX3). Our FX3 driver code cannot drive this device
/// without a separate FX2LP code path. Included for completeness.
pub static LOGIC_4_PROFILE: DeviceProfile = DeviceProfile {
    device_type: DeviceType::Logic4,
    digital_channels: 4,
    analog_channels: 4,
    fpga_type: FpgaType::NonPro,
    adc_bits: 8,    // HMCAD1100 (Xilinx rev), 12 for ADS528x (Lattice rev)
    gpif_width: 16, // FX2LP has 8 or 16-bit GPIF
    supports_encryption: false,
    max_digital_rate_usb3: 12_000_000, // Logic 4 is USB 2.0 only
    max_digital_rate_usb2: 12_000_000,
    max_analog_rate_usb3: 6_000_000,
    max_analog_rate_usb2: 6_000_000,
};

/// Device profile for Logic 8 (21a9:1004).
pub static LOGIC_8_PROFILE: DeviceProfile = DeviceProfile {
    device_type: DeviceType::Logic8,
    digital_channels: 8,
    analog_channels: 8,
    fpga_type: FpgaType::NonPro,
    adc_bits: 12,
    gpif_width: 16, // FpgaDeviceFeatures, 16-bit default
    supports_encryption: false,
    max_digital_rate_usb3: 200_000_000,
    max_digital_rate_usb2: 100_000_000,
    max_analog_rate_usb3: 20_000_000,
    max_analog_rate_usb2: 20_000_000,
};

/// Device profile for Logic Pro 8 (21a9:1005).
pub static LOGIC_PRO_8_PROFILE: DeviceProfile = DeviceProfile {
    device_type: DeviceType::LogicPro8,
    digital_channels: 8,
    analog_channels: 8,
    fpga_type: FpgaType::Pro,
    adc_bits: 12,
    gpif_width: 32,
    supports_encryption: true,
    max_digital_rate_usb3: 500_000_000,
    max_digital_rate_usb2: 100_000_000,
    max_analog_rate_usb3: 100_000_000,  // 100 MHz available at USB 3.0
    max_analog_rate_usb2: 50_000_000,   // 50 MHz max at USB 2.0
};

/// Device profile for Logic Pro 16 (21a9:1006).
pub static LOGIC_PRO_16_PROFILE: DeviceProfile = DeviceProfile {
    device_type: DeviceType::LogicPro16,
    digital_channels: 16,
    analog_channels: 16, // channels 8-15 become digital-only at USB 2.0
    fpga_type: FpgaType::Pro,
    adc_bits: 12,
    gpif_width: 32,
    supports_encryption: true,
    max_digital_rate_usb3: 500_000_000,
    max_digital_rate_usb2: 100_000_000,
    max_analog_rate_usb3: 100_000_000,  // 100 MHz available at USB 3.0
    max_analog_rate_usb2: 50_000_000,   // 50 MHz max at USB 2.0
};

impl DeviceType {
    /// Return the static hardware profile for this device type.
    ///
    /// Returns `None` for legacy devices (LogicOriginal, Logic16) which
    /// use a completely different architecture and are not supported by
    /// this driver.
    pub const fn profile(&self) -> Option<&'static DeviceProfile> {
        match self {
            DeviceType::Logic4 => Some(&LOGIC_4_PROFILE),
            DeviceType::Logic8 => Some(&LOGIC_8_PROFILE),
            DeviceType::LogicPro8 => Some(&LOGIC_PRO_8_PROFILE),
            DeviceType::LogicPro16 => Some(&LOGIC_PRO_16_PROFILE),
            DeviceType::LogicOriginal | DeviceType::Logic16 => None,
        }
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
/// Maps to the binary's `std::variant<DSInitializing, DSReady, DSError>` with
/// additional states for the full device lifecycle. See
/// `doc/state-machine/device-states.md`.
///
/// The binary uses three variant types (DSInitializing, DSReady, DSError).
/// We add Disconnected, Bootloader, and Capturing to model the full
/// lifecycle that the DevicesManager manages externally to the variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceState {
    /// No USB connection. Initial state and reset state after errors.
    Disconnected,
    /// Device detected in FX3 bootloader mode (bcdDevice=0x0100).
    /// Firmware upload is the only valid operation.
    Bootloader,
    /// Firmware uploaded, waiting for FPGA programming and post-init.
    /// Maps to the binary's `DSInitializing`.
    Initializing,
    /// FPGA programmed, post-init complete, idle. Maps to `DSReady`.
    Ready,
    /// Capture in progress. Sub-state of Ready in the binary
    /// (the binary tracks this separately, not in the variant).
    Capturing,
    /// Unrecoverable error. Maps to `DSError`. Requires physical
    /// reconnect (transition to Disconnected) to recover.
    Error(String),
}

impl DeviceState {
    /// Check whether a transition from this state to `next` is valid.
    ///
    /// Valid transitions:
    /// - Disconnected -> Bootloader (device found in bootloader)
    /// - Bootloader -> Initializing (firmware uploaded, re-enumerated)
    /// - Initializing -> Ready (FPGA programmed, post-init done)
    /// - Ready -> Capturing (capture started)
    /// - Capturing -> Ready (capture stopped)
    /// - Any -> Error (on USB/device error)
    /// - Error -> Disconnected (reset for reconnect)
    /// - Any -> Disconnected (USB disconnect)
    pub fn can_transition_to(&self, next: &DeviceState) -> bool {
        match (self, next) {
            // Any -> Disconnected (USB disconnect or reset)
            (_, DeviceState::Disconnected) => true,
            // Any -> Error (error can happen in any state)
            (_, DeviceState::Error(_)) => true,
            // Disconnected -> Bootloader
            (DeviceState::Disconnected, DeviceState::Bootloader) => true,
            // Bootloader -> Initializing
            (DeviceState::Bootloader, DeviceState::Initializing) => true,
            // Initializing -> Ready
            (DeviceState::Initializing, DeviceState::Ready) => true,
            // Ready -> Capturing
            (DeviceState::Ready, DeviceState::Capturing) => true,
            // Capturing -> Ready
            (DeviceState::Capturing, DeviceState::Ready) => true,
            // All other transitions are invalid
            _ => false,
        }
    }

    /// Short name for this state, suitable for error messages.
    pub fn name(&self) -> &str {
        match self {
            DeviceState::Disconnected => "Disconnected",
            DeviceState::Bootloader => "Bootloader",
            DeviceState::Initializing => "Initializing",
            DeviceState::Ready => "Ready",
            DeviceState::Capturing => "Capturing",
            DeviceState::Error(_) => "Error",
        }
    }
}

impl std::fmt::Display for DeviceState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeviceState::Disconnected => f.write_str("Disconnected"),
            DeviceState::Bootloader => f.write_str("Bootloader"),
            DeviceState::Initializing => f.write_str("Initializing"),
            DeviceState::Ready => f.write_str("Ready"),
            DeviceState::Capturing => f.write_str("Capturing"),
            DeviceState::Error(msg) => write!(f, "Error: {msg}"),
        }
    }
}

impl std::hash::Hash for DeviceState {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        core::mem::discriminant(self).hash(state);
        if let DeviceState::Error(msg) = self {
            msg.hash(state);
        }
    }
}

// ---------------------------------------------------------------------------
// USB speed (our own enum, independent of nusb)
// ---------------------------------------------------------------------------

/// USB connection speed.
///
/// This is our own type so that `saleae-proto` doesn't depend on `nusb`.
/// The `saleae-usb` discovery layer converts from `nusb::Speed`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UsbSpeed {
    /// USB 1.x Low Speed (1.5 Mbit/s).
    Low,
    /// USB 1.x Full Speed (12 Mbit/s).
    Full,
    /// USB 2.0 High Speed (480 Mbit/s).
    High,
    /// USB 3.0 SuperSpeed (5 Gbit/s).
    Super,
    /// USB 3.1 SuperSpeed+ (10 Gbit/s).
    SuperPlus,
}

impl std::fmt::Display for UsbSpeed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UsbSpeed::Low => f.write_str("Low Speed (1.5 Mbit/s)"),
            UsbSpeed::Full => f.write_str("Full Speed (12 Mbit/s)"),
            UsbSpeed::High => f.write_str("High Speed (480 Mbit/s)"),
            UsbSpeed::Super => f.write_str("SuperSpeed (5 Gbit/s)"),
            UsbSpeed::SuperPlus => f.write_str("SuperSpeed+ (10 Gbit/s)"),
        }
    }
}

// ---------------------------------------------------------------------------
// Boot state (derived from bcdDevice)
// ---------------------------------------------------------------------------

/// Device boot state, derived from the USB `bcdDevice` descriptor field.
///
/// Cypress FX3 devices enumerate differently depending on whether firmware
/// has been uploaded:
///
/// - **bcdDevice = 0x0100 (1.00)**: Device is in bootloader/WestBridge mode.
///   The SPI flash has booted the default Cypress image but no Saleae firmware
///   has been uploaded yet. The device exposes only EP0 for vendor requests.
///
/// - **bcdDevice = 0x0000 (0.00)**: Saleae firmware is running. The device
///   re-enumerates with the Saleae VID:PID and 4 endpoints (EP0, EP1 OUT,
///   EP1 IN, EP2 IN).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BootState {
    /// Bootloader mode -- firmware needs to be uploaded.
    Bootloader,
    /// Firmware is running -- device is ready for FPGA programming / capture.
    FirmwareRunning,
    /// bcdDevice value doesn't match known patterns.
    Unknown(u16),
}

impl BootState {
    /// Determine boot state from the raw `bcdDevice` descriptor value.
    pub fn from_bcd_device(bcd: u16) -> Self {
        match bcd {
            0x0100 => BootState::Bootloader,
            0x0000 => BootState::FirmwareRunning,
            other => BootState::Unknown(other),
        }
    }

    /// Return the raw `bcdDevice` value for this state (if known).
    pub fn bcd_device(self) -> Option<u16> {
        match self {
            BootState::Bootloader => Some(0x0100),
            BootState::FirmwareRunning => Some(0x0000),
            BootState::Unknown(v) => Some(v),
        }
    }
}

impl std::fmt::Display for BootState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BootState::Bootloader => f.write_str("Bootloader (needs firmware upload)"),
            BootState::FirmwareRunning => f.write_str("Firmware running"),
            BootState::Unknown(bcd) => write!(f, "Unknown (bcdDevice={bcd:#06x})"),
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
    /// Boot state derived from bcdDevice descriptor.
    pub boot_state: BootState,
    /// USB connection speed, if reported by the OS.
    pub usb_speed: Option<UsbSpeed>,
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

    #[test]
    fn boot_state_from_bcd_device() {
        assert_eq!(BootState::from_bcd_device(0x0100), BootState::Bootloader);
        assert_eq!(
            BootState::from_bcd_device(0x0000),
            BootState::FirmwareRunning
        );
        assert_eq!(
            BootState::from_bcd_device(0x0200),
            BootState::Unknown(0x0200)
        );
    }

    #[test]
    fn boot_state_round_trip() {
        assert_eq!(BootState::Bootloader.bcd_device(), Some(0x0100));
        assert_eq!(BootState::FirmwareRunning.bcd_device(), Some(0x0000));
        assert_eq!(BootState::Unknown(0x0042).bcd_device(), Some(0x0042));
    }

    #[test]
    fn boot_state_display() {
        assert_eq!(
            BootState::Bootloader.to_string(),
            "Bootloader (needs firmware upload)"
        );
        assert_eq!(BootState::FirmwareRunning.to_string(), "Firmware running");
        assert!(BootState::Unknown(0x0200).to_string().contains("0x0200"));
    }

    #[test]
    fn usb_speed_display() {
        assert!(UsbSpeed::Super.to_string().contains("5 Gbit/s"));
        assert!(UsbSpeed::High.to_string().contains("480 Mbit/s"));
        assert!(UsbSpeed::Low.to_string().contains("1.5 Mbit/s"));
    }

    // -----------------------------------------------------------------------
    // FpgaType tests
    // -----------------------------------------------------------------------

    #[test]
    fn fpga_type_display() {
        assert!(FpgaType::NonPro.to_string().contains("NonPro"));
        assert!(FpgaType::Pro.to_string().contains("Pro"));
        assert!(FpgaType::Mso.to_string().contains("Mso"));
    }

    // -----------------------------------------------------------------------
    // DeviceProfile tests
    // -----------------------------------------------------------------------

    #[test]
    fn fx3_devices_have_profiles() {
        assert!(DeviceType::Logic4.profile().is_some());
        assert!(DeviceType::Logic8.profile().is_some());
        assert!(DeviceType::LogicPro8.profile().is_some());
        assert!(DeviceType::LogicPro16.profile().is_some());
    }

    #[test]
    fn legacy_devices_have_no_profile() {
        assert!(DeviceType::LogicOriginal.profile().is_none());
        assert!(DeviceType::Logic16.profile().is_none());
    }

    #[test]
    fn profile_device_types_match() {
        for &dt in &[
            DeviceType::Logic4,
            DeviceType::Logic8,
            DeviceType::LogicPro8,
            DeviceType::LogicPro16,
        ] {
            let profile = dt.profile().unwrap();
            assert_eq!(profile.device_type, dt);
        }
    }

    #[test]
    fn profile_channel_counts() {
        assert_eq!(LOGIC_4_PROFILE.digital_channels, 4);
        assert_eq!(LOGIC_4_PROFILE.analog_channels, 4);

        assert_eq!(LOGIC_8_PROFILE.digital_channels, 8);
        assert_eq!(LOGIC_8_PROFILE.analog_channels, 8);

        assert_eq!(LOGIC_PRO_8_PROFILE.digital_channels, 8);
        assert_eq!(LOGIC_PRO_8_PROFILE.analog_channels, 8);

        assert_eq!(LOGIC_PRO_16_PROFILE.digital_channels, 16);
        assert_eq!(LOGIC_PRO_16_PROFILE.analog_channels, 16);
    }

    #[test]
    fn profile_fpga_types() {
        assert_eq!(LOGIC_4_PROFILE.fpga_type, FpgaType::NonPro);
        assert_eq!(LOGIC_8_PROFILE.fpga_type, FpgaType::NonPro);
        assert_eq!(LOGIC_PRO_8_PROFILE.fpga_type, FpgaType::Pro);
        assert_eq!(LOGIC_PRO_16_PROFILE.fpga_type, FpgaType::Pro);
    }

    #[test]
    fn profile_encryption() {
        assert!(!LOGIC_4_PROFILE.supports_encryption);
        assert!(!LOGIC_8_PROFILE.supports_encryption);
        assert!(LOGIC_PRO_8_PROFILE.supports_encryption);
        assert!(LOGIC_PRO_16_PROFILE.supports_encryption);
    }

    #[test]
    fn profile_max_rates() {
        assert_eq!(LOGIC_4_PROFILE.max_digital_rate_usb3, 12_000_000);
        assert_eq!(LOGIC_8_PROFILE.max_digital_rate_usb3, 200_000_000);
        assert_eq!(LOGIC_PRO_8_PROFILE.max_digital_rate_usb3, 500_000_000);
        assert_eq!(LOGIC_PRO_16_PROFILE.max_digital_rate_usb3, 500_000_000);

        // Non-Pro: same analog rate regardless of USB speed.
        assert_eq!(LOGIC_4_PROFILE.max_analog_rate_usb3, 6_000_000);
        assert_eq!(LOGIC_4_PROFILE.max_analog_rate_usb2, 6_000_000);
        assert_eq!(LOGIC_8_PROFILE.max_analog_rate_usb3, 20_000_000);
        assert_eq!(LOGIC_8_PROFILE.max_analog_rate_usb2, 20_000_000);

        // Pro: 100 MHz at USB 3.0, 50 MHz at USB 2.0.
        assert_eq!(LOGIC_PRO_8_PROFILE.max_analog_rate_usb3, 100_000_000);
        assert_eq!(LOGIC_PRO_8_PROFILE.max_analog_rate_usb2, 50_000_000);
        assert_eq!(LOGIC_PRO_16_PROFILE.max_analog_rate_usb3, 100_000_000);
        assert_eq!(LOGIC_PRO_16_PROFILE.max_analog_rate_usb2, 50_000_000);
    }

    #[test]
    fn profile_adc_bits() {
        assert_eq!(LOGIC_4_PROFILE.adc_bits, 8);
        assert_eq!(LOGIC_8_PROFILE.adc_bits, 12);
        assert_eq!(LOGIC_PRO_8_PROFILE.adc_bits, 12);
        assert_eq!(LOGIC_PRO_16_PROFILE.adc_bits, 12);
    }

    #[test]
    fn profile_gpif_width() {
        assert_eq!(LOGIC_4_PROFILE.gpif_width, 16);
        assert_eq!(LOGIC_8_PROFILE.gpif_width, 16);
        assert_eq!(LOGIC_PRO_8_PROFILE.gpif_width, 32);
        assert_eq!(LOGIC_PRO_16_PROFILE.gpif_width, 32);
    }

    // -----------------------------------------------------------------------
    // Derived profile method tests
    // -----------------------------------------------------------------------

    #[test]
    fn fpga_chunk_size_non_pro() {
        let p = &LOGIC_8_PROFILE;
        assert_eq!(p.fpga_chunk_size(UsbSpeed::High), 62);
        assert_eq!(p.fpga_chunk_size(UsbSpeed::Super), 62);
    }

    #[test]
    fn fpga_chunk_size_pro() {
        let p = &LOGIC_PRO_8_PROFILE;
        assert_eq!(p.fpga_chunk_size(UsbSpeed::High), 508);
        assert_eq!(p.fpga_chunk_size(UsbSpeed::Super), 1020);
        assert_eq!(p.fpga_chunk_size(UsbSpeed::SuperPlus), 1020);
    }

    #[test]
    fn register_write_verifies() {
        assert!(!LOGIC_4_PROFILE.register_write_verifies());
        assert!(!LOGIC_8_PROFILE.register_write_verifies());
        assert!(LOGIC_PRO_8_PROFILE.register_write_verifies());
        assert!(LOGIC_PRO_16_PROFILE.register_write_verifies());
    }

    #[test]
    fn default_forced_usb_speed() {
        assert_eq!(LOGIC_8_PROFILE.default_forced_usb_speed(), UsbSpeed::High);
        assert_eq!(
            LOGIC_PRO_8_PROFILE.default_forced_usb_speed(),
            UsbSpeed::Super
        );
    }

    #[test]
    fn max_digital_rate_by_speed() {
        let p = &LOGIC_PRO_8_PROFILE;
        assert_eq!(p.max_digital_rate(UsbSpeed::Super), 500_000_000);
        assert_eq!(p.max_digital_rate(UsbSpeed::High), 100_000_000);
    }

    #[test]
    fn interleave_slots() {
        assert_eq!(LOGIC_PRO_8_PROFILE.interleave_slots(), 32);
        assert_eq!(LOGIC_PRO_16_PROFILE.interleave_slots(), 16);
        assert_eq!(LOGIC_8_PROFILE.interleave_slots(), 0);
    }

    #[test]
    fn disaggregator_types() {
        assert_eq!(LOGIC_8_PROFILE.disaggregator_type(), 1);
        assert_eq!(LOGIC_4_PROFILE.disaggregator_type(), 3);
        assert_eq!(LOGIC_PRO_8_PROFILE.disaggregator_type(), 4);
        assert_eq!(LOGIC_PRO_16_PROFILE.disaggregator_type(), 5);
    }

    #[test]
    fn has_afe() {
        assert!(!LOGIC_4_PROFILE.has_afe());
        assert!(!LOGIC_8_PROFILE.has_afe());
        assert!(LOGIC_PRO_8_PROFILE.has_afe());
        assert!(LOGIC_PRO_16_PROFILE.has_afe());
    }

    #[test]
    fn transfer_block_alignment() {
        let p = &LOGIC_PRO_8_PROFILE;
        assert_eq!(p.transfer_block_alignment(UsbSpeed::High), 4096);
        assert_eq!(p.transfer_block_alignment(UsbSpeed::Super), 16384);
    }

    #[test]
    fn max_analog_rate_by_speed() {
        // Pro devices: 100 MHz at USB 3.0, 50 MHz at USB 2.0.
        let p = &LOGIC_PRO_8_PROFILE;
        assert_eq!(p.max_analog_rate(UsbSpeed::Super), 100_000_000);
        assert_eq!(p.max_analog_rate(UsbSpeed::SuperPlus), 100_000_000);
        assert_eq!(p.max_analog_rate(UsbSpeed::High), 50_000_000);
        assert_eq!(p.max_analog_rate(UsbSpeed::Full), 50_000_000);

        // Non-Pro: same rate at any speed.
        let p = &LOGIC_8_PROFILE;
        assert_eq!(p.max_analog_rate(UsbSpeed::Super), 20_000_000);
        assert_eq!(p.max_analog_rate(UsbSpeed::High), 20_000_000);
    }

    #[test]
    fn available_analog_channels_pro16_usb2() {
        let p = &LOGIC_PRO_16_PROFILE;
        // USB 3.0: all 16 analog channels.
        assert_eq!(p.available_analog_channels(UsbSpeed::Super), 16);
        assert_eq!(p.available_analog_channels(UsbSpeed::SuperPlus), 16);
        // USB 2.0: only 8 analog channels (upper 8 are digital-only).
        assert_eq!(p.available_analog_channels(UsbSpeed::High), 8);
        assert_eq!(p.available_analog_channels(UsbSpeed::Full), 8);
    }

    #[test]
    fn available_analog_channels_pro8_unaffected() {
        // Pro 8 has only 8 channels, not affected by USB speed.
        let p = &LOGIC_PRO_8_PROFILE;
        assert_eq!(p.available_analog_channels(UsbSpeed::Super), 8);
        assert_eq!(p.available_analog_channels(UsbSpeed::High), 8);
    }

    #[test]
    fn available_analog_channels_non_pro_unaffected() {
        let p = &LOGIC_8_PROFILE;
        assert_eq!(p.available_analog_channels(UsbSpeed::Super), 8);
        assert_eq!(p.available_analog_channels(UsbSpeed::High), 8);

        let p = &LOGIC_4_PROFILE;
        assert_eq!(p.available_analog_channels(UsbSpeed::High), 4);
    }

    // -----------------------------------------------------------------------
    // validate_config tests
    // -----------------------------------------------------------------------

    #[test]
    fn validate_config_pro8_valid() {
        let p = &LOGIC_PRO_8_PROFILE;
        p.validate_config(500_000_000, 50_000_000, 0xFF, 0xFF, UsbSpeed::Super)
            .unwrap();
    }

    #[test]
    fn validate_config_digital_rate_too_high() {
        let p = &LOGIC_PRO_8_PROFILE;
        let err = p
            .validate_config(600_000_000, 0, 0xFF, 0, UsbSpeed::Super)
            .unwrap_err();
        assert!(err.contains("digital sample rate"));
        assert!(err.contains("500000000"));
    }

    #[test]
    fn validate_config_analog_rate_too_high_usb3() {
        let p = &LOGIC_PRO_8_PROFILE;
        // At USB 3.0, max analog is 100 MHz; 200 MHz should fail.
        let err = p
            .validate_config(100_000_000, 200_000_000, 0xFF, 0xFF, UsbSpeed::Super)
            .unwrap_err();
        assert!(err.contains("analog sample rate"));
        assert!(err.contains("100000000"));
    }

    #[test]
    fn validate_config_analog_rate_too_high_usb2() {
        let p = &LOGIC_PRO_8_PROFILE;
        // At USB 2.0, max analog is 50 MHz; 100 MHz should fail.
        let err = p
            .validate_config(50_000_000, 100_000_000, 0xFF, 0xFF, UsbSpeed::High)
            .unwrap_err();
        assert!(err.contains("analog sample rate"));
        assert!(err.contains("50000000"));
    }

    #[test]
    fn validate_config_analog_rate_at_usb3_limit_ok() {
        let p = &LOGIC_PRO_8_PROFILE;
        // 100 MHz analog at USB 3.0 should pass.
        p.validate_config(100_000_000, 100_000_000, 0xFF, 0xFF, UsbSpeed::Super)
            .unwrap();
    }

    #[test]
    fn validate_config_digital_channel_out_of_range() {
        let p = &LOGIC_8_PROFILE;
        let err = p
            .validate_config(100_000_000, 0, 0x1FF, 0, UsbSpeed::Super)
            .unwrap_err();
        assert!(err.contains("digital channel mask"));
        assert!(err.contains("8 channels"));
    }

    #[test]
    fn validate_config_analog_channel_out_of_range() {
        let p = &LOGIC_8_PROFILE;
        let err = p
            .validate_config(100_000_000, 10_000_000, 0xFF, 0x1FF, UsbSpeed::Super)
            .unwrap_err();
        assert!(err.contains("analog channel mask"));
    }

    #[test]
    fn validate_config_pro16_usb2_analog_upper_channels() {
        let p = &LOGIC_PRO_16_PROFILE;
        // Analog on channels 8-15 at USB 2.0 should fail.
        let err = p
            .validate_config(100_000_000, 10_000_000, 0xFFFF, 0x0100, UsbSpeed::High)
            .unwrap_err();
        assert!(err.contains("channels 8-15"));
        assert!(err.contains("digital-only"));
    }

    #[test]
    fn validate_config_pro16_usb3_analog_upper_channels_ok() {
        let p = &LOGIC_PRO_16_PROFILE;
        // Analog on channels 8-15 at USB 3.0 should be fine.
        p.validate_config(500_000_000, 50_000_000, 0xFFFF, 0xFF00, UsbSpeed::Super)
            .unwrap();
    }

    #[test]
    fn validate_config_usb2_rate_limits() {
        let p = &LOGIC_PRO_8_PROFILE;
        // 500 MHz digital at USB 2.0 should fail (max 100 MHz).
        let err = p
            .validate_config(500_000_000, 0, 0xFF, 0, UsbSpeed::High)
            .unwrap_err();
        assert!(err.contains("digital sample rate"));
        assert!(err.contains("100000000"));
    }

    #[test]
    fn validate_config_zero_rates_ok() {
        let p = &LOGIC_PRO_8_PROFILE;
        p.validate_config(0, 0, 0, 0, UsbSpeed::Super).unwrap();
    }
}
