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

/// Start a digital capture.
pub fn capture_digital(sample_rate: u64, duration: f64, channels: &str) -> Result<()> {
    let _channel_list = parse_channel_list(channels)?;

    bail!(
        "Digital capture not yet implemented.\n\
         \n\
         This requires reverse engineering of:\n\
         - Capture start/stop command sequence\n\
         - Digital data streaming format\n\
         - Host-side trigger detection\n\
         \n\
         Sample rate: {sample_rate} Hz\n\
         Duration: {duration}s\n\
         Channels: {channels}"
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
