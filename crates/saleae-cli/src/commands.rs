//! CLI command implementations.

use std::time::Duration;

use anyhow::{bail, Context, Result};
use saleae_proto::device::BootState;

// -----------------------------------------------------------------------
// Default firmware/bitstream paths (relative to project root)
// -----------------------------------------------------------------------

/// Default firmware image path (used when --firmware is not specified).
pub const DEFAULT_FIRMWARE_PATH: &str = "firmware/fw-fx3-pro8.img";

/// Default FPGA bitstream path (Xilinx Spartan-6 for Logic Pro 8 Rev 1.2).
pub const DEFAULT_BITSTREAM_PATH: &str = "firmware/bitstream-spartan6-slx9-03.bin";

/// SLX9 bitstream variants to try during auto-detection (ordered by likelihood).
const SLX9_BITSTREAMS: &[&str] = &[
    "firmware/bitstream-spartan6-slx9-03.bin",
    "firmware/bitstream-spartan6-slx9-02.bin",
    "firmware/bitstream-spartan6-slx9-07.bin",
    "firmware/bitstream-spartan6-slx9-01.bin",
    "firmware/bitstream-spartan6-slx9-06.bin",
    "firmware/bitstream-spartan6-slx9-00.bin",
];

// -----------------------------------------------------------------------
// Auto-recovery helper
// -----------------------------------------------------------------------

/// Ensure a Saleae device is ready for EP1 communication.
///
/// This handles the common failure mode where the FX3 command thread is
/// dead (previous host process exited, DMA channel destroyed). The recovery
/// sequence is:
///
/// 1. Find the device on USB.
/// 2. If in bootloader mode: upload firmware, wait for re-enumeration.
/// 3. If firmware running: try a ping to test EP1.
///    - If ping succeeds: EP1 is alive, return the channel.
///    - If ping fails (timeout): EP1 thread is dead.
///      a. Send ResetAndReEnumerate (may fail, that's OK).
///      b. Wait for device in bootloader mode.
///      c. Upload firmware.
///      d. Wait for re-enumeration.
/// 4. After firmware is confirmed running, open transport + create EP1 channel.
/// 5. Return the ready-to-use initialized channel.
///
/// The returned channel has already passed ReseedDevice + Ping.
pub fn ensure_device_ready(firmware_path: &str) -> Result<saleae_driver::ep1::Ep1CommandChannel> {
    // Parse firmware upfront so we fail fast on bad path.
    let firmware_data = std::fs::read(firmware_path)
        .with_context(|| format!("failed to read firmware file: {firmware_path}"))?;
    let (image, _) = saleae_proto::Fx3Image::parse(&firmware_data)
        .context("failed to parse FX3 firmware image")?;

    let timeout = Duration::from_secs(10);
    let poll_interval = Duration::from_millis(500);

    // Step 1: Find device.
    let device =
        saleae_usb::discovery::find_first_saleae().context("no Saleae device found on USB")?;

    println!("Found device: {device}");
    println!("  Boot state: {}", device.boot_state);

    match device.boot_state {
        BootState::Bootloader => {
            // Device is in bootloader -- upload firmware directly.
            println!("Device in bootloader mode. Uploading firmware...");
            upload_firmware_and_wait(&image, timeout, poll_interval)?;
        }
        BootState::FirmwareRunning => {
            // Firmware running -- try a quick ping to see if EP1 thread is alive.
            println!("Firmware running. Testing EP1 with ping...");
            if !try_ping_ep1(&device) {
                // EP1 is dead. Reset device to bootloader, then re-upload firmware.
                println!("EP1 ping failed (command thread likely dead).");
                println!("Resetting device to bootloader for firmware re-upload...");
                attempt_reset_to_bootloader(&device);

                // Wait for bootloader to appear.
                println!("Waiting for device in bootloader mode...");
                match saleae_usb::discovery::wait_for_bootloader(timeout, poll_interval) {
                    Ok(bl_device) => {
                        println!("Device in bootloader: {bl_device}");
                    }
                    Err(_) => {
                        bail!(
                            "Device did not enter bootloader mode within {timeout:?}.\n\
                             The FX3 command thread is dead and the device cannot be reset\n\
                             via USB. A physical power cycle is required."
                        );
                    }
                }

                // Upload firmware.
                println!("Uploading firmware...");
                upload_firmware_and_wait(&image, timeout, poll_interval)?;
            } else {
                println!("EP1 ping succeeded. Device is ready.");
                // Device is good -- open a fresh channel and return it.
                // (The ping channel consumed the transport, so we open a new one.)
            }
        }
        BootState::Unknown(bcd) => {
            bail!("Device has unexpected bcdDevice value {bcd:#06x}.");
        }
    }

    // At this point, firmware should be running. Open a fresh channel.
    let device =
        saleae_usb::discovery::find_first_saleae().context("device disappeared after recovery")?;

    if device.boot_state != BootState::FirmwareRunning {
        bail!(
            "Expected firmware running after recovery, got: {}",
            device.boot_state
        );
    }

    let encryption_mode = encryption_mode_for_device(&device);
    let usb_device = saleae_usb::discovery::open_first_device().context("failed to open device")?;
    let transport = saleae_usb::nusb_backend::NusbTransport::open(usb_device)
        .context("failed to claim USB interface")?;

    // Note: The framed protocol handshake (0xA7/0xFE) is NOT needed for EP1.
    // The FX3 firmware starts the EP1 command thread at boot and it accepts
    // encrypted commands directly. Sending framed packets on EP1 corrupts
    // the command thread state.

    let reads_write_ack = device.device_type.is_pro();
    let mut channel =
        saleae_driver::ep1::Ep1CommandChannel::new(Box::new(transport), encryption_mode, reads_write_ack);

    // Initialize the channel (ReseedDevice + Ping).
    channel
        .initialize()
        .context("EP1 initialization failed after recovery")?;
    println!("EP1 command channel initialized.");

    Ok(channel)
}

/// Try to ping the device via EP1. Returns true if ping succeeds.
///
/// Opens a fresh transport, creates a channel, and attempts initialize().
/// If anything fails (timeout, USB error, wrong response), returns false.
fn try_ping_ep1(device: &saleae_proto::device::SaleaeDevice) -> bool {
    let encryption_mode = encryption_mode_for_device(device);

    let usb_device = match saleae_usb::discovery::open_first_device() {
        Ok(d) => d,
        Err(_) => return false,
    };

    let transport = match saleae_usb::nusb_backend::NusbTransport::open(usb_device) {
        Ok(t) => t,
        Err(_) => return false,
    };

    let reads_write_ack = device.device_type.is_pro();
    let mut channel =
        saleae_driver::ep1::Ep1CommandChannel::new(Box::new(transport), encryption_mode, reads_write_ack);

    channel.initialize().is_ok()
}

/// Attempt to reset the device to bootloader mode via ResetAndReEnumerate.
///
/// This may fail if the EP1 thread is dead (which is why we're resetting
/// in the first place). Failure is expected and silently ignored.
fn attempt_reset_to_bootloader(device: &saleae_proto::device::SaleaeDevice) {
    let encryption_mode = encryption_mode_for_device(device);

    let usb_device = match saleae_usb::discovery::open_first_device() {
        Ok(d) => d,
        Err(e) => {
            println!("  Could not open device for reset: {e}");
            return;
        }
    };

    let transport = match saleae_usb::nusb_backend::NusbTransport::open(usb_device) {
        Ok(t) => t,
        Err(e) => {
            println!("  Could not claim interface for reset: {e}");
            return;
        }
    };

    let reads_write_ack = device.device_type.is_pro();
    let mut channel =
        saleae_driver::ep1::Ep1CommandChannel::new(Box::new(transport), encryption_mode, reads_write_ack);

    // Try to initialize first (may fail if EP1 is dead).
    let _ = channel.initialize();

    // Send ResetAndReEnumerate (0x82). May fail, that's OK.
    match channel.send_command(0x82, &[]) {
        Ok(()) => println!("  ResetAndReEnumerate sent."),
        Err(e) => println!("  ResetAndReEnumerate failed (expected): {e}"),
    }
}

