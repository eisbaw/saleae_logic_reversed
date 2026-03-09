//! CLI command implementations.

use anyhow::{bail, Context, Result};

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
    }

    Ok(())
}

/// Upload firmware to a Saleae device.
pub fn upload_firmware(firmware_path: &str) -> Result<()> {
    // Read firmware file.
    let firmware_data = std::fs::read(firmware_path)
        .with_context(|| format!("failed to read firmware file: {firmware_path}"))?;

    // Parse FX3 image.
    let (image, _consumed) = saleae_proto::Fx3Image::parse(&firmware_data)
        .context("failed to parse FX3 firmware image")?;

    println!("Firmware image parsed:");
    println!("  Sections: {}", image.sections.len());
    println!("  Total data: {} bytes", image.total_data_bytes());
    println!("  Entry point: {:#010x}", image.entry_address);
    println!();

    // Open device.
    let usb_device =
        saleae_usb::discovery::open_first_device().context("failed to open Saleae device")?;

    let transport = saleae_usb::nusb_backend::NusbTransport::open(usb_device)
        .context("failed to claim USB interface")?;

    // Upload.
    saleae_driver::firmware::upload_fx3_firmware(&transport, &image)
        .context("firmware upload failed")?;

    println!("Firmware uploaded successfully.");
    println!("Device should re-enumerate momentarily.");

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
