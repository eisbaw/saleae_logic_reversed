//! High-level facade for common device open/init sequences.
//!
//! CLI commands repeat the same boilerplate: find device, check boot state,
//! determine encryption mode, open USB, create transport, build an EP1
//! command channel, and initialize it. This module extracts those patterns
//! into reusable functions so that CLI command handlers can remain thin.
//!
//! # Typical usage
//!
//! ```ignore
//! use saleae_driver::facade;
//!
//! // Find device, open USB, create + initialize EP1 channel.
//! let mut dev = facade::open_and_connect()?;
//!
//! // Full init: FPGA program + ADC init + channel config + ADC power-on.
//! let bitstream = std::fs::read("firmware/bitstream.bin")?;
//! facade::init_for_capture(&mut dev, &bitstream)?;
//!
//! // Device is now ready for capture.
//! ```

use saleae_proto::device::{BootState, DeviceProfile, SaleaeDevice, UsbSpeed};
use saleae_proto::obfuscation::EncryptionMode;
use tracing::info;

use crate::ep1::Ep1CommandChannel;
use crate::error::{DriverError, DriverResult};

// ---------------------------------------------------------------------------
// ConnectedDevice
// ---------------------------------------------------------------------------

/// A Saleae device with an open, initialized EP1 command channel.
///
/// Bundles the channel together with the device metadata (profile, USB speed,
/// device type) that downstream operations need. Created by [`open_and_connect`].
pub struct ConnectedDevice {
    /// The EP1 command channel (already initialized: reseed + ping done).
    pub channel: Ep1CommandChannel,
    /// The device profile (channel counts, FPGA class, rates, etc.).
    pub profile: &'static DeviceProfile,
    /// The USB connection speed.
    pub usb_speed: UsbSpeed,
    /// The discovered device info (type, boot state, serial, etc.).
    pub device: SaleaeDevice,
}

// ---------------------------------------------------------------------------
// open_and_connect
// ---------------------------------------------------------------------------

/// Find the first Saleae device, open it, and return an initialized EP1 channel.
///
/// This is the "connect to a device that already has firmware running"
/// path. It does NOT handle firmware upload or auto-recovery -- for that,
/// use `ensure_device_ready` in the CLI (which calls this after firmware
/// upload).
///
/// Steps:
/// 1. Find first Saleae device on USB.
/// 2. Require firmware running (bail on bootloader/unknown).
/// 3. Determine encryption mode from device type.
/// 4. Open USB device, create transport.
/// 5. Create EP1 command channel and initialize (reseed + ping).
///
/// Returns a [`ConnectedDevice`] with the initialized channel and device info.
pub fn open_and_connect() -> DriverResult<ConnectedDevice> {
    // Step 1: Find device.
    let device = saleae_usb::discovery::find_first_saleae()?;
    info!("Found device: {device}");

    // Step 2: Require firmware running.
    match device.boot_state {
        BootState::FirmwareRunning => {}
        BootState::Bootloader => {
            return Err(DriverError::InvalidState {
                current: "Bootloader".to_string(),
                required: "FirmwareRunning (upload firmware first)".to_string(),
            });
        }
        BootState::Unknown(bcd) => {
            return Err(DriverError::InvalidState {
                current: format!("Unknown(bcdDevice={bcd:#06x})"),
                required: "FirmwareRunning".to_string(),
            });
        }
    }

    // Step 3: Determine encryption mode.
    let encryption_mode = encryption_mode_for_device(&device);

    // Step 4: Get profile and USB speed.
    let profile = device.device_type.profile().ok_or_else(|| {
        DriverError::InvalidArgument {
            message: format!("no device profile for {}", device.device_type),
        }
    })?;
    let usb_speed = device
        .usb_speed
        .unwrap_or_else(|| profile.default_forced_usb_speed());

    // Step 5: Open USB and create channel.
    let usb_device = saleae_usb::discovery::open_first_device()?;
    let transport = saleae_usb::nusb_backend::NusbTransport::open(usb_device)?;

    let reads_write_ack = device.device_type.is_pro();
    let mut channel = Ep1CommandChannel::new(Box::new(transport), encryption_mode, reads_write_ack);

    // Step 6: Initialize (reseed + ping).
    channel.initialize()?;
    info!("EP1 command channel initialized");

    Ok(ConnectedDevice {
        channel,
        profile,
        usb_speed,
        device,
    })
}

