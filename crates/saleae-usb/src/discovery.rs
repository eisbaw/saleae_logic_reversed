//! USB device discovery for Saleae devices.
//!
//! This module is only compiled when the `nusb-backend` feature is enabled.

use std::time::{Duration, Instant};

use saleae_proto::device::{BootState, DeviceType, SaleaeDevice, UsbId, UsbSpeed};

use crate::error::{UsbError, UsbResult};

/// Convert nusb's `Speed` enum to our crate-independent `UsbSpeed`.
fn convert_speed(speed: nusb::Speed) -> UsbSpeed {
    match speed {
        nusb::Speed::Low => UsbSpeed::Low,
        nusb::Speed::Full => UsbSpeed::Full,
        nusb::Speed::High => UsbSpeed::High,
        nusb::Speed::Super => UsbSpeed::Super,
        nusb::Speed::SuperPlus => UsbSpeed::SuperPlus,
        // nusb::Speed is #[non_exhaustive], so handle future variants
        _ => UsbSpeed::Super,
    }
}

/// List all Saleae devices currently connected to the USB bus.
///
/// Scans all USB devices and filters for known Saleae VID:PID pairs.
/// Populates boot state (from bcdDevice) and USB speed for each device.
pub fn list_devices() -> UsbResult<Vec<SaleaeDevice>> {
    let device_list = nusb::list_devices().map_err(|e| UsbError::OpenFailed {
        context: format!("failed to enumerate USB devices: {e}"),
    })?;

    let mut devices = Vec::new();

    for info in device_list {
        let vid = info.vendor_id();
        let pid = info.product_id();
        let usb_id = UsbId::new(vid, pid);

        if let Ok(device_type) = DeviceType::from_usb_id(usb_id) {
            let boot_state = BootState::from_bcd_device(info.device_version());
            let usb_speed = info.speed().map(convert_speed);

            devices.push(SaleaeDevice {
                usb_id,
                device_type,
                bus: info.bus_number(),
                address: info.device_address(),
                serial: info.serial_number().map(|s| s.to_string()),
                boot_state,
                usb_speed,
            });
        }
    }

    Ok(devices)
}

/// Find and open the first Saleae device on the USB bus.
///
/// Returns the nusb `Device` handle. Use [`NusbTransport::open`] to wrap it
/// in a transport.
pub fn open_first_device() -> UsbResult<nusb::Device> {
    let device_list = nusb::list_devices().map_err(|e| UsbError::OpenFailed {
        context: format!("failed to enumerate USB devices: {e}"),
    })?;

    for info in device_list {
        let vid = info.vendor_id();
        let pid = info.product_id();
        let usb_id = UsbId::new(vid, pid);

        if DeviceType::from_usb_id(usb_id).is_ok() {
            return info.open().map_err(|e| UsbError::OpenFailed {
                context: format!("failed to open device {usb_id}: {e}"),
            });
        }
    }

    Err(UsbError::NoDeviceFound)
}

/// Find and open a specific Saleae device by VID:PID.
pub fn open_device_by_type(device_type: DeviceType) -> UsbResult<nusb::Device> {
    let target_id = device_type.usb_id();
    let device_list = nusb::list_devices().map_err(|e| UsbError::OpenFailed {
        context: format!("failed to enumerate USB devices: {e}"),
    })?;

    for info in device_list {
        let vid = info.vendor_id();
        let pid = info.product_id();
        if vid == target_id.vid && pid == target_id.pid {
            return info.open().map_err(|e| UsbError::OpenFailed {
                context: format!("failed to open device {target_id}: {e}"),
            });
        }
    }

    Err(UsbError::NoDeviceFound)
}

/// Find the first Saleae device and return its info without opening it.
///
/// This is useful for checking boot state before deciding whether to
/// open the device for firmware upload.
pub fn find_first_saleae() -> UsbResult<SaleaeDevice> {
    let devices = list_devices()?;
    devices.into_iter().next().ok_or(UsbError::NoDeviceFound)
}

/// Wait for a Saleae device to appear on the USB bus with firmware running.
///
/// Polls `list_devices()` every `poll_interval` until a device with
/// `BootState::FirmwareRunning` is found or `timeout` expires.
///
/// This is used after firmware upload to wait for the device to
/// re-enumerate with its new firmware.
///
/// Returns the discovered device info, or `UsbError::Timeout` if the
/// device does not appear within the timeout.
pub fn wait_for_device(timeout: Duration, poll_interval: Duration) -> UsbResult<SaleaeDevice> {
    let start = Instant::now();

    loop {
        if let Ok(devices) = list_devices() {
            for device in devices {
                if device.boot_state == BootState::FirmwareRunning {
                    return Ok(device);
                }
            }
        }

        if start.elapsed() >= timeout {
            return Err(UsbError::Timeout {
                timeout_ms: timeout.as_millis() as u64,
            });
        }

        std::thread::sleep(poll_interval);
    }
}

/// Wait for a Saleae device to appear on the USB bus in bootloader mode.
///
/// Polls `list_devices()` every `poll_interval` until a device with
/// `BootState::Bootloader` is found or `timeout` expires.
///
/// This is used after sending ResetAndReEnumerate to wait for the device
/// to reboot into the B2 bootloader.
pub fn wait_for_bootloader(timeout: Duration, poll_interval: Duration) -> UsbResult<SaleaeDevice> {
    let start = Instant::now();

    loop {
        if let Ok(devices) = list_devices() {
            for device in devices {
                if device.boot_state == BootState::Bootloader {
                    return Ok(device);
                }
            }
        }

        if start.elapsed() >= timeout {
            return Err(UsbError::Timeout {
                timeout_ms: timeout.as_millis() as u64,
            });
        }

        std::thread::sleep(poll_interval);
    }
}