/// Upload firmware to the device and wait for re-enumeration.
///
/// Assumes the device is currently in bootloader mode.
fn upload_firmware_and_wait(
    image: &saleae_proto::Fx3Image,
    timeout: Duration,
    poll_interval: Duration,
) -> Result<()> {
    let usb_device = saleae_usb::discovery::open_first_device()
        .context("failed to open device for firmware upload")?;
    let transport = saleae_usb::nusb_backend::NusbTransport::open(usb_device)
        .context("failed to claim USB interface for firmware upload")?;

    saleae_driver::firmware::upload_fx3_firmware(&transport, image)
        .context("firmware upload failed")?;
    println!(
        "Firmware uploaded ({} bytes). Waiting for re-enumeration...",
        image.total_data_bytes()
    );

    // Drop transport so the old device handle is released before polling.
    drop(transport);

    saleae_usb::discovery::wait_for_device(timeout, poll_interval)
        .context("device did not re-enumerate after firmware upload")?;
    println!("Device re-enumerated with firmware running.");

    Ok(())
}

/// Determine the encryption mode for a given device.
fn encryption_mode_for_device(
    device: &saleae_proto::device::SaleaeDevice,
) -> saleae_proto::obfuscation::EncryptionMode {
    if device.device_type.is_pro() {
        saleae_proto::obfuscation::EncryptionMode::XorAll
    } else if device.device_type.is_fx3() {
        saleae_proto::obfuscation::EncryptionMode::XorFirmwareOnly
    } else {
        saleae_proto::obfuscation::EncryptionMode::None
    }
}

/// List connected Saleae devices.
pub fn list_devices() -> Result<()> {
    let devices =
        saleae_usb::discovery::list_devices().context("failed to enumerate USB devices")?;

    if devices.is_empty() {
        println!("No Saleae devices found.");
        println!();
        println!("Troubleshooting:");
        println!("  - Is the device plugged in?");
        println!("  - Are udev rules installed? (just udev-install)");
        println!("  - Do you have USB permissions?");
        return Ok(());
    }

    println!("Found {} Saleae device(s):", devices.len());
    println!();
    for device in &devices {
        println!("  {device}");
        println!(
            "    Controller: {}  Pro: {}",
            if device.device_type.is_fx3() {
                "FX3"
            } else {
                "FX2"
            },
            if device.device_type.is_pro() {
                "yes"
            } else {
                "no"
            },
        );
        println!("    Boot state: {}", device.boot_state);
        match device.usb_speed {
            Some(speed) => println!("    USB speed:  {speed}"),
            None => println!("    USB speed:  (unknown)"),
        }
    }

    Ok(())
}

/// Upload firmware to a Saleae device.
///
/// Checks the device's boot state before attempting upload:
/// - If firmware is already running (bcdDevice=0x0000), prints a message
///   and exits successfully without uploading.
/// - If in bootloader state (bcdDevice=0x0100), proceeds with upload
///   and waits for re-enumeration.
pub fn upload_firmware(firmware_path: &str) -> Result<()> {
    // Read and parse firmware file first (before touching USB), so we
    // fail fast on a bad file path or corrupt image.
    let firmware_data = std::fs::read(firmware_path)
        .with_context(|| format!("failed to read firmware file: {firmware_path}"))?;

    let (image, _consumed) = saleae_proto::Fx3Image::parse(&firmware_data)
        .context("failed to parse FX3 firmware image")?;

    println!("Firmware image parsed:");
    println!("  Sections:    {}", image.sections.len());
    println!("  Total data:  {} bytes", image.total_data_bytes());
    println!("  Entry point: {:#010x}", image.entry_address);
    println!();

    // Discover device and check boot state.
    let device =
        saleae_usb::discovery::find_first_saleae().context("failed to find Saleae device")?;

    println!("Found device: {device}");
    println!("  Boot state: {}", device.boot_state);
    if let Some(speed) = device.usb_speed {
        println!("  USB speed:  {speed}");
    }
    println!();

    match device.boot_state {
        BootState::FirmwareRunning => {
            println!("Firmware is already running on this device.");
            println!("No upload needed. The device is ready for FPGA programming.");
            println!();
            println!("To force a firmware re-upload, power-cycle the device first");
            println!("so it re-enters bootloader mode (bcdDevice=1.00).");
            return Ok(());
        }
        BootState::Bootloader => {
            println!("Device is in bootloader mode. Proceeding with firmware upload...");
        }
        BootState::Unknown(bcd) => {
            bail!(
                "Device has unexpected bcdDevice value {bcd:#06x}. \
                 Cannot determine whether firmware upload is safe. \
                 Expected 0x0000 (firmware running) or 0x0100 (bootloader)."
            );
        }
    }

    // Verify this is an FX3-based device.
    if !device.device_type.is_fx3() {
        bail!(
            "Device {} is not FX3-based (it's {}). \
             FX3 firmware upload is not applicable to this device.",
            device.device_type,
            if device.device_type.is_fx2() {
                "FX2-based"
            } else {
                "unknown controller"
            }
        );
    }

    // Open the device and upload firmware.
    let usb_device =
        saleae_usb::discovery::open_first_device().context("failed to open Saleae device")?;

    let transport = saleae_usb::nusb_backend::NusbTransport::open(usb_device)
        .context("failed to claim USB interface")?;

    saleae_driver::firmware::upload_fx3_firmware(&transport, &image)
        .context("firmware upload failed")?;

    println!("Firmware uploaded. Waiting for device to re-enumerate...");

    // Drop the transport so the old device handle is released before
    // we poll for the re-enumerated device.
    drop(transport);

    // Wait for the device to reappear with firmware running.
    let timeout = Duration::from_secs(10);
    let poll_interval = Duration::from_millis(500);

    match saleae_usb::discovery::wait_for_device(timeout, poll_interval) {
        Ok(new_device) => {
            println!();
            println!("Device re-enumerated successfully:");
            println!("  {new_device}");
            println!("  Boot state: {}", new_device.boot_state);
            if let Some(speed) = new_device.usb_speed {
                println!("  USB speed:  {speed}");
            }
            println!();
            println!("Firmware upload complete. Device is ready for FPGA programming.");
        }
        Err(_) => {
            println!();
            println!("WARNING: Device did not re-enumerate within {timeout:?}.");
            println!("The firmware may still be loading, or the device may need");
            println!("a power cycle. Try 'saleae-cli list-devices' to check.");
        }
    }

    Ok(())
}

/// Reset and re-enumerate the device.
///
/// Sends ResetAndReEnumerate (opcode 0x82) via EP1. This causes the FX3
/// to reset and boot from EEPROM (B2 bootloader), returning to bootloader mode.
pub fn reset_device() -> Result<()> {
    let device =
        saleae_usb::discovery::find_first_saleae().context("failed to find Saleae device")?;

    println!("Found device: {device}");

    match device.boot_state {
        BootState::FirmwareRunning => {}
        BootState::Bootloader => {
            println!("Device is already in bootloader mode.");
            return Ok(());
        }
        BootState::Unknown(bcd) => {
            bail!("Unexpected bcdDevice {bcd:#06x}");
        }
    }

    let encryption_mode = if device.device_type.is_pro() {
        saleae_proto::obfuscation::EncryptionMode::XorAll
    } else if device.device_type.is_fx3() {
        saleae_proto::obfuscation::EncryptionMode::XorFirmwareOnly
    } else {
        saleae_proto::obfuscation::EncryptionMode::None
    };

    let usb_device = saleae_usb::discovery::open_first_device().context("failed to open device")?;
    let transport = saleae_usb::nusb_backend::NusbTransport::open(usb_device)
        .context("failed to claim USB interface")?;
    let reads_write_ack = device.device_type.is_pro();
    let mut channel =
        saleae_driver::ep1::Ep1CommandChannel::new(Box::new(transport), encryption_mode, reads_write_ack);

    println!("Initializing EP1...");
    channel.initialize().context("EP1 initialization failed")?;

    println!("Sending ResetAndReEnumerate (0x82)...");
    // Send the command. The device will reset, so we don't expect a response.
    let _ = channel.send_command(0x82, &[]);
    println!("Reset command sent. Device should re-enumerate in bootloader mode.");
    println!("Wait a few seconds, then check with: lsusb -d 21a9:");

    Ok(())
}

