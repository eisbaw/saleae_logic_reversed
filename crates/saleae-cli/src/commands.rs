//! CLI command implementations.

use std::time::Duration;

use anyhow::{bail, Context, Result};
use saleae_proto::device::BootState;

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

    let mut channel =
        saleae_driver::ep1::Ep1CommandChannel::new(Box::new(transport), encryption_mode);

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

    println!("Bitstream loaded: {} bytes from {bitstream_path}", bitstream.len());
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

    let mut channel =
        saleae_driver::ep1::Ep1CommandChannel::new(Box::new(transport), encryption_mode);

    // Initialize command channel (ReseedDevice + Ping).
    println!("Initializing command channel (ReseedDevice + Ping)...");
    channel
        .initialize()
        .context("command channel initialization failed")?;
    println!("Command channel initialized.");
    println!();

    // Program FPGA.
    println!("Programming FPGA ({} bytes)...", bitstream.len());
    saleae_driver::fpga::program_fpga(&mut channel, &bitstream)
        .context("FPGA programming failed")?;
    println!("FPGA programming verified (readback = 0xAA).");
    println!();

    // Post-init sequence.
    if skip_post_init {
        println!("Skipping post-init sequence (--skip-post-init).");
    } else {
        // Determine if this is a Lattice FPGA.
        // For now, assume Lattice for all current-gen devices (Logic 4/8/Pro 8/Pro 16).
        // Xilinx is only for legacy Logic 16.
        let is_lattice = device.device_type.is_fx3();

        println!(
            "Running post-init sequence (Lattice: {is_lattice})..."
        );
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

    let mut channel =
        saleae_driver::ep1::Ep1CommandChannel::new(Box::new(transport), encryption_mode);

    // Initialize command channel (ReseedDevice + Ping).
    println!("Initializing command channel (ReseedDevice + Ping)...");
    channel
        .initialize()
        .context("command channel initialization failed")?;
    println!("Command channel initialized.");
    println!();

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

        println!("Programming FPGA ({} bytes from {bs_path})...", bitstream.len());
        saleae_driver::fpga::program_fpga(&mut channel, &bitstream)
            .context("FPGA programming failed")?;
        println!("FPGA programming verified.");
        println!();

        // Post-init.
        let is_lattice = device.device_type.is_fx3();
        println!("Running FPGA post-init (Lattice: {is_lattice})...");
        saleae_driver::fpga::fpga_post_init(&mut channel, is_lattice)
            .context("FPGA post-init failed")?;
        println!("Post-init complete.");
        println!();
    } else {
        println!("Skipping FPGA programming (--skip-fpga).");
        println!();
    }

    // Run CommonDeviceInit.
    let is_lattice = device.device_type.is_fx3();
    println!("Running CommonDeviceInit (Lattice: {is_lattice})...");
    let result = saleae_driver::fpga::common_device_init(&mut channel, is_lattice)
        .context("CommonDeviceInit failed")?;

    println!();
    println!("CommonDeviceInit results:");
    println!("  Device ID:   {:#018x} ({})", result.device_id, result.device_id);
    println!("  JESD locked: {}", result.jesd_locked);
    println!("  DDR ready:   {}", result.ddr_ready);
    println!();

    // Print calibration URL for convenience.
    println!(
        "Calibration URL: https://downloads.saleae.com/calibration/{:x}.cal",
        result.device_id
    );

    Ok(())
}

/// Start a digital capture.
///
/// Validates capture configuration and, when hardware is available, runs the
/// full capture pipeline: configure FPGA, start sampling, stream EP2 data,
/// parse chunks, disaggregate channels, and stop.
///
/// Currently performs a dry-run: validates configuration, prints what would
/// happen, and exits. Actual hardware streaming is blocked until the device
/// is recovered (red LED / stuck state).
pub fn capture_digital(sample_rate: u64, duration: f64, channels: &str) -> Result<()> {
    let channel_list = parse_channel_list(channels)?;

    // Build capture configuration.
    let duration_samples = if duration > 0.0 {
        (sample_rate as f64 * duration) as u64
    } else {
        0
    };

    let config = saleae_driver::capture::CaptureConfig {
        sample_rate,
        digital_channels: channel_list,
        duration_samples,
        voltage_threshold: 1.65,
        stats_enabled: true,
        samples_per_chunk: saleae_driver::capture::DEFAULT_SAMPLES_PER_CHUNK,
    };

    // Validate configuration (catches bad sample rates, channel numbers, etc.)
    config
        .validate()
        .map_err(|e| anyhow::anyhow!("invalid capture config: {e}"))?;

    println!("Digital capture configuration:");
    println!("  Sample rate:       {} Hz", config.sample_rate);
    println!("  Channels:          {:?}", config.digital_channels);
    println!("  Duration samples:  {}", config.duration_samples);
    println!("  Voltage threshold: {} V", config.voltage_threshold);
    println!("  Stats enabled:     {}", config.stats_enabled);
    println!("  Samples per chunk: {}", config.samples_per_chunk);
    println!(
        "  Sample data bytes: {} bytes/chunk",
        config.sample_data_bytes_per_chunk()
    );
    println!();

    bail!(
        "Hardware capture not yet available.\n\
         \n\
         The capture module is implemented (configure, start, stream, stop)\n\
         but hardware is currently stuck (red LED). Once the device is\n\
         recovered, this command will:\n\
         \n\
         1. Open the device and initialize the command channel\n\
         2. Write capture parameters to FPGA registers\n\
         3. Send StartSampling (0x01) to begin data flow on EP2 IN\n\
         4. Read and parse data chunks (header + samples + stats)\n\
         5. Disaggregate interleaved channels into per-channel streams\n\
         6. Send StopDataPath (0x86) + StopStream (0x02) to end capture"
    );
}

/// Start an analog capture.
pub fn capture_analog(sample_rate: u64, duration: f64, channels: &str) -> Result<()> {
    let _channel_list = parse_channel_list(channels)?;

    bail!(
        "Analog capture not yet implemented.\n\
         \n\
         This requires reverse engineering of:\n\
         - Analog data streaming format and ADC encoding\n\
         - Calibration data download and application\n\
         - FIR filter reconstruction\n\
         \n\
         Sample rate: {sample_rate} Hz\n\
         Duration: {duration}s\n\
         Channels: {channels}"
    );
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
