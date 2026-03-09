//! USB device discovery for Saleae devices.
//!
//! This module is only compiled when the `nusb-backend` feature is enabled.

use saleae_proto::device::{DeviceType, SaleaeDevice, UsbId};

use crate::error::{UsbError, UsbResult};

/// List all Saleae devices currently connected to the USB bus.
///
/// Scans all USB devices and filters for known Saleae VID:PID pairs.
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
            devices.push(SaleaeDevice {
                usb_id,
                device_type,
                bus: info.bus_number(),
                address: info.device_address(),
                serial: info.serial_number().map(|s| s.to_string()),
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