/// Ping the device: ReseedDevice + Ping handshake.
///
/// This tests firmware responsiveness and verifies the XOR encryption
/// is working correctly. It is the first real hardware validation of
/// the LFSR implementation.
pub fn ping() -> Result<()> {
    // Discover device and check boot state.
    let device =
        saleae_usb::discovery::find_first_saleae().context("failed to find Saleae device")?;

    println!("Found device: {device}");
    println!("  Boot state: {}", device.boot_state);
    if let Some(speed) = device.usb_speed {
        println!("  USB speed:  {speed}");
    }
    println!();

    match device.boot_state {
        BootState::FirmwareRunning => {
            println!("Firmware is running. Proceeding with ping...");
        }
        BootState::Bootloader => {
            bail!(
                "Device is in bootloader mode. Firmware must be uploaded first.\n\
                 Run: saleae-cli upload-firmware --firmware <path>"
            );
        }
        BootState::Unknown(bcd) => {
            bail!("Device has unexpected bcdDevice value {bcd:#06x}.");
        }
    }

    // Determine encryption mode from device type.
    let encryption_mode = if device.device_type.is_pro() {
        saleae_proto::obfuscation::EncryptionMode::XorAll
    } else if device.device_type.is_fx3() {
        // Non-Pro FX3 devices only encrypt during firmware download.
        // For normal commands after firmware is running, no encryption.
        saleae_proto::obfuscation::EncryptionMode::XorFirmwareOnly
    } else {
        saleae_proto::obfuscation::EncryptionMode::None
    };

    println!("Encryption mode: {encryption_mode:?}");
    println!();

    // Open device and create command channel.
    let usb_device =
        saleae_usb::discovery::open_first_device().context("failed to open Saleae device")?;

    let transport = saleae_usb::nusb_backend::NusbTransport::open(usb_device)
        .context("failed to claim USB interface")?;

    let reads_write_ack = device.device_type.is_pro();
    let mut channel =
        saleae_driver::ep1::Ep1CommandChannel::new(Box::new(transport), encryption_mode, reads_write_ack);

    // Run the ReseedDevice + Ping handshake.
    println!("Running ReseedDevice + Ping...");
    match channel.initialize() {
        Ok(()) => {
            println!();
            println!("SUCCESS: Ping returned correct complement.");
            println!("XOR encryption is in sync with firmware.");
        }
        Err(e) => {
            println!();
            println!("FAILED: {e}");
            println!();
            println!("If this is a ping response mismatch, the LFSR");
            println!("implementation may be incorrect. Check:");
            println!("  - crates/saleae-proto/src/obfuscation.rs");
            println!("  - The LFSR taps (bits 1, 21, 22)");
            println!("  - The advance_seed iteration count formula");
            return Err(e.into());
        }
    }

    Ok(())
}

/// Program the FPGA with a bitstream file.
///
/// Reads the bitstream from disk, opens the device, performs the
/// ReseedDevice + Ping handshake, then programs the FPGA and runs
/// the post-init sequence.
pub fn program_fpga(bitstream_path: &str, skip_post_init: bool) -> Result<()> {
    // Read bitstream file.
    let bitstream = std::fs::read(bitstream_path)
        .with_context(|| format!("failed to read bitstream file: {bitstream_path}"))?;

    println!(
        "Bitstream loaded: {} bytes from {bitstream_path}",
        bitstream.len()
    );
    println!();

    // Discover device and check boot state.
    let device =
        saleae_usb::discovery::find_first_saleae().context("failed to find Saleae device")?;

    println!("Found device: {device}");
    println!("  Boot state: {}", device.boot_state);
    if let Some(speed) = device.usb_speed {
        println!("  USB speed:  {speed}");
    }
    println!();

    match device.boot_state {
        BootState::FirmwareRunning => {
            println!("Firmware is running. Proceeding with FPGA programming...");
        }
        BootState::Bootloader => {
            bail!(
                "Device is in bootloader mode. Firmware must be uploaded first.\n\
                 Run: saleae-cli upload-firmware --firmware <path>"
            );
        }
        BootState::Unknown(bcd) => {
            bail!("Device has unexpected bcdDevice value {bcd:#06x}.");
        }
    }

    // Determine encryption mode from device type.
    let encryption_mode = if device.device_type.is_pro() {
        saleae_proto::obfuscation::EncryptionMode::XorAll
    } else if device.device_type.is_fx3() {
        saleae_proto::obfuscation::EncryptionMode::XorFirmwareOnly
    } else {
        saleae_proto::obfuscation::EncryptionMode::None
    };

    // Open device and create command channel.
    let usb_device =
        saleae_usb::discovery::open_first_device().context("failed to open Saleae device")?;

    let transport = saleae_usb::nusb_backend::NusbTransport::open(usb_device)
        .context("failed to claim USB interface")?;

    let reads_write_ack = device.device_type.is_pro();
    let mut channel =
        saleae_driver::ep1::Ep1CommandChannel::new(Box::new(transport), encryption_mode, reads_write_ack);

    // Initialize command channel (ReseedDevice + Ping).
    println!("Initializing command channel (ReseedDevice + Ping)...");
    channel
        .initialize()
        .context("command channel initialization failed")?;
    println!("Command channel initialized.");
    println!();

    // Program FPGA.
    let profile = device
        .device_type
        .profile()
        .context("no device profile for this device type")?;
    let usb_speed = device
        .usb_speed
        .unwrap_or_else(|| profile.default_forced_usb_speed());

    println!("Programming FPGA ({} bytes)...", bitstream.len());
    saleae_driver::fpga::program_fpga(&mut channel, &bitstream, profile, usb_speed)
        .context("FPGA programming failed")?;
    println!("FPGA programming verified (readback = 0xAA).");
    println!();

    // Post-init sequence.
    if skip_post_init {
        println!("Skipping post-init sequence (--skip-post-init).");
    } else {
        // Determine FPGA type from EEPROM hardware revision.
        let hw_rev = saleae_driver::fpga::read_hw_revision(&mut channel)
            .context("failed to read hardware revision from EEPROM")?;
        let is_lattice = hw_rev.is_lattice();

        println!("Hardware revision: {hw_rev}");
        println!("Running post-init sequence (Lattice: {is_lattice})...");
        saleae_driver::fpga::fpga_post_init(&mut channel, is_lattice)
            .context("FPGA post-init failed")?;
        println!("Post-init complete.");
    }

    println!();
    println!("FPGA is programmed and initialized. Device should be ready.");

    Ok(())
}

/// Run CommonDeviceInit: ADC initialization, JESD link, EEPROM device ID.
///
/// Optionally programs the FPGA first if a bitstream path is provided.
pub fn device_init(bitstream_path: Option<&str>, skip_fpga: bool) -> Result<()> {
    // Discover device and check boot state.
    let device =
        saleae_usb::discovery::find_first_saleae().context("failed to find Saleae device")?;

    println!("Found device: {device}");
    println!("  Boot state: {}", device.boot_state);
    if let Some(speed) = device.usb_speed {
        println!("  USB speed:  {speed}");
    }
    println!();

    match device.boot_state {
        BootState::FirmwareRunning => {
            println!("Firmware is running. Proceeding...");
        }
        BootState::Bootloader => {
            bail!(
                "Device is in bootloader mode. Firmware must be uploaded first.\n\
                 Run: saleae-cli upload-firmware --firmware <path>"
            );
        }
        BootState::Unknown(bcd) => {
            bail!("Device has unexpected bcdDevice value {bcd:#06x}.");
        }
    }

    // Determine encryption mode from device type.
    let encryption_mode = if device.device_type.is_pro() {
        saleae_proto::obfuscation::EncryptionMode::XorAll
    } else if device.device_type.is_fx3() {
        saleae_proto::obfuscation::EncryptionMode::XorFirmwareOnly
    } else {
        saleae_proto::obfuscation::EncryptionMode::None
    };

    // Open device and create command channel.
    let usb_device =
        saleae_usb::discovery::open_first_device().context("failed to open Saleae device")?;

    let transport = saleae_usb::nusb_backend::NusbTransport::open(usb_device)
        .context("failed to claim USB interface")?;

    let reads_write_ack = device.device_type.is_pro();
    let mut channel =
        saleae_driver::ep1::Ep1CommandChannel::new(Box::new(transport), encryption_mode, reads_write_ack);

    // Initialize command channel (ReseedDevice + Ping).
    println!("Initializing command channel (ReseedDevice + Ping)...");
    channel
        .initialize()
        .context("command channel initialization failed")?;
    println!("Command channel initialized.");
    println!();

    // Get device profile and USB speed for FPGA programming.
    let profile = device
        .device_type
        .profile()
        .context("no device profile for this device type")?;
    let usb_speed = device
        .usb_speed
        .unwrap_or_else(|| profile.default_forced_usb_speed());

    // Optionally program FPGA.
    if !skip_fpga {
        let bs_path = bitstream_path.ok_or_else(|| {
            anyhow::anyhow!(
                "No bitstream provided and --skip-fpga not set.\n\
                 Either provide --bitstream <path> or use --skip-fpga \
                 if FPGA is already programmed."
            )
        })?;

        let bitstream = std::fs::read(bs_path)
            .with_context(|| format!("failed to read bitstream file: {bs_path}"))?;

        println!(
            "Programming FPGA ({} bytes from {bs_path})...",
            bitstream.len()
        );
        saleae_driver::fpga::program_fpga(&mut channel, &bitstream, profile, usb_speed)
            .context("FPGA programming failed")?;
        println!("FPGA programming verified.");
        println!();

        // Post-init: determine FPGA type from EEPROM hardware revision.
        let hw_rev = saleae_driver::fpga::read_hw_revision(&mut channel)
            .context("failed to read hardware revision from EEPROM")?;
        let is_lattice = hw_rev.is_lattice();
        println!("Hardware revision: {hw_rev}");
        println!("Running FPGA post-init (Lattice: {is_lattice})...");
        saleae_driver::fpga::fpga_post_init(&mut channel, is_lattice)
            .context("FPGA post-init failed")?;
        println!("Post-init complete.");
        println!();
    } else {
        println!("Skipping FPGA programming (--skip-fpga).");
        println!();
    }

    // Determine FPGA type from EEPROM hardware revision.
    let hw_rev = saleae_driver::fpga::read_hw_revision(&mut channel)
        .context("failed to read hardware revision from EEPROM")?;
    let is_lattice = hw_rev.is_lattice();
    println!("Hardware revision: {hw_rev}");
    println!("Running CommonDeviceInit (Lattice: {is_lattice})...");
    let result = saleae_driver::fpga::common_device_init(&mut channel, is_lattice)
        .context("CommonDeviceInit failed")?;

    println!();
    println!("CommonDeviceInit results:");
    println!(
        "  Device ID:   {:#018x} ({})",
        result.device_id, result.device_id
    );
    println!("  JESD locked: {}", result.jesd_locked);
    println!("  DDR ready:   {}", result.ddr_ready);
    println!("  DCM locked:  {}", result.dcm_locked);
    println!();

    // Print calibration URL for convenience.
    println!(
        "Calibration URL: https://downloads.saleae.com/calibration/{:x}.cal",
        result.device_id
    );

    Ok(())
}

