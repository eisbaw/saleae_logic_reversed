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

use std::time::Duration;

use saleae_proto::device::{BootState, DeviceProfile, SaleaeDevice, UsbSpeed};
use saleae_proto::firmware::fx3::Fx3Image;
use saleae_proto::obfuscation::EncryptionMode;
use tracing::{info, warn};

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

/// Find a Saleae device, open it, and return an initialized EP1 channel.
///
/// This is the "connect to a device that already has firmware running"
/// path. It does NOT handle firmware upload or auto-recovery -- for that,
/// use [`connect_or_recover`] which handles the full "from any state to
/// connected" flow including firmware upload and dead-EP1 recovery.
///
/// When `serial_filter` is `Some`, only devices whose USB serial number
/// contains the filter string (case-insensitive) are considered. When
/// `None`, the first Saleae device found is used.
///
/// Steps:
/// 1. Find Saleae device on USB (filtered by serial if provided).
/// 2. Require firmware running (bail on bootloader/unknown).
/// 3. Determine encryption mode from device type.
/// 4. Open USB device, create transport.
/// 5. Create EP1 command channel and initialize (reseed + ping).
///
/// Returns a [`ConnectedDevice`] with the initialized channel and device info.
pub fn open_and_connect_with_serial(serial_filter: Option<&str>) -> DriverResult<ConnectedDevice> {
    // Step 1: Find device.
    let device = match serial_filter {
        Some(serial) => saleae_usb::discovery::find_saleae_by_serial(serial)?,
        None => saleae_usb::discovery::find_first_saleae()?,
    };
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
    let profile = device
        .device_type
        .profile()
        .ok_or_else(|| DriverError::InvalidArgument {
            message: format!("no device profile for {}", device.device_type),
        })?;
    let usb_speed = device
        .usb_speed
        .unwrap_or_else(|| profile.default_forced_usb_speed());

    // Step 5: Open USB and create channel.
    let usb_device = match serial_filter {
        Some(serial) => saleae_usb::discovery::open_device_by_serial(serial)?,
        None => saleae_usb::discovery::open_first_device()?,
    };
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

/// Find the first Saleae device, open it, and return an initialized EP1 channel.
///
/// Convenience wrapper around [`open_and_connect_with_serial`] with no serial filter.
pub fn open_and_connect() -> DriverResult<ConnectedDevice> {
    open_and_connect_with_serial(None)
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
///
/// When `serial_filter` is `Some`, only devices whose USB serial number
/// contains the filter string (case-insensitive) are considered.
pub fn reconnect_with_channel(
    channel: Ep1CommandChannel,
    serial_filter: Option<&str>,
) -> DriverResult<ConnectedDevice> {
    let device = match serial_filter {
        Some(serial) => saleae_usb::discovery::find_saleae_by_serial(serial)?,
        None => saleae_usb::discovery::find_first_saleae()?,
    };

    let profile = device
        .device_type
        .profile()
        .ok_or_else(|| DriverError::InvalidArgument {
            message: format!("no device profile for {}", device.device_type),
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
// connect_or_recover
// ---------------------------------------------------------------------------

/// Ensure a Saleae device is ready for EP1 communication, handling all
/// recovery scenarios automatically.
///
/// This is the "from any state to connected" facade function. It handles:
///
/// 1. **Bootloader mode**: uploads firmware, waits for re-enumeration.
/// 2. **Firmware running, EP1 alive**: returns immediately.
/// 3. **Firmware running, EP1 dead**: resets device to bootloader,
///    re-uploads firmware, waits for re-enumeration.
/// 4. **Unknown bcdDevice**: returns an error.
///
/// After recovery, opens a fresh EP1 channel (reseed + ping verified)
/// and returns a [`ConnectedDevice`] ready for use.
///
/// The `firmware_data` must be the raw bytes of an FX3 firmware image
/// (already read from disk by the caller). The `firmware_image` is the
/// parsed `Fx3Image`.
///
/// # Callback
///
/// The optional `on_status` callback receives human-readable status
/// messages suitable for display to the user. If `None`, messages are
/// logged via `tracing::info!`.
pub fn connect_or_recover(
    firmware_image: &Fx3Image,
    serial_filter: Option<&str>,
    on_status: Option<&dyn Fn(&str)>,
) -> DriverResult<ConnectedDevice> {
    let status = |msg: &str| {
        if let Some(cb) = on_status {
            cb(msg);
        } else {
            info!("{}", msg);
        }
    };

    let timeout = Duration::from_secs(10);
    let poll_interval = Duration::from_millis(500);

    // Step 1: Find device.
    let device = find_device(serial_filter)?;
    status(&format!("Found device: {device}"));
    status(&format!("  Boot state: {}", device.boot_state));

    match device.boot_state {
        BootState::Bootloader => {
            // Device is in bootloader -- upload firmware directly.
            status("Device in bootloader mode. Uploading firmware...");
            upload_firmware_and_wait(
                firmware_image,
                timeout,
                poll_interval,
                serial_filter,
                &status,
            )?;
        }
        BootState::FirmwareRunning => {
            // Firmware running -- try a quick ping to see if EP1 thread is alive.
            status("Firmware running. Testing EP1 with ping...");
            if try_ping_ep1(serial_filter) {
                status("EP1 ping succeeded. Device is ready.");
            } else {
                // EP1 is dead. Reset device to bootloader, then re-upload firmware.
                status("EP1 ping failed (command thread likely dead).");
                status("Resetting device to bootloader for firmware re-upload...");
                attempt_reset_to_bootloader(serial_filter, &status);

                // Wait for bootloader to appear.
                status("Waiting for device in bootloader mode...");
                match saleae_usb::discovery::wait_for_bootloader(timeout, poll_interval) {
                    Ok(bl_device) => {
                        status(&format!("Device in bootloader: {bl_device}"));
                    }
                    Err(_) => {
                        return Err(DriverError::PowerCycleRequired {
                            reason: format!(
                                "Device did not enter bootloader mode within {timeout:?}. \
                                 The FX3 command thread is dead and the device cannot be reset \
                                 via USB. A physical power cycle is required."
                            ),
                        });
                    }
                }

                // Upload firmware.
                status("Uploading firmware...");
                upload_firmware_and_wait(
                    firmware_image,
                    timeout,
                    poll_interval,
                    serial_filter,
                    &status,
                )?;
            }
        }
        BootState::Unknown(bcd) => {
            return Err(DriverError::InvalidState {
                current: format!("Unknown(bcdDevice={bcd:#06x})"),
                required: "FirmwareRunning or Bootloader".to_string(),
            });
        }
    }

    // At this point, firmware should be running. Open a fresh channel.
    let dev = open_and_connect_with_serial(serial_filter)?;
    status("EP1 command channel initialized.");

    Ok(dev)
}

/// Upload firmware to a device in bootloader mode and wait for re-enumeration.
///
/// Opens the USB device, sends the firmware image via control transfers,
/// drops the transport, then polls until the device reappears with
/// `BootState::FirmwareRunning`.
pub fn upload_firmware_and_wait(
    image: &Fx3Image,
    timeout: Duration,
    poll_interval: Duration,
    serial_filter: Option<&str>,
    on_status: &dyn Fn(&str),
) -> DriverResult<()> {
    let usb_device = match serial_filter {
        Some(serial) => saleae_usb::discovery::open_device_by_serial(serial)?,
        None => saleae_usb::discovery::open_first_device()?,
    };
    let transport = saleae_usb::nusb_backend::NusbTransport::open(usb_device)?;

    crate::firmware::upload_fx3_firmware(&transport, image)?;
    on_status(&format!(
        "Firmware uploaded ({} bytes). Waiting for re-enumeration...",
        image.total_data_bytes()
    ));

    // Drop transport so the old device handle is released before polling.
    drop(transport);

    saleae_usb::discovery::wait_for_device(timeout, poll_interval)?;
    on_status("Device re-enumerated with firmware running.");

    Ok(())
}

// ---------------------------------------------------------------------------
// Recovery helpers (private)
// ---------------------------------------------------------------------------

/// Find a Saleae device, optionally filtered by serial.
fn find_device(serial_filter: Option<&str>) -> DriverResult<SaleaeDevice> {
    let device = match serial_filter {
        Some(serial) => saleae_usb::discovery::find_saleae_by_serial(serial)?,
        None => saleae_usb::discovery::find_first_saleae()?,
    };
    Ok(device)
}

/// Try to ping the device via EP1. Returns true if ping succeeds.
///
/// Uses `open_and_connect_with_serial` which does reseed + ping. If anything
/// fails (timeout, USB error, wrong response), returns false.
fn try_ping_ep1(serial_filter: Option<&str>) -> bool {
    open_and_connect_with_serial(serial_filter).is_ok()
}

/// Attempt to reset the device to bootloader mode via ResetAndReEnumerate.
///
/// This may fail if the EP1 thread is dead (which is why we're resetting
/// in the first place). Failure is expected and silently handled.
fn attempt_reset_to_bootloader(serial_filter: Option<&str>, on_status: &dyn Fn(&str)) {
    // Try to open and connect. If EP1 is dead, this will fail -- that's OK.
    let mut dev = match open_and_connect_with_serial(serial_filter) {
        Ok(d) => d,
        Err(e) => {
            warn!("Could not connect for reset (expected if EP1 dead): {e}");
            on_status(&format!(
                "  Could not connect for reset (expected if EP1 dead): {e}"
            ));
            return;
        }
    };

    // Send ResetAndReEnumerate (0x82). May fail, that's OK.
    match dev.channel.send_command(0x82, &[]) {
        Ok(()) => on_status("  ResetAndReEnumerate sent."),
        Err(e) => on_status(&format!("  ResetAndReEnumerate failed (expected): {e}")),
    }
}

// ---------------------------------------------------------------------------
// init_for_capture
// ---------------------------------------------------------------------------

/// Idempotent hardware initialization for capture.
///
/// Checks whether the FPGA is already programmed (via r0x7F readback).
/// - **Cold path** (FPGA not programmed): full sequence -- FPGA program,
///   EEPROM read, ADC init, reseed, channel enables, ADC power-on.
/// - **Fast path** (FPGA already programmed): skips FPGA reprogramming
///   and full ADC init. Just reseeds, sets channel enables, and powers
///   on the ADC (which re-locks the DCM).
///
/// This makes the capture pipeline restartable without a power cycle:
/// after `adc::power_down`, calling `init_for_capture` again takes the
/// fast path.
///
/// Returns the EEPROM calibration data (16 bytes) for use in downstream
/// calibration.
pub fn init_for_capture(dev: &mut ConnectedDevice, bitstream: &[u8]) -> DriverResult<Vec<u8>> {
    let fpga_ready = crate::fpga::is_fpga_programmed(&mut dev.channel);

    if fpga_ready {
        info!("FPGA already programmed -- taking fast path (skip reprogram + ADC init)");
    }

    // Cold path: full FPGA program + ADC init.
    if !fpga_ready {
        cold_init(dev, bitstream)?;
    }

    match prepare_for_capture(dev) {
        Ok(cal_data) => Ok(cal_data),
        Err(e) if fpga_ready => {
            // Fast path failed -- fall back to full cold init.
            warn!("Fast path failed ({e}), falling back to cold init");
            cold_init(dev, bitstream)?;
            prepare_for_capture(dev)
        }
        Err(e) => Err(e),
    }
}

/// Common tail for init_for_capture: EEPROM read, reseed, channel enables,
/// ADC power-on. Extracted to avoid duplication between fast and fallback paths.
fn prepare_for_capture(dev: &mut ConnectedDevice) -> DriverResult<Vec<u8>> {
    // Read EEPROM calibration (offset 0x10, 16 bytes).
    let cal_data = dev
        .channel
        .read_version(&[0x33, 0x81, 0x10, 0x00, 0x10, 0x00])?;

    // Re-init EP1 (reseed + ping).
    dev.channel.initialize()?;

    // Channel enables (hardcoded to pcap Phase 4 defaults).
    dev.channel
        .write_registers(&[(0x0f, 0x00), (0x10, 0x0f), (0x11, 0x00)])?;

    // Pre-capture ADC power-on + DCM lock.
    let capture_dcm = crate::adc::power_on_for_capture(&mut dev.channel)?;
    info!(
        status_40 = format_args!("{:#04x}", capture_dcm.status_40),
        "ADC powered on for capture"
    );

    Ok(cal_data)
}

/// Full cold initialization: FPGA program + ADC init.
fn cold_init(dev: &mut ConnectedDevice, bitstream: &[u8]) -> DriverResult<()> {
    info!(bitstream_len = bitstream.len(), "Programming FPGA");
    crate::fpga::program_fpga(&mut dev.channel, bitstream, dev.profile, dev.usb_speed)?;
    info!("FPGA programmed and verified");

    // Read EEPROM calibration for ADC init.
    let cal_data = dev
        .channel
        .read_version(&[0x33, 0x81, 0x10, 0x00, 0x10, 0x00])?;

    let dcm_result = crate::adc::init_ad9637(&mut dev.channel, &cal_data)?;
    info!(
        locked = dcm_result.locked,
        status_40 = format_args!("{:#04x}", dcm_result.status_40),
        "AD9637 init complete"
    );
    Ok(())
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
