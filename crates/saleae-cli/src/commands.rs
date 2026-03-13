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
// Auto-recovery helper (thin wrapper around driver facade)
// -----------------------------------------------------------------------

/// Ensure a Saleae device is ready for EP1 communication.
///
/// Delegates to [`saleae_driver::facade::connect_or_recover`] which handles
/// all recovery scenarios: bootloader mode, dead EP1 thread, and firmware
/// upload. Status messages are printed to stdout for the user.
///
/// Returns a [`saleae_driver::facade::ConnectedDevice`] with an initialized
/// EP1 channel (reseed + ping verified).
fn ensure_device_ready(
    firmware_path: &str,
    serial_filter: Option<&str>,
) -> Result<saleae_driver::facade::ConnectedDevice> {
    // Parse firmware upfront so we fail fast on bad path.
    let firmware_data = std::fs::read(firmware_path)
        .with_context(|| format!("failed to read firmware file: {firmware_path}"))?;
    let (image, _) = saleae_proto::Fx3Image::parse(&firmware_data)
        .context("failed to parse FX3 firmware image")?;

    let dev = saleae_driver::facade::connect_or_recover(
        &image,
        serial_filter,
        Some(&|msg| println!("{msg}")),
    )
    .context("failed to connect/recover device")?;

    Ok(dev)
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
pub fn upload_firmware(firmware_path: &str, serial_filter: Option<&str>) -> Result<()> {
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
    let device = match serial_filter {
        Some(serial) => saleae_usb::discovery::find_saleae_by_serial(serial)
            .context("failed to find Saleae device matching serial filter")?,
        None => saleae_usb::discovery::find_first_saleae()
            .context("failed to find Saleae device")?,
    };

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

    // Upload firmware and wait for re-enumeration via driver facade.
    let timeout = Duration::from_secs(10);
    let poll_interval = Duration::from_millis(500);

    match saleae_driver::facade::upload_firmware_and_wait(
        &image,
        timeout,
        poll_interval,
        serial_filter,
        &|msg| println!("{msg}"),
    ) {
        Ok(()) => {
            println!();
            println!("Firmware upload complete. Device is ready for FPGA programming.");
        }
        Err(e) => {
            println!();
            println!("WARNING: Firmware upload issue: {e}");
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
pub fn reset_device(serial_filter: Option<&str>) -> Result<()> {
    let device = match serial_filter {
        Some(serial) => saleae_usb::discovery::find_saleae_by_serial(serial)
            .context("failed to find Saleae device matching serial filter")?,
        None => saleae_usb::discovery::find_first_saleae()
            .context("failed to find Saleae device")?,
    };

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

    let mut dev = saleae_driver::facade::open_and_connect_with_serial(serial_filter)
        .context("failed to connect to device")?;

    println!("Sending ResetAndReEnumerate (0x82)...");
    // Send the command. The device will reset, so we don't expect a response.
    let _ = dev.channel.send_command(0x82, &[]);
    println!("Reset command sent. Device should re-enumerate in bootloader mode.");
    println!("Wait a few seconds, then check with: lsusb -d 21a9:");

    Ok(())
}

/// Ping the device: ReseedDevice + Ping handshake.
///
/// This tests firmware responsiveness and verifies the XOR encryption
/// is working correctly. It is the first real hardware validation of
/// the LFSR implementation.
pub fn ping(serial_filter: Option<&str>) -> Result<()> {
    println!("Running ReseedDevice + Ping...");
    let dev = saleae_driver::facade::open_and_connect_with_serial(serial_filter)
        .context("failed to connect to device")?;

    let encryption_mode = saleae_driver::facade::encryption_mode_for_device(&dev.device);
    println!("Found device: {}", dev.device);
    println!("  Boot state: {}", dev.device.boot_state);
    if let Some(speed) = dev.device.usb_speed {
        println!("  USB speed:  {speed}");
    }
    println!("  Encryption mode: {encryption_mode:?}");
    println!();
    println!("SUCCESS: Ping returned correct complement.");
    println!("XOR encryption is in sync with firmware.");

    Ok(())
}

/// Program the FPGA with a bitstream file.
///
/// Reads the bitstream from disk, opens the device, performs the
/// ReseedDevice + Ping handshake, then programs the FPGA and runs
/// the post-init sequence.
pub fn program_fpga(bitstream_path: &str, skip_post_init: bool, serial_filter: Option<&str>) -> Result<()> {
    // Read bitstream file.
    let bitstream = std::fs::read(bitstream_path)
        .with_context(|| format!("failed to read bitstream file: {bitstream_path}"))?;

    println!(
        "Bitstream loaded: {} bytes from {bitstream_path}",
        bitstream.len()
    );
    println!();

    // Open device and create initialized EP1 channel.
    let mut dev = saleae_driver::facade::open_and_connect_with_serial(serial_filter)
        .context("failed to connect to device")?;

    println!("Found device: {}", dev.device);
    println!("  Boot state: {}", dev.device.boot_state);
    if let Some(speed) = dev.device.usb_speed {
        println!("  USB speed:  {speed}");
    }
    println!();

    // Program FPGA.
    println!("Programming FPGA ({} bytes)...", bitstream.len());
    saleae_driver::fpga::program_fpga(&mut dev.channel, &bitstream, dev.profile, dev.usb_speed)
        .context("FPGA programming failed")?;
    println!("FPGA programming verified (readback = 0xAA).");
    println!();

    // Post-init sequence.
    if skip_post_init {
        println!("Skipping post-init sequence (--skip-post-init).");
    } else {
        // Determine FPGA type from EEPROM hardware revision.
        let hw_rev = saleae_driver::fpga::read_hw_revision(&mut dev.channel)
            .context("failed to read hardware revision from EEPROM")?;
        let is_lattice = hw_rev.is_lattice();

        println!("Hardware revision: {hw_rev}");
        println!("Running post-init sequence (Lattice: {is_lattice})...");
        saleae_driver::fpga::fpga_post_init(&mut dev.channel, is_lattice)
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
pub fn device_init(bitstream_path: Option<&str>, skip_fpga: bool, serial_filter: Option<&str>) -> Result<()> {
    // Open device and create initialized EP1 channel.
    let mut dev = saleae_driver::facade::open_and_connect_with_serial(serial_filter)
        .context("failed to connect to device")?;

    println!("Found device: {}", dev.device);
    println!("  Boot state: {}", dev.device.boot_state);
    if let Some(speed) = dev.device.usb_speed {
        println!("  USB speed:  {speed}");
    }
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

        println!(
            "Programming FPGA ({} bytes from {bs_path})...",
            bitstream.len()
        );
        saleae_driver::fpga::program_fpga(&mut dev.channel, &bitstream, dev.profile, dev.usb_speed)
            .context("FPGA programming failed")?;
        println!("FPGA programming verified.");
        println!();

        // Post-init: determine FPGA type from EEPROM hardware revision.
        let hw_rev = saleae_driver::fpga::read_hw_revision(&mut dev.channel)
            .context("failed to read hardware revision from EEPROM")?;
        let is_lattice = hw_rev.is_lattice();
        println!("Hardware revision: {hw_rev}");
        println!("Running FPGA post-init (Lattice: {is_lattice})...");
        saleae_driver::fpga::fpga_post_init(&mut dev.channel, is_lattice)
            .context("FPGA post-init failed")?;
        println!("Post-init complete.");
        println!();
    } else {
        println!("Skipping FPGA programming (--skip-fpga).");
        println!();
    }

    // Determine FPGA type from EEPROM hardware revision.
    let hw_rev = saleae_driver::fpga::read_hw_revision(&mut dev.channel)
        .context("failed to read hardware revision from EEPROM")?;
    let is_lattice = hw_rev.is_lattice();
    println!("Hardware revision: {hw_rev}");
    println!("Running CommonDeviceInit (Lattice: {is_lattice})...");
    let result = saleae_driver::fpga::common_device_init(&mut dev.channel, is_lattice)
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
    serial_filter: Option<&str>,
) -> Result<()> {
    // Use ensure_device_ready for firmware upload + EP1 recovery.
    let mut dev = ensure_device_ready(firmware_path, serial_filter)?;

    // Read firmware version string for diagnostics.
    match dev.channel.get_device_info() {
        Ok(info) => println!("Firmware: {info}"),
        Err(e) => println!("GetDeviceInfo failed: {e}"),
    }
    println!();

    // Step 4: FPGA program + verify
    if auto_bitstream {
        println!("Auto-detecting bitstream...");
        let (_bitstream_data, found_path) =
            try_bitstreams(&mut dev.channel, SLX9_BITSTREAMS, dev.profile, dev.usb_speed)?;
        println!("Using bitstream: {found_path}");
    } else {
        let bitstream = std::fs::read(bitstream_path)
            .with_context(|| format!("failed to read bitstream: {bitstream_path}"))?;
        println!("Programming FPGA ({} bytes)...", bitstream.len());
        saleae_driver::fpga::program_fpga(&mut dev.channel, &bitstream, dev.profile, dev.usb_speed)?;
        println!("FPGA programmed and verified.");
    }
    println!();

    // Read EEPROM device ID (offset 0x08, 8 bytes LE) for calibration file lookup.
    let device_id = dev.channel
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
    let cal_data = dev.channel.read_version(&[0x33, 0x81, 0x10, 0x00, 0x10, 0x00])?;
    println!(
        "  Calibration: {}",
        cal_data
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<Vec<_>>()
            .join(" ")
    );

    let dcm_result =
        saleae_driver::adc::init_ad9637(&mut dev.channel, &cal_data).context("AD9637 init failed")?;
    println!(
        "  DCM locked: {} (r0x40={:#04x})",
        dcm_result.locked, dcm_result.status_40
    );
    println!();

    // Re-init EP1 (reseed + ping for next phase)
    dev.channel.initialize().context("EP1 re-init failed")?;
    dev.channel.write_registers(&[(0x0f, 0x00), (0x10, 0x0f), (0x11, 0x00)])?;
    println!("Channel enables set.");

    // Step 6: Pre-capture ADC power on
    println!("Pre-capture ADC power on...");
    let capture_dcm = saleae_driver::adc::power_on_for_capture(&mut dev.channel)
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
        &mut dev.channel,
        &capture_config,
        Duration::from_secs(2),
        None,
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

    // Compute matching disaggregator from the same capture config.
    println!();
    println!("Decoding captured data...");

    let disagg = capture_config.disaggregator()?;
    let cycle_size = disagg.cycle_bytes();

    println!(
        "  Cycle size: {} bytes ({} digital ch, {} analog ch)",
        cycle_size,
        disagg.num_digital(),
        disagg.num_analog(),
    );

    // Truncate to a whole number of cycles.
    let usable_bytes = (total_bytes / cycle_size) * cycle_size;
    let num_cycles = usable_bytes / cycle_size;
    if usable_bytes < total_bytes {
        println!(
            "  Truncated {} trailing bytes ({} complete cycles)",
            total_bytes - usable_bytes,
            num_cycles,
        );
    }

    let output = disagg.process_chunk(&raw_data[..usable_bytes])?;

    // Print digital channel summary.
    println!();
    println!("Digital channels ({} channels):", output.digital_samples.len());
    for (ch, data) in output.digital_samples.iter().enumerate() {
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
        output.analog_samples.len()
    );
    for (ch, data) in output.analog_samples.iter().enumerate() {
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
    if !output.analog_samples.is_empty() {
        match resolve_cal_file(cal_file, Some(device_id)) {
            Ok(cal_path) => {
                println!("Loading calibration from {}", cal_path.display());
                match std::fs::read(&cal_path) {
                    Ok(cal_bytes) => {
                        match saleae_proto::calibration::parse_calibration(&cal_bytes) {
                            Ok(cal) => {
                                let analog_indices: Vec<u8> =
                                    capture_config.analog_channels.clone();
                                match saleae_driver::cal_pipeline::apply_calibration(
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
    saleae_driver::adc::power_down(&mut dev.channel).context("ADC power down failed")?;
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
#[allow(clippy::too_many_arguments)]
pub fn capture_digital(
    sample_rate: u64,
    duration: f64,
    channels: &str,
    output: Option<&str>,
    format: &str,
    firmware_path: &str,
    bitstream_path: &str,
    serial_filter: Option<&str>,
    stop_flag: &std::sync::Arc<std::sync::atomic::AtomicBool>,
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

    let duration_desc = if duration <= 0.0 {
        "until Ctrl-C".to_string()
    } else {
        format!("{duration:.1}s")
    };

    println!("Digital capture configuration:");
    println!("  Sample rate:       {} Hz", config.sample_rate);
    println!("  Channels:          {:?}", config.digital_channels);
    println!("  Duration samples:  {}", config.duration_samples);
    println!("  Duration:          {duration_desc}");
    println!("  Voltage threshold: {} V", config.voltage_threshold);
    println!("  Firmware:          {firmware_path}");
    println!("  Bitstream:         {bitstream_path}");
    println!();

    // Parse export format early so we fail fast on bad --format.
    let export_format = parse_export_format(format)?;

    if duration < 0.0 {
        bail!("Duration must be >= 0 (0 = until Ctrl-C).");
    }

    // Parse bitstream upfront so we fail fast on bad path.
    let bitstream = std::fs::read(bitstream_path)
        .with_context(|| format!("failed to read bitstream: {bitstream_path}"))?;

    // --- Step 1: Firmware + EP1 init (auto-recovery) ---
    println!("Ensuring device is ready (firmware + EP1)...");
    let mut dev = ensure_device_ready(firmware_path, serial_filter)?;
    println!("Device ready: {}", dev.device);
    println!();

    // --- Steps 2-5: FPGA + ADC init + channel config + ADC power-on ---
    println!("Initializing device for capture...");
    saleae_driver::facade::init_for_capture(&mut dev, &bitstream)
        .context("device initialization for capture failed")?;
    println!("Device initialized for capture.");
    println!();

    // --- Step 6-8: Capture pipeline (configure + start + stream + stop) ---
    println!("Running capture ({duration_desc})...");
    let capture_duration = if duration <= 0.0 {
        Duration::ZERO
    } else {
        Duration::from_secs_f64(duration)
    };
    let result = saleae_driver::capture::run_capture(
        &mut dev.channel,
        &config,
        capture_duration,
        Some(stop_flag),
    )
    .context("capture failed")?;

    // --- Step 9: Teardown ---
    // Power down ADC
    match saleae_driver::adc::power_down(&mut dev.channel) {
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
            dev.profile,
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
#[allow(clippy::too_many_arguments)]
pub fn capture_quick(
    duration: f64,
    digital_mask_str: &str,
    analog_mask_str: &str,
    bitstream_path: Option<&str>,
    output_path: Option<&str>,
    firmware_path: Option<&str>,
    serial_filter: Option<&str>,
    stop_flag: &std::sync::Arc<std::sync::atomic::AtomicBool>,
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

    if let Some(p) = output_path {
        println!("  Output file:   {p}");
    }
    if let Some(fw) = firmware_path {
        println!("  Firmware:      {fw} (auto-recovery enabled)");
    }
    println!();

    if duration < 0.0 {
        bail!("Duration must be >= 0 (0 = until Ctrl-C).");
    }

    // Step 1: Get device ready.
    let mut dev = if let Some(fw_path) = firmware_path {
        // Auto-recovery: handles bootloader, dead EP1, etc.
        println!("Ensuring device is ready (firmware + EP1)...");
        ensure_device_ready(fw_path, serial_filter)?
    } else {
        // Manual mode: device must already have firmware running.
        saleae_driver::facade::open_and_connect_with_serial(serial_filter)
            .context("failed to connect to device")?
    };

    println!("Found device: {}", dev.device);
    if let Some(speed) = dev.device.usb_speed {
        println!("  USB speed: {speed}");
    }
    println!();

    // Step 3: Optionally program FPGA.
    if let Some(bs_path) = bitstream_path {
        let bitstream = std::fs::read(bs_path)
            .with_context(|| format!("failed to read bitstream: {bs_path}"))?;

        // program_fpga includes reset + bitstream upload + r0x7F verify.
        println!("Programming FPGA ({} bytes)...", bitstream.len());
        saleae_driver::fpga::program_fpga(&mut dev.channel, &bitstream, dev.profile, dev.usb_speed)
            .context("FPGA programming failed")?;
        println!("FPGA programmed and verified.");
        println!();
    } else {
        // Verify FPGA is already programmed by reading r0x7F.
        println!("Checking if FPGA is programmed...");
        dev.channel.write_registers(&[(0x7F, 0xAA)])?;
        let verify = dev.channel.read_registers(&[0x7F])?;
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
        let cal_data = dev.channel
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

        match saleae_driver::adc::init_ad9637(&mut dev.channel, &cal_data) {
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
        dev.channel
            .initialize()
            .context("EP1 re-init after ADC failed")?;

        // Channel enables.
        dev.channel.write_registers(&[(0x0f, 0x00), (0x10, 0x0f), (0x11, 0x00)])?;
        println!("  Channel enables set.");

        // Pre-capture ADC power on.
        match saleae_driver::adc::power_on_for_capture(&mut dev.channel) {
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
    let capture_duration = if duration <= 0.0 {
        Duration::ZERO
    } else {
        Duration::from_secs_f64(duration)
    };
    let duration_desc = if duration <= 0.0 {
        "until Ctrl-C".to_string()
    } else {
        format!("{duration:.1}s")
    };
    println!("Running capture ({duration_desc})...");
    let result = saleae_driver::capture::run_capture(
        &mut dev.channel,
        &quick_config,
        capture_duration,
        Some(stop_flag),
    )
    .context("capture failed")?;

    // Power down ADC if we powered it on.
    if !is_digital_only {
        match saleae_driver::adc::power_down(&mut dev.channel) {
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

/// Print a warning if the capture ended early or teardown had errors.
fn warn_if_partial(result: &saleae_driver::capture::CaptureResult) {
    if !result.completed_normally {
        println!("WARNING: capture ended early due to EP2 read error (data may be partial)");
    }
    for w in &result.teardown_warnings {
        println!("WARNING: teardown: {w}");
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
/// Performs the full sequence: firmware upload (auto-recovery), FPGA
/// programming, ADC init, EP2 capture, disaggregation, calibration
/// (VoltageTransform + IIR biquad filter), and export.
///
/// The capture uses `CaptureConfig` with analog channels enabled and no
/// digital channels. The raw EP2 stream is disaggregated to extract
/// per-channel ADC codes, which are then converted to calibrated voltages.
#[allow(clippy::too_many_arguments)]
pub fn capture_analog(
    sample_rate: u64,
    duration: f64,
    channels: &str,
    output: Option<&str>,
    format: &str,
    cal_file: Option<&str>,
    firmware_path: &str,
    bitstream_path: &str,
    serial_filter: Option<&str>,
    stop_flag: &std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> Result<()> {
    let channel_list = parse_channel_list(channels)?;

    // Build capture configuration (analog-only: no digital channels).
    let duration_samples = if duration > 0.0 {
        (sample_rate as f64 * duration) as u64
    } else {
        0
    };

    let config = saleae_driver::capture::CaptureConfig {
        duration_samples,
        ..saleae_driver::capture::CaptureConfig::pro8(sample_rate, vec![], channel_list.clone())
    };

    config.validate().context("invalid capture config")?;

    let duration_desc = if duration <= 0.0 {
        "until Ctrl-C".to_string()
    } else {
        format!("{duration:.1}s")
    };

    println!("Analog capture configuration:");
    println!("  Sample rate:       {} Hz", config.sample_rate);
    println!("  Analog channels:   {:?}", config.analog_channels);
    println!("  Duration samples:  {}", config.duration_samples);
    println!("  Duration:          {duration_desc}");
    println!("  Firmware:          {firmware_path}");
    println!("  Bitstream:         {bitstream_path}");
    println!();

    // Parse export format early so we fail fast on bad --format.
    let export_format = parse_export_format(format)?;

    if duration < 0.0 {
        bail!("Duration must be >= 0 (0 = until Ctrl-C).");
    }

    // Parse bitstream upfront so we fail fast on bad path.
    let bitstream = std::fs::read(bitstream_path)
        .with_context(|| format!("failed to read bitstream: {bitstream_path}"))?;

    // --- Load calibration data ---
    // Try explicit --cal-file first, then search common directories.
    let cal_result = match resolve_cal_file(cal_file, None) {
        Ok(path) => {
            println!("Loading calibration from {}", path.display());
            saleae_driver::cal_download::load_calibration_file(&path)
                .with_context(|| format!("failed to load {}", path.display()))
        }
        Err(_) => {
            let cal_dir = std::path::PathBuf::from("re");
            find_and_load_calibration(&cal_dir)
        }
    };

    let cal_data_opt = match cal_result {
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

            // Create per-channel AnalogProcessors.
            println!();
            println!("Per-channel processor setup:");
            for &ch in &channel_list {
                match create_channel_processor(&cal, ch, sample_rate) {
                    Ok(proc) => {
                        let Some(vr) = cal.find_voltage_range(ch) else {
                            println!("  Channel {ch}: no voltage range in calibration data");
                            continue;
                        };
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
                            "    IIR filter:    {} biquad sections, {} settling samples",
                            proc.filter().num_sections(),
                            proc.settling_samples()
                        );
                    }
                    Err(e) => {
                        println!("  Channel {ch}: FAILED - {e}");
                    }
                }
            }
            println!();
            Some(cal)
        }
        Err(e) => {
            println!("WARNING: No calibration data available: {e}");
            println!("  Analog output will be uncalibrated (raw ADC codes -> naive voltage).");
            println!("  Place a .cal file in re/ or use --cal-file.");
            println!();
            None
        }
    };

    // --- Step 1: Firmware + EP1 init (auto-recovery) ---
    println!("Ensuring device is ready (firmware + EP1)...");
    let mut dev = ensure_device_ready(firmware_path, serial_filter)?;
    println!("Device ready: {}", dev.device);
    println!();

    // --- Steps 2-5: FPGA + ADC init + channel config + ADC power-on ---
    println!("Initializing device for capture...");
    saleae_driver::facade::init_for_capture(&mut dev, &bitstream)
        .context("device initialization for capture failed")?;
    println!("Device initialized for capture.");
    println!();

    // --- Step 6-8: Capture pipeline (configure + start + stream + stop) ---
    let capture_duration = if duration <= 0.0 {
        Duration::ZERO
    } else {
        Duration::from_secs_f64(duration)
    };
    println!("Running analog capture ({duration_desc})...");
    let result = saleae_driver::capture::run_capture(
        &mut dev.channel,
        &config,
        capture_duration,
        Some(stop_flag),
    )
    .context("capture failed")?;

    // --- Step 9: Teardown ---
    match saleae_driver::adc::power_down(&mut dev.channel) {
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

    // --- Disaggregate + calibrate + export ---
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

        // Disaggregate raw EP2 data into per-channel analog samples.
        let disaggregator = saleae_proto::Disaggregator::new(
            dev.profile,
            &[],              // no digital channels
            &channel_list,
            0,                // no digital rate
            sample_rate,
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

            // Apply calibration: convert raw ADC codes to calibrated voltages.
            let calibrated: Vec<Vec<f64>> = if let Some(ref cal) = cal_data_opt {
                // Build per-channel processors and apply calibration pipeline.
                channel_list
                    .iter()
                    .enumerate()
                    .map(|(i, &ch)| {
                        match create_channel_processor(cal, ch, sample_rate) {
                            Ok(mut proc) => proc.process(&disagg.analog_samples[i]),
                            Err(e) => {
                                println!(
                                    "  WARNING: no calibration for channel {ch}: {e}. Using naive transform."
                                );
                                // Fall back to uncalibrated linear transform.
                                let vr = cal.find_voltage_range(ch);
                                let (min_v, max_v) = match vr {
                                    Some(vr) => (vr.min_voltage, vr.max_voltage),
                                    None => (-10.0, 10.0),
                                };
                                let transform = saleae_proto::VoltageTransform::from_range(
                                    min_v, max_v, -2048, 2047,
                                );
                                disagg.analog_samples[i]
                                    .iter()
                                    .map(|&code| transform.apply(code))
                                    .collect()
                            }
                        }
                    })
                    .collect()
            } else {
                // No calibration at all: use default +/-10V range.
                let transform =
                    saleae_proto::VoltageTransform::from_range(-10.0, 10.0, -2048, 2047);
                disagg
                    .analog_samples
                    .iter()
                    .map(|ch_samples| ch_samples.iter().map(|&code| transform.apply(code)).collect())
                    .collect()
            };

            // Write calibrated voltages to export files.
            match saleae_proto::AnalogExporter::new(&export_config)
                .context("failed to create analog exporter")?
            {
                saleae_proto::AnalogExporter::Csv(mut w) => {
                    w.write_samples(&calibrated)
                        .context("failed to write analog CSV samples")?;
                    w.finish()
                        .context("failed to finalize analog CSV exporter")?;
                }
                saleae_proto::AnalogExporter::Binary(mut w) => {
                    w.write_samples(&calibrated)
                        .context("failed to write analog binary samples")?;
                    w.finish()
                        .context("failed to finalize analog binary exporter")?;
                }
            }

            let num_samples = calibrated.first().map(|c| c.len()).unwrap_or(0);
            let trimmed = result.raw_data.len() - usable_len;
            println!(
                "  Exported {} analog samples per channel ({} channels, {} bytes trimmed for alignment)",
                num_samples,
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
    } else if !result.raw_data.is_empty() {
        // No --output specified, but we have data. Print a summary of what
        // we could do with it.
        println!();
        println!("No --output specified. Use --output <dir> to export analog data.");
    }

    Ok(())
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
            return saleae_driver::cal_download::load_calibration_file(&path)
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
    let settling = set.settling_samples();

    Ok(saleae_proto::AnalogProcessor::with_settling(
        transform, filter, settling,
    ))
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