/// Full initialization: firmware upload (with auto-recovery) + FPGA + ADC + capture.
///
/// Uses `ensure_device_ready` to handle firmware upload and EP1 recovery
/// automatically. Works from any device state: bootloader, firmware running
/// with live EP1, or firmware running with dead EP1.
/// Try multiple bitstream files and return the first one that programs successfully.
///
/// For each bitstream: ResetFpga, program, verify (r0x7F=0xAA readback).
/// Returns the working bitstream data and its path.
fn try_bitstreams(
    channel: &mut saleae_driver::ep1::Ep1CommandChannel,
    paths: &[&str],
    profile: &saleae_proto::device::DeviceProfile,
    usb_speed: saleae_proto::device::UsbSpeed,
) -> Result<(Vec<u8>, String)> {
    for path in paths {
        let bitstream = match std::fs::read(path) {
            Ok(data) => data,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => {
                println!("  Skipping {}: {}", path, e);
                continue;
            }
        };

        println!("  Trying {} ({} bytes)...", path, bitstream.len());
        match saleae_driver::fpga::program_fpga(channel, &bitstream, profile, usb_speed) {
            Ok(()) => {
                println!("    Verified!");
                return Ok((bitstream, path.to_string()));
            }
            Err(e) => {
                println!("    Programming failed: {e}");
                continue;
            }
        }
    }

    bail!("No compatible bitstream found. Tried: {:?}", paths);
}