// ---------------------------------------------------------------------------
// reconnect (re-discover after ensure_device_ready)
// ---------------------------------------------------------------------------

/// Re-discover the device and create a fresh [`ConnectedDevice`] from an
/// existing, already-initialized EP1 channel.
///
/// This is for use after `ensure_device_ready` (which handles firmware
/// upload and returns a bare channel). It discovers the device to obtain
/// profile/speed info and bundles them together.
pub fn reconnect_with_channel(channel: Ep1CommandChannel) -> DriverResult<ConnectedDevice> {
    let device = saleae_usb::discovery::find_first_saleae()?;

    let profile = device.device_type.profile().ok_or_else(|| {
        DriverError::InvalidArgument {
            message: format!("no device profile for {}", device.device_type),
        }
    })?;
    let usb_speed = device
        .usb_speed
        .unwrap_or_else(|| profile.default_forced_usb_speed());

    Ok(ConnectedDevice {
        channel,
        profile,
        usb_speed,
        device,
    })
}

// ---------------------------------------------------------------------------
// init_for_capture
// ---------------------------------------------------------------------------

/// Full hardware initialization for capture: FPGA + ADC + channel config + power-on.
///
/// This is the common pipeline shared by `full_init`, `capture_digital`,
/// and `capture_analog`. After this returns, the device is ready for
/// `run_capture`.
///
/// Steps:
/// 1. Program FPGA (reset + bitstream upload + r0x7F verify).
/// 2. Read EEPROM calibration coefficients (offset 0x10, 16 bytes).
/// 3. AD9637 ADC init (with calibration data).
/// 4. Re-init EP1 (reseed + ping for next phase).
/// 5. Write channel enable registers (r0x0f, r0x10, r0x11).
/// 6. Pre-capture ADC power-on + DCM lock.
///
/// Returns the EEPROM calibration data (16 bytes) for use in downstream
/// calibration.
pub fn init_for_capture(dev: &mut ConnectedDevice, bitstream: &[u8]) -> DriverResult<Vec<u8>> {
    // Step 1: FPGA program.
    info!(
        bitstream_len = bitstream.len(),
        "Programming FPGA"
    );
    crate::fpga::program_fpga(&mut dev.channel, bitstream, dev.profile, dev.usb_speed)?;
    info!("FPGA programmed and verified");

    // Step 2: Read EEPROM calibration (offset 0x10, 16 bytes).
    let cal_data = dev
        .channel
        .read_version(&[0x33, 0x81, 0x10, 0x00, 0x10, 0x00])?;

    // Step 3: AD9637 ADC init.
    let dcm_result = crate::adc::init_ad9637(&mut dev.channel, &cal_data)?;
    info!(
        locked = dcm_result.locked,
        status_40 = format_args!("{:#04x}", dcm_result.status_40),
        "AD9637 init complete"
    );

    // Step 4: Re-init EP1 (reseed + ping).
    dev.channel.initialize()?;

    // Step 5: Channel enables (hardcoded to pcap Phase 4 defaults).
    dev.channel
        .write_registers(&[(0x0f, 0x00), (0x10, 0x0f), (0x11, 0x00)])?;

    // Step 6: Pre-capture ADC power-on.
    let capture_dcm = crate::adc::power_on_for_capture(&mut dev.channel)?;
    info!(
        status_40 = format_args!("{:#04x}", capture_dcm.status_40),
        "ADC powered on for capture"
    );

    Ok(cal_data)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Determine the EP1 encryption mode for a given device.
pub fn encryption_mode_for_device(device: &SaleaeDevice) -> EncryptionMode {
    if device.device_type.is_pro() {
        EncryptionMode::XorAll
    } else if device.device_type.is_fx3() {
        EncryptionMode::XorFirmwareOnly
    } else {
        EncryptionMode::None
    }
}