pub fn full_init(
    firmware_path: &str,
    bitstream_path: &str,
    auto_bitstream: bool,
    cal_file: Option<&str>,
) -> Result<()> {
    // Use ensure_device_ready for firmware upload + EP1 recovery.
    let mut channel = ensure_device_ready(firmware_path)?;

    // Read firmware version string for diagnostics.
    match channel.get_device_info() {
        Ok(info) => println!("Firmware: {info}"),
        Err(e) => println!("GetDeviceInfo failed: {e}"),
    }

    // Re-discover device for profile/speed info.
    let device = saleae_usb::discovery::find_first_saleae().context("device disappeared")?;
    println!();

    let profile = device.device_type.profile().context("no device profile")?;
    let usb_speed = device
        .usb_speed
        .unwrap_or_else(|| profile.default_forced_usb_speed());

    // Step 4: FPGA program + verify
    if auto_bitstream {
        println!("Auto-detecting bitstream...");
        let (_bitstream_data, found_path) =
            try_bitstreams(&mut channel, SLX9_BITSTREAMS, profile, usb_speed)?;
        println!("Using bitstream: {found_path}");
    } else {
        let bitstream = std::fs::read(bitstream_path)
            .with_context(|| format!("failed to read bitstream: {bitstream_path}"))?;
        println!("Programming FPGA ({} bytes)...", bitstream.len());
        saleae_driver::fpga::program_fpga(&mut channel, &bitstream, profile, usb_speed)?;
        println!("FPGA programmed and verified.");
    }
    println!();

    // Read EEPROM device ID (offset 0x08, 8 bytes LE) for calibration file lookup.
    let device_id = channel
        .read_eeprom_device_id()
        .context("failed to read EEPROM device ID")?;
    println!("Device ID: {:#018x} ({:x})", device_id, device_id);
    println!(
        "Calibration URL: https://downloads.saleae.com/calibration/{:x}.cal",
        device_id
    );
    println!();

    // Step 5: AD9637 ADC init via driver module
    println!("AD9637 ADC init...");

    // Read calibration from EEPROM (offset 0x10, 16 bytes)
    let cal_data = channel.read_version(&[0x33, 0x81, 0x10, 0x00, 0x10, 0x00])?;
    println!(
        "  Calibration: {}",
        cal_data
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<Vec<_>>()
            .join(" ")
    );

    let dcm_result =
        saleae_driver::adc::init_ad9637(&mut channel, &cal_data).context("AD9637 init failed")?;
    println!(
        "  DCM locked: {} (r0x40={:#04x})",
        dcm_result.locked, dcm_result.status_40
    );
    println!();

    // Re-init EP1 (reseed + ping for next phase)
    channel.initialize().context("EP1 re-init failed")?;
    channel.write_registers(&[(0x0f, 0x00), (0x10, 0x0f), (0x11, 0x00)])?;
    println!("Channel enables set.");

    // Step 6: Pre-capture ADC power on
    println!("Pre-capture ADC power on...");
    let capture_dcm = saleae_driver::adc::power_on_for_capture(&mut channel)
        .context("ADC power-on for capture failed")?;
    println!(
        "  DCM locked for capture (r0x40={:#04x})",
        capture_dcm.status_40
    );
    println!();

    // Compute capture registers from config (12.5 MSa/s mixed, all channels).
    let capture_config =
        saleae_driver::capture::CaptureConfig::pro8(12_500_000, (0..8).collect(), (0..8).collect());

    println!("Starting capture (2 seconds)...");
    let result = saleae_driver::capture::run_capture(
        &mut channel,
        &capture_config,
        Duration::from_secs(2),
        16384,
    )
    .context("capture failed")?;

    warn_if_partial(&result);
    let raw_data = &result.raw_data;
    let total_bytes = result.bytes_read();
    println!(
        "Capture stopped. Read {} bytes ({:.1} KB/s)",
        total_bytes,
        result.throughput_kbps(),
    );

    // Compute matching disaggregator settings from the same capture config.
    println!();
    println!("Decoding captured data...");

    use saleae_driver::disaggregator::Disaggregator;

    let settings = capture_config.disaggregator_settings();

    println!(
        "  Cycle size: {} bytes (digital={}, analog={})",
        settings.full_cycle_size(),
        settings.digital_group_bytes(),
        settings.analog_group_bytes(),
    );

    // Truncate to a whole number of cycles.
    let cycle_size = settings.full_cycle_size();
    let usable_bytes = (total_bytes / cycle_size) * cycle_size;
    let num_cycles = usable_bytes / cycle_size;
    if usable_bytes < total_bytes {
        println!(
            "  Truncated {} trailing bytes ({} complete cycles)",
            total_bytes - usable_bytes,
            num_cycles,
        );
    }

    let disagg = Disaggregator::new(settings)?;
    let output = disagg.process(&raw_data[..usable_bytes])?;

    // Print digital channel summary.
    println!();
    println!("Digital channels ({} channels):", output.digital.len());
    for (ch, data) in output.digital.iter().enumerate() {
        // Count ones and zeros across all bytes.
        let total_bits = data.len() * 8;
        let ones: u32 = data.iter().map(|b| b.count_ones()).sum();
        let zeros = total_bits as u32 - ones;
        // Show first few bytes hex.
        let preview: String = data
            .iter()
            .take(8)
            .map(|b| format!("{b:02x}"))
            .collect::<Vec<_>>()
            .join(" ");
        println!(
            "  CH{ch}: {} samples, {ones} high / {zeros} low ({:.1}% high) | {preview}...",
            total_bits,
            ones as f64 / total_bits as f64 * 100.0,
        );
    }

    // Print analog channel summary (raw ADC codes).
    println!();
    println!(
        "Analog channels ({} channels, raw ADC codes):",
        output.analog.len()
    );
    for (ch, data) in output.analog.iter().enumerate() {
        if data.is_empty() {
            println!("  CH{ch}: no samples");
            continue;
        }
        let min = *data.iter().min().unwrap();
        let max = *data.iter().max().unwrap();
        let mean = data.iter().map(|&s| s as f64).sum::<f64>() / data.len() as f64;
        let preview: String = data
            .iter()
            .take(8)
            .map(|s| format!("{s}"))
            .collect::<Vec<_>>()
            .join(", ");
        println!(
            "  CH{ch}: {} samples, min={min} max={max} mean={mean:.1} | {preview}...",
            data.len(),
        );
    }

    // Apply analog calibration if a .cal file is available.
    if !output.analog.is_empty() {
        match resolve_cal_file(cal_file, Some(device_id)) {
            Ok(cal_path) => {
                println!("Loading calibration from {}", cal_path.display());
                match std::fs::read(&cal_path) {
                    Ok(cal_bytes) => {
                        match saleae_proto::calibration::parse_calibration(&cal_bytes) {
                            Ok(cal) => {
                                let analog_indices: Vec<u8> =
                                    capture_config.analog_channels.clone();
                                match saleae_driver::calibrate::apply_calibration(
                                    &output,
                                    &cal,
                                    &analog_indices,
                                    capture_config.sample_rate,
                                    -2048, // Logic Pro 8: 12-bit signed ADC
                                    2047,
                                ) {
                                    Ok(cal_output) => {
                                        println!();
                                        println!("Calibrated analog voltages:");
                                        for ch in &cal_output.channels {
                                            if let Some((min, max, mean)) = ch.stats() {
                                                println!(
                                                    "  CH{}: {} samples, min={min:.4}V max={max:.4}V mean={mean:.4}V",
                                                    ch.index, ch.voltages.len(),
                                                );
                                            } else {
                                                println!("  CH{}: no samples", ch.index);
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        println!();
                                        println!("Calibration skipped: {e}");
                                    }
                                }
                            }
                            Err(e) => {
                                println!();
                                println!("Calibration skipped: failed to parse .cal file: {e}");
                            }
                        }
                    }
                    Err(e) => {
                        println!();
                        println!(
                            "Calibration skipped: failed to read {}: {e}",
                            cal_path.display()
                        );
                    }
                }
            }
            Err(e) => {
                println!();
                println!("Calibration skipped: {e}");
            }
        }
    }

    // Power down
    saleae_driver::adc::power_down(&mut channel).context("ADC power down failed")?;
    println!("Device powered down.");

    Ok(())
}

/// Start a digital capture.
///
/// Performs the full sequence from any device state:
/// 1. Firmware upload + EP1 init (auto-recovery via ensure_device_ready)
/// 2. FPGA programming + verify
/// 3. AD9637 ADC init + DCM lock + calibration
/// 4. Pre-capture ADC power-on
/// 5. Write capture parameters (pcap Phase 7 values)
/// 6. EP2 read loop
/// 7. Teardown (stop capture, power down ADC)
pub fn capture_digital(
    sample_rate: u64,
    duration: f64,
    channels: &str,
    output: Option<&str>,
    format: &str,
    firmware_path: &str,
    bitstream_path: &str,
) -> Result<()> {
    let channel_list = parse_channel_list(channels)?;

    // Build capture configuration.
    let duration_samples = if duration > 0.0 {
        (sample_rate as f64 * duration) as u64
    } else {
        0
    };

    let config = saleae_driver::capture::CaptureConfig {
        duration_samples,
        ..saleae_driver::capture::CaptureConfig::pro8(sample_rate, channel_list.clone(), vec![])
    };

    config.validate().context("invalid capture config")?;

    println!("Digital capture configuration:");
    println!("  Sample rate:       {} Hz", config.sample_rate);
    println!("  Channels:          {:?}", config.digital_channels);
    println!("  Duration samples:  {}", config.duration_samples);
    println!("  Voltage threshold: {} V", config.voltage_threshold);
    println!("  Firmware:          {firmware_path}");
    println!("  Bitstream:         {bitstream_path}");
    println!();

    // Parse export format early so we fail fast on bad --format.
    let export_format = parse_export_format(format)?;

    if duration <= 0.0 {
        bail!("Duration must be > 0 for hardware capture (Ctrl-C not yet supported).");
    }

    // Parse bitstream upfront so we fail fast on bad path.
    let bitstream = std::fs::read(bitstream_path)
        .with_context(|| format!("failed to read bitstream: {bitstream_path}"))?;

    // --- Step 1: Firmware + EP1 init (auto-recovery) ---
    println!("Ensuring device is ready (firmware + EP1)...");
    let mut channel = ensure_device_ready(firmware_path)?;

    // Re-discover device for profile/speed info.
    let device =
        saleae_usb::discovery::find_first_saleae().context("device disappeared after init")?;
    let profile = device
        .device_type
        .profile()
        .context("no device profile for this device type")?;
    let usb_speed = device
        .usb_speed
        .unwrap_or_else(|| profile.default_forced_usb_speed());

    println!("Device ready: {device}");
    println!();

    // --- Step 2: FPGA programming ---
    // program_fpga includes reset + bitstream upload + r0x7F verify.
    println!("Programming FPGA ({} bytes)...", bitstream.len());
    saleae_driver::fpga::program_fpga(&mut channel, &bitstream, profile, usb_speed)
        .context("FPGA programming failed")?;
    println!("FPGA programmed and verified.");
    println!();

    // --- Step 3: AD9637 ADC init ---
    println!("AD9637 ADC init...");
    let cal_data = channel
        .read_version(&[0x33, 0x81, 0x10, 0x00, 0x10, 0x00])
        .context("failed to read EEPROM calibration")?;
    println!(
        "  Calibration: {}",
        cal_data
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<Vec<_>>()
            .join(" ")
    );

    let dcm_result =
        saleae_driver::adc::init_ad9637(&mut channel, &cal_data).context("AD9637 init failed")?;
    println!(
        "  DCM locked: {} (r0x40={:#04x})",
        dcm_result.locked, dcm_result.status_40
    );
    println!();

    // --- Step 4: Re-init EP1 + channel enables ---
    channel
        .initialize()
        .context("EP1 re-initialization failed")?;
    channel.write_registers(&[(0x0f, 0x00), (0x10, 0x0f), (0x11, 0x00)])?;
    println!("Channel enables set (r0x0f=0x00, r0x10=0x0f, r0x11=0x00).");
    println!();

    // --- Step 5: Pre-capture ADC power-on ---
    println!("Pre-capture ADC power-on...");
    let capture_dcm = saleae_driver::adc::power_on_for_capture(&mut channel)
        .context("ADC power-on for capture failed")?;
    println!(
        "  DCM locked for capture (r0x40={:#04x})",
        capture_dcm.status_40
    );
    println!();

    // --- Step 6-8: Capture pipeline (configure + start + stream + stop) ---
    println!("Running capture for {duration:.1}s...");
    let result = saleae_driver::capture::run_capture(
        &mut channel,
        &config,
        Duration::from_secs_f64(duration),
        16 * 1024,
    )
    .context("capture failed")?;

    // --- Step 9: Teardown ---
    // Power down ADC
    match saleae_driver::adc::power_down(&mut channel) {
        Ok(()) => println!("  ADC powered down."),
        Err(e) => println!("  WARNING: ADC power-down failed: {e}"),
    }
    println!();

    // --- Summary ---
    warn_if_partial(&result);
    println!("Capture complete:");
    println!("  Duration:    {:.2}s", result.elapsed.as_secs_f64());
    println!("  Total reads: {}", result.read_count);
    println!("  Total bytes: {}", result.bytes_read());
    if result.elapsed.as_secs_f64() > 0.0 {
        println!("  Throughput:  {:.1} Mbps", result.throughput_mbps());
    }

    let first = result.first_bytes(256);
    if !first.is_empty() {
        println!();
        println!("First {} bytes of EP2 data:", first.len());
        print_hex_dump(first);
    }

    // --- Export captured data ---
    if let Some(output_dir) = output {
        let export_config = saleae_proto::ExportConfig {
            output_dir: std::path::PathBuf::from(output_dir),
            format: export_format,
            sample_rate,
            channels: channel_list.clone(),
        };
        println!();
        println!("Export configuration:");
        println!("  Output dir: {}", export_config.output_dir.display());
        println!("  Format:     {:?}", export_config.format);

        // Disaggregate raw EP2 data into per-channel packed digital samples.
        let disaggregator = saleae_proto::Disaggregator::new(
            profile,
            &channel_list,
            &[], // no analog channels
            sample_rate,
            0, // no analog rate
        )
        .context("failed to create disaggregator")?;

        let cycle_size = disaggregator.cycle_bytes();
        let usable_len = if cycle_size > 0 {
            result.raw_data.len() - (result.raw_data.len() % cycle_size)
        } else {
            0
        };

        if usable_len > 0 {
            let disagg = disaggregator
                .process_chunk(&result.raw_data[..usable_len])
                .context("failed to disaggregate capture data")?;

            match saleae_proto::DigitalExporter::new(&export_config)
                .context("failed to create digital exporter")?
            {
                saleae_proto::DigitalExporter::Csv(mut w) => {
                    w.write_packed_samples(&disagg.digital_samples)
                        .context("failed to write digital CSV samples")?;
                    w.finish()
                        .context("failed to finalize digital CSV exporter")?;
                }
                saleae_proto::DigitalExporter::Binary(mut w) => {
                    w.write_samples(&disagg.digital_samples)
                        .context("failed to write digital binary samples")?;
                    w.finish()
                        .context("failed to finalize digital binary exporter")?;
                }
            }

            let trimmed = result.raw_data.len() - usable_len;
            println!(
                "  Exported {} bytes of sample data ({} channels, {} bytes trimmed for alignment)",
                usable_len,
                channel_list.len(),
                trimmed,
            );
        } else {
            println!("  WARNING: no usable capture data to export (0 bytes).");
        }

        println!(
            "  Output written to: {}",
            export_config.output_dir.display()
        );
    }

    Ok(())
}

/// Quick capture: minimal sequence to get EP2 data flowing.
///
/// For digital-only (analog_mask=0x00): skips all ADC/DCM init.
/// For mixed (analog_mask!=0x00): runs ADC init with best-effort DCM lock.
///
/// This is the simplest path to seeing EP2 data. When --firmware is provided,
/// uses ensure_device_ready for auto-recovery. Otherwise assumes firmware is
/// already running.
pub fn capture_quick(
    duration: f64,
    digital_mask_str: &str,
    analog_mask_str: &str,
    bitstream_path: Option<&str>,
    buf_size: usize,
    output_path: Option<&str>,
    firmware_path: Option<&str>,
) -> Result<()> {
    // Parse hex masks.
    let digital_mask = parse_hex_u8(digital_mask_str)
        .with_context(|| format!("invalid digital mask: {digital_mask_str}"))?;
    let analog_mask = parse_hex_u8(analog_mask_str)
        .with_context(|| format!("invalid analog mask: {analog_mask_str}"))?;

    let is_digital_only = analog_mask == 0x00;

    println!("capture-quick configuration:");
    println!("  Duration:      {duration:.1}s");
    println!("  Digital mask:  {digital_mask:#04x}");
    println!("  Analog mask:   {analog_mask:#04x}");
    println!(
        "  Mode:          {}",
        if is_digital_only {
            "digital-only"
        } else {
            "mixed analog+digital"
        }
    );
    println!(
        "  Bitstream:     {}",
        bitstream_path.unwrap_or("(skip FPGA programming)")
    );
    println!("  EP2 buf size:  {buf_size}");
    if let Some(p) = output_path {
        println!("  Output file:   {p}");
    }
    if let Some(fw) = firmware_path {
        println!("  Firmware:      {fw} (auto-recovery enabled)");
    }
    println!();

    if duration <= 0.0 {
        bail!("Duration must be > 0.");
    }

    // Step 1: Get device ready.
    let mut channel = if let Some(fw_path) = firmware_path {
        // Auto-recovery: handles bootloader, dead EP1, etc.
        println!("Ensuring device is ready (firmware + EP1)...");
        ensure_device_ready(fw_path)?
    } else {
        // Manual mode: device must already have firmware running.
        let device =
            saleae_usb::discovery::find_first_saleae().context("failed to find Saleae device")?;

        match device.boot_state {
            BootState::FirmwareRunning => {}
            BootState::Bootloader => {
                bail!("Device in bootloader mode. Upload firmware first, or use --firmware for auto-recovery.");
            }
            BootState::Unknown(bcd) => {
                bail!("Unexpected bcdDevice {bcd:#06x}");
            }
        }

        println!("Found device: {device}");
        if let Some(speed) = device.usb_speed {
            println!("  USB speed: {speed}");
        }

        let encryption_mode = if device.device_type.is_pro() {
            saleae_proto::obfuscation::EncryptionMode::XorAll
        } else if device.device_type.is_fx3() {
            saleae_proto::obfuscation::EncryptionMode::XorFirmwareOnly
        } else {
            saleae_proto::obfuscation::EncryptionMode::None
        };

        let usb_device =
            saleae_usb::discovery::open_first_device().context("failed to open device")?;
        let transport = saleae_usb::nusb_backend::NusbTransport::open(usb_device)
            .context("failed to claim USB interface")?;
        let reads_write_ack = device.device_type.is_pro();
        let mut ch =
            saleae_driver::ep1::Ep1CommandChannel::new(Box::new(transport), encryption_mode, reads_write_ack);

        println!("Initializing EP1 (reseed + ping)...");
        ch.initialize().context("EP1 initialization failed")?;
        println!("EP1 initialized.");
        ch
    };
    println!();

    // Step 3: Optionally program FPGA.
    if let Some(bs_path) = bitstream_path {
        let bitstream = std::fs::read(bs_path)
            .with_context(|| format!("failed to read bitstream: {bs_path}"))?;

        // Re-discover device for profile/speed info.
        let device = saleae_usb::discovery::find_first_saleae().context("device disappeared")?;
        let profile = device.device_type.profile().context("no device profile")?;
        let usb_speed = device
            .usb_speed
            .unwrap_or_else(|| profile.default_forced_usb_speed());

        // program_fpga includes reset + bitstream upload + r0x7F verify.
        println!("Programming FPGA ({} bytes)...", bitstream.len());
        saleae_driver::fpga::program_fpga(&mut channel, &bitstream, profile, usb_speed)
            .context("FPGA programming failed")?;
        println!("FPGA programmed and verified.");
        println!();
    } else {
        // Verify FPGA is already programmed by reading r0x7F.
        println!("Checking if FPGA is programmed...");
        channel.write_registers(&[(0x7F, 0xAA)])?;
        let verify = channel.read_registers(&[0x7F])?;
        if verify[0] != 0xAA {
            println!(
                "WARNING: FPGA may not be programmed (r0x7F readback = {:#04x}).",
                verify[0]
            );
            println!("Consider using --bitstream to program it.");
            println!();
        } else {
            println!("FPGA appears programmed (r0x7F readback = 0xAA).");
            println!();
        }
    }

    // Step 4: ADC init (only for mixed mode).
    if !is_digital_only {
        println!("Mixed mode: running ADC init...");

        // Read calibration from EEPROM (offset 0x10, 16 bytes).
        let cal_data = channel
            .read_version(&[0x33, 0x81, 0x10, 0x00, 0x10, 0x00])
            .context("failed to read EEPROM calibration")?;
        println!(
            "  Calibration: {}",
            cal_data
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<Vec<_>>()
                .join(" ")
        );

        match saleae_driver::adc::init_ad9637(&mut channel, &cal_data) {
            Ok(dcm_result) => {
                println!(
                    "  ADC init OK: DCM locked={}, r0x40={:#04x}",
                    dcm_result.locked, dcm_result.status_40
                );
            }
            Err(e) => {
                println!("  WARNING: ADC init failed: {e}");
                println!("  Continuing anyway (best-effort)...");
            }
        }

        // Re-init EP1 after ADC init (the binary does this).
        channel
            .initialize()
            .context("EP1 re-init after ADC failed")?;

        // Channel enables.
        channel.write_registers(&[(0x0f, 0x00), (0x10, 0x0f), (0x11, 0x00)])?;
        println!("  Channel enables set.");

        // Pre-capture ADC power on.
        match saleae_driver::adc::power_on_for_capture(&mut channel) {
            Ok(dcm_result) => {
                println!("  ADC power-on OK: r0x40={:#04x}", dcm_result.status_40);
            }
            Err(e) => {
                println!("  WARNING: ADC power-on failed: {e}");
                println!("  Continuing anyway (best-effort for capture data)...");
            }
        }
        println!();
    } else {
        println!("Digital-only mode: skipping ADC/DCM init entirely.");
        println!();
    }

    // Step 5: Write capture registers (computed from mask config).
    let digital_channels: Vec<u8> = (0..8).filter(|&ch| digital_mask & (1 << ch) != 0).collect();
    let analog_channels: Vec<u8> = (0..8).filter(|&ch| analog_mask & (1 << ch) != 0).collect();

    let quick_config =
        saleae_driver::capture::CaptureConfig::pro8(12_500_000, digital_channels, analog_channels);

    // Step 5-8: Capture pipeline (configure + start + stream + stop).
    println!("Running capture for {duration:.1}s...");
    let result = saleae_driver::capture::run_capture(
        &mut channel,
        &quick_config,
        Duration::from_secs_f64(duration),
        buf_size,
    )
    .context("capture failed")?;

    // Power down ADC if we powered it on.
    if !is_digital_only {
        match saleae_driver::adc::power_down(&mut channel) {
            Ok(()) => println!("  ADC powered down."),
            Err(e) => println!("  WARNING: ADC power-down failed: {e}"),
        }
    }
    println!();

    // Write raw data to output file if requested.
    if let Some(p) = output_path {
        use std::io::Write;
        let mut f = std::fs::File::create(p)
            .with_context(|| format!("failed to create output file: {p}"))?;
        f.write_all(&result.raw_data)
            .with_context(|| format!("failed to write output file: {p}"))?;
        println!("  Raw data written to: {p} ({} bytes)", result.bytes_read());
    }

    // Summary.
    warn_if_partial(&result);
    println!("Capture complete:");
    println!("  Duration:    {:.2}s", result.elapsed.as_secs_f64());
    println!("  Total reads: {}", result.read_count);
    println!("  Total bytes: {}", result.bytes_read());
    if result.elapsed.as_secs_f64() > 0.0 {
        println!(
            "  Throughput:  {:.1} Mbps ({:.0} KB/s)",
            result.throughput_mbps(),
            result.throughput_kbps(),
        );
    }

    let first = result.first_bytes(256);
    if !first.is_empty() {
        println!();
        println!("First {} bytes of EP2 data:", first.len());
        print_hex_dump(first);
    }

    Ok(())
}

/// Print a warning if the capture ended early due to EP2 error.
fn warn_if_partial(result: &saleae_driver::capture::CaptureResult) {
    if !result.completed_normally {
        println!("WARNING: capture ended early due to EP2 read error (data may be partial)");
    }
}

/// Print a hex dump with ASCII sidebar (16 bytes per line).
fn print_hex_dump(data: &[u8]) {
    for (i, chunk) in data.chunks(16).enumerate() {
        print!("  {:04x}:", i * 16);
        for b in chunk {
            print!(" {b:02x}");
        }
        // Pad short lines.
        for _ in 0..(16 - chunk.len()) {
            print!("   ");
        }
        print!("  ");
        for b in chunk {
            let c = if b.is_ascii_graphic() || *b == b' ' {
                *b as char
            } else {
                '.'
            };
            print!("{c}");
        }
        println!();
    }
}

/// Parse a hex string like "0xff" or "ff" or decimal "255" into a u8.
fn parse_hex_u8(s: &str) -> Result<u8> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u8::from_str_radix(hex, 16).with_context(|| format!("invalid hex u8: {s}"))
    } else {
        // Try decimal first, then hex.
        s.parse::<u8>()
            .or_else(|_| u8::from_str_radix(s, 16).with_context(|| format!("invalid u8: {s}")))
            .with_context(|| format!("invalid u8: {s}"))
    }
}

/// Start an analog capture.
///
/// Validates the analog capture configuration, loads calibration data,
/// creates per-channel AnalogProcessors (VoltageTransform + BiquadFilter),
/// and prints the capture plan.
///
/// Hardware streaming is blocked (device stuck with red LED), so this
/// performs a dry-run: validates everything, prints diagnostics, and exits.
pub fn capture_analog(
    sample_rate: u64,
    duration: f64,
    channels: &str,
    output: Option<&str>,
    format: &str,
    cal_file: Option<&str>,
) -> Result<()> {
    let channel_list = parse_channel_list(channels)?;

    // Build analog capture configuration.
    let duration_samples = if duration > 0.0 {
        (sample_rate as f64 * duration) as u64
    } else {
        0
    };

    let config = saleae_driver::capture::AnalogCaptureConfig {
        sample_rate,
        analog_channels: channel_list.clone(),
        duration_samples,
        samples_per_chunk: saleae_driver::capture::DEFAULT_SAMPLES_PER_CHUNK,
        stats_enabled: true,
    };

    // Validate configuration.
    config.validate().context("invalid analog capture config")?;

    println!("Analog capture configuration:");
    println!("  Sample rate:       {} Hz", config.sample_rate);
    println!("  Channels:          {:?}", config.analog_channels);
    println!("  Duration samples:  {}", config.duration_samples);
    println!("  Stats enabled:     {}", config.stats_enabled);
    println!("  Samples per chunk: {}", config.samples_per_chunk);
    println!(
        "  Sample data bytes: {} bytes/chunk",
        config.sample_data_bytes_per_chunk()
    );
    println!();

    // Try to load calibration data from local file.
    // Use explicit --cal-file if given, otherwise search common directories.
    let cal_result = match resolve_cal_file(cal_file, None) {
        Ok(path) => {
            println!("Loading calibration from {}", path.display());
            saleae_driver::calibration::load_calibration_file(&path)
                .with_context(|| format!("failed to load {}", path.display()))
        }
        Err(_) => {
            // Fall back to searching re/ directory for any .cal file.
            let cal_dir = std::path::PathBuf::from("re");
            find_and_load_calibration(&cal_dir)
        }
    };

    match cal_result {
        Ok(cal) => {
            println!("Calibration data loaded:");
            println!("  Device ID:        {:#018x}", cal.device_id);
            println!("  Device type:      {}", cal.device_type);
            println!("  Calibration date: {}", cal.calibration_date);
            println!("  Voltage ranges:   {} channels", cal.voltage_ranges.len());
            println!(
                "  Cal sets:         {} (channel, rate) combos",
                cal.calibration_sets.len()
            );
            println!();

            // Create per-channel AnalogProcessors.
            println!("Per-channel processor setup:");
            for &ch in &channel_list {
                match create_channel_processor(&cal, ch, sample_rate) {
                    Ok(proc) => {
                        let vr = cal.find_voltage_range(ch).unwrap();
                        println!("  Channel {ch}:");
                        println!(
                            "    Voltage range: [{:.3}, {:.3}] V",
                            vr.min_voltage, vr.max_voltage
                        );
                        println!(
                            "    Transform:     scale={:.6}, offset={:.6}",
                            proc.transform().scale,
                            proc.transform().offset
                        );
                        println!(
                            "    IIR filter:    {} biquad sections",
                            proc.filter().num_sections()
                        );
                    }
                    Err(e) => {
                        println!("  Channel {ch}: FAILED - {e}");
                    }
                }
            }
            println!();
        }
        Err(e) => {
            println!("No calibration data available: {e}");
            println!("  Without calibration, analog output would be uncalibrated.");
            println!("  Place a .cal file in re/ or download with device ID.");
            println!();
        }
    }

    // Print AFE configuration (placeholder).
    println!("AFE configuration (PLACEHOLDER - not validated against hardware):");
    for &ch in &channel_list {
        let afe = saleae_driver::capture::AfeChannelConfig::default();
        println!(
            "  Channel {ch}: attn1={} attn2={} preamp={} ladder={} gain={} dac_offset={} -> reg={:#010x}",
            afe.attn1,
            afe.attn2,
            afe.preamp,
            afe.ladder,
            afe.gain,
            afe.dac_offset,
            afe.to_register_value()
        );
    }
    println!();

    // Export configuration.
    let export_format = parse_export_format(format)?;
    if let Some(output_dir) = output {
        let export_config = saleae_proto::ExportConfig {
            output_dir: std::path::PathBuf::from(output_dir),
            format: export_format,
            sample_rate,
            channels: channel_list.clone(),
        };
        println!("Export configuration:");
        println!("  Output dir: {}", export_config.output_dir.display());
        println!("  Format:     {:?}", export_config.format);
        println!();

        // Validate that the exporter can be created (creates output dir + files).
        let exporter = saleae_proto::AnalogExporter::new(&export_config)
            .context("failed to create analog exporter")?;
        exporter
            .finish()
            .context("failed to finalize analog exporter")?;
        println!(
            "Export directory created: {}",
            export_config.output_dir.display()
        );
        println!();
    }

    bail!(
        "Hardware capture not yet available.\n\
         \n\
         The analog capture pipeline is implemented:\n\
         - AnalogCaptureConfig with validation\n\
         - Analog disaggregation (interleaved int16 -> per-channel)\n\
         - AnalogProcessor (VoltageTransform + IIR biquad calibration filter)\n\
         - AFE channel config (placeholder register packing)\n\
         - CSV and binary export writers\n\
         \n\
         But hardware is currently blocked. Once the device is recovered,\n\
         this command will:\n\
         \n\
         1. Open the device and initialize the command channel\n\
         2. Write AFE config to FPGA registers 0x0054-0x0068\n\
         3. Write capture parameters to FPGA registers\n\
         4. Send StartSampling (0x01) to begin data flow on EP2 IN\n\
         5. Read and parse data chunks (header + int16 samples + stats)\n\
         6. Disaggregate interleaved channels into per-channel int16 streams\n\
         7. Apply VoltageTransform + IIR biquad filter per channel\n\
         8. Export data to --output directory (if specified)\n\
         9. Send StopDataPath (0x86) + StopStream (0x02) to end capture"
    );
}

/// Find and load the first .cal file in a directory.
fn find_and_load_calibration(dir: &std::path::Path) -> Result<saleae_proto::CalibrationData> {
    if !dir.exists() {
        bail!("calibration directory {} does not exist", dir.display());
    }

    let entries = std::fs::read_dir(dir)
        .with_context(|| format!("failed to read directory {}", dir.display()))?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "cal") {
            return saleae_driver::calibration::load_calibration_file(&path)
                .with_context(|| format!("failed to load {}", path.display()));
        }
    }

    bail!("no .cal files found in {}", dir.display());
}

/// Resolve a .cal calibration file path.
///
/// If `explicit_path` is provided, uses that directly.
/// Otherwise, derives the filename from the device ID (`calibration-{id:x}.cal`)
/// and searches these locations in order:
///   1. Current directory
///   2. `re/` subdirectory
///   3. `calibration/` subdirectory
///
/// Returns the path to the .cal file, or an error if not found.
fn resolve_cal_file(
    explicit_path: Option<&str>,
    device_id: Option<u64>,
) -> Result<std::path::PathBuf> {
    // If the user gave an explicit path, use it directly.
    if let Some(path) = explicit_path {
        let p = std::path::PathBuf::from(path);
        if p.exists() {
            return Ok(p);
        }
        bail!("calibration file not found: {}", p.display());
    }

    // Derive filename from device ID.
    let device_id =
        device_id.ok_or_else(|| anyhow::anyhow!("no device ID available to derive .cal path"))?;
    let filename = format!("calibration-{:x}.cal", device_id);

    // Search in common locations.
    let search_dirs: &[&str] = &[".", "re", "calibration"];
    for dir in search_dirs {
        let candidate = std::path::Path::new(dir).join(&filename);
        if candidate.exists() {
            return Ok(candidate);
        }
    }

    // Also try just {id:x}.cal (the format used by download_calibration cache).
    let short_filename = format!("{:x}.cal", device_id);
    for dir in search_dirs {
        let candidate = std::path::Path::new(dir).join(&short_filename);
        if candidate.exists() {
            return Ok(candidate);
        }
    }

    bail!(
        "calibration file not found for device {:x}. Searched for {} and {} in {:?}",
        device_id,
        filename,
        short_filename,
        search_dirs,
    );
}

/// Create an AnalogProcessor for a specific channel and sample rate.
fn create_channel_processor(
    cal: &saleae_proto::CalibrationData,
    channel: u8,
    sample_rate: u64,
) -> Result<saleae_proto::AnalogProcessor> {
    let vr = cal
        .find_voltage_range(channel)
        .ok_or_else(|| anyhow::anyhow!("no voltage range for channel {}", channel))?;

    let set = cal.find_set(channel, sample_rate).ok_or_else(|| {
        anyhow::anyhow!(
            "no calibration set for channel {} at {} Hz",
            channel,
            sample_rate
        )
    })?;

    let transform =
        saleae_proto::VoltageTransform::from_range(vr.min_voltage, vr.max_voltage, -2048, 2047);
    let filter = saleae_proto::BiquadFilter::new(&set.sections);

    Ok(saleae_proto::AnalogProcessor::new(transform, filter))
}

/// Parse an export format string ("csv", "binary", or "sal") into an ExportFormat.
fn parse_export_format(s: &str) -> Result<saleae_proto::ExportFormat> {
    match s.to_lowercase().as_str() {
        "csv" => Ok(saleae_proto::ExportFormat::Csv),
        "binary" | "bin" => Ok(saleae_proto::ExportFormat::Binary),
        "sal" => Ok(saleae_proto::ExportFormat::Sal),
        _ => bail!("unknown export format: {s:?}. Expected \"csv\", \"binary\", or \"sal\"."),
    }
}

/// Monitor USB bus for Saleae device connect/disconnect events.
///
/// Polls every `interval_ms` milliseconds and prints events as they
/// happen. Runs until Ctrl+C terminates the process.
pub fn monitor(interval_ms: u64) -> Result<()> {
    use saleae_driver::device_manager::{DeviceManager, UsbDeviceEnumerator};

    let poll_interval = Duration::from_millis(interval_ms);
    let mut manager = DeviceManager::new(UsbDeviceEnumerator);

    println!("Monitoring USB bus for Saleae devices (poll interval: {interval_ms}ms)");
    println!("Press Ctrl+C to stop.");
    println!();

    // Initial scan to show what's already connected.
    let events = manager.poll().context("initial USB scan failed")?;
    if events.is_empty() {
        println!("No Saleae devices currently connected.");
    } else {
        for event in &events {
            println!("[initial] {event}");
        }
    }
    println!();

    // Poll loop. Ctrl+C terminates the process via the default signal
    // handler, which is the expected UX for a monitoring tool.
    let start = std::time::Instant::now();
    loop {
        std::thread::sleep(poll_interval);

        match manager.poll() {
            Ok(events) => {
                for event in &events {
                    let elapsed = start.elapsed().as_secs_f64();
                    println!("[{elapsed:8.1}s] {event}");
                }
            }
            Err(e) => {
                eprintln!("USB scan error: {e}");
            }
        }
    }
}

/// Parse a comma-separated channel list like "0,1,2,3" into a Vec<u8>.
fn parse_channel_list(s: &str) -> Result<Vec<u8>> {
    s.split(',')
        .map(|c| {
            c.trim()
                .parse::<u8>()
                .with_context(|| format!("invalid channel number: {c:?}"))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_channel_list_valid() {
        let channels = parse_channel_list("0,1,2,3").unwrap();
        assert_eq!(channels, vec![0, 1, 2, 3]);
    }

    #[test]
    fn parse_channel_list_with_spaces() {
        let channels = parse_channel_list("0, 1, 2").unwrap();
        assert_eq!(channels, vec![0, 1, 2]);
    }

    #[test]
    fn parse_channel_list_single() {
        let channels = parse_channel_list("5").unwrap();
        assert_eq!(channels, vec![5]);
    }

    #[test]
    fn parse_channel_list_invalid() {
        assert!(parse_channel_list("a,b,c").is_err());
    }
}
