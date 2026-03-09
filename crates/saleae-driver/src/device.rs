//! High-level device abstraction.
//!
//! The [`SaleaeDriver`] manages the full device lifecycle from discovery
//! through capture. It holds a [`UsbTransport`] and tracks device state.

use saleae_proto::device::{DeviceState, DeviceType};
use saleae_usb::transport::UsbTransport;

use crate::error::{DriverError, DriverResult};

/// High-level driver for a single Saleae device.
///
/// Manages the device state machine and provides methods for each
/// stage of the device lifecycle:
///
/// 1. [`upload_firmware`](SaleaeDriver::upload_firmware) -- upload FX3 firmware
/// 2. [`program_fpga`](SaleaeDriver::program_fpga) -- program FPGA bitstream (stub)
/// 3. [`start_capture`](SaleaeDriver::start_capture) -- start data capture (stub)
/// 4. [`stop_capture`](SaleaeDriver::stop_capture) -- stop data capture (stub)
pub struct SaleaeDriver {
    transport: Box<dyn UsbTransport>,
    device_type: DeviceType,
    state: DeviceState,
}

impl SaleaeDriver {
    /// Create a new driver instance for a device.
    ///
    /// The device starts in `Initializing` state.
    pub fn new(transport: Box<dyn UsbTransport>, device_type: DeviceType) -> Self {
        Self {
            transport,
            device_type,
            state: DeviceState::Initializing,
        }
    }

    /// Return the current device state.
    pub fn state(&self) -> DeviceState {
        self.state
    }

    /// Return the device type.
    pub fn device_type(&self) -> DeviceType {
        self.device_type
    }

    /// Return a reference to the underlying transport.
    pub fn transport(&self) -> &dyn UsbTransport {
        self.transport.as_ref()
    }

    /// Upload FX3 firmware to the device.
    ///
    /// The device must be in `Initializing` state (fresh bootloader).
    /// After upload, the device re-enumerates and this driver instance
    /// becomes invalid -- a new transport must be opened after
    /// re-enumeration.
    pub fn upload_firmware(&mut self, image: &saleae_proto::Fx3Image) -> DriverResult<()> {
        if self.state != DeviceState::Initializing {
            return Err(DriverError::InvalidState {
                current: self.state,
                required: DeviceState::Initializing,
            });
        }

        if !self.device_type.is_fx3() {
            return Err(DriverError::NoFirmware {
                device_type: self.device_type,
            });
        }

        crate::firmware::upload_fx3_firmware(self.transport.as_ref(), image)?;

        // After firmware upload, the device re-enumerates. This driver
        // instance is now stale. The caller must open a new transport.
        // We do NOT transition to Ready here because the device is gone
        // from the USB bus momentarily.
        Ok(())
    }

    /// Program the FPGA bitstream.
    ///
    /// Stub -- not yet implemented. Requires RE of the bitstream download
    /// protocol (see TASK-0008).
    pub fn program_fpga(&mut self, _bitstream: &[u8]) -> DriverResult<()> {
        if self.state != DeviceState::Initializing {
            return Err(DriverError::InvalidState {
                current: self.state,
                required: DeviceState::Initializing,
            });
        }

        // TODO: implement FPGA bitstream download protocol
        // After successful FPGA programming + ADC init + crypto:
        self.state = DeviceState::Ready;
        Ok(())
    }

    /// Start a digital capture.
    ///
    /// Stub -- not yet implemented. Requires RE of the capture start
    /// sequence (see backlog tasks).
    pub fn start_capture(&mut self, _config: &CaptureConfig) -> DriverResult<()> {
        if self.state != DeviceState::Ready {
            return Err(DriverError::InvalidState {
                current: self.state,
                required: DeviceState::Ready,
            });
        }

        // TODO: implement capture start sequence
        Ok(())
    }

    /// Stop the current capture.
    ///
    /// Stub -- not yet implemented.
    pub fn stop_capture(&mut self) -> DriverResult<()> {
        // TODO: implement capture stop sequence
        Ok(())
    }

    /// Transition the device to error state.
    ///
    /// Once in error state, the device cannot recover without a physical
    /// reconnect (matching Saleae's `DSError` behavior).
    pub fn set_error(&mut self) {
        self.state = DeviceState::Error;
    }
}

/// Capture configuration parameters.
///
/// Stub -- fields will be populated as the capture protocol is
/// reverse engineered.
#[derive(Debug, Clone)]
pub struct CaptureConfig {
    /// Sample rate in Hz.
    pub sample_rate_hz: u64,
    /// Which digital channels to enable (bitmask).
    pub digital_channels: u16,
    /// Which analog channels to enable (bitmask). Pro devices only.
    pub analog_channels: u16,
    /// Capture duration in seconds (0 = continuous until stop).
    pub duration_secs: f64,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            sample_rate_hz: 1_000_000, // 1 MHz
            digital_channels: 0xFF,    // all 8 channels
            analog_channels: 0,
            duration_secs: 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use saleae_usb::mock::MockTransport;

    #[test]
    fn new_driver_starts_in_initializing() {
        let mock = Box::new(MockTransport::new());
        let driver = SaleaeDriver::new(mock, DeviceType::LogicPro8);
        assert_eq!(driver.state(), DeviceState::Initializing);
        assert_eq!(driver.device_type(), DeviceType::LogicPro8);
    }

    #[test]
    fn cannot_start_capture_while_initializing() {
        let mock = Box::new(MockTransport::new());
        let mut driver = SaleaeDriver::new(mock, DeviceType::LogicPro8);
        let config = CaptureConfig::default();
        let err = driver.start_capture(&config).unwrap_err();
        assert!(matches!(err, DriverError::InvalidState { .. }));
    }

    #[test]
    fn fpga_program_transitions_to_ready() {
        let mock = Box::new(MockTransport::new());
        let mut driver = SaleaeDriver::new(mock, DeviceType::LogicPro8);
        driver.program_fpga(&[]).unwrap();
        assert_eq!(driver.state(), DeviceState::Ready);
    }

    #[test]
    fn capture_allowed_after_ready() {
        let mock = Box::new(MockTransport::new());
        let mut driver = SaleaeDriver::new(mock, DeviceType::LogicPro8);
        driver.program_fpga(&[]).unwrap();
        let config = CaptureConfig::default();
        driver.start_capture(&config).unwrap();
    }

    #[test]
    fn error_state_is_terminal() {
        let mock = Box::new(MockTransport::new());
        let mut driver = SaleaeDriver::new(mock, DeviceType::LogicPro8);
        driver.set_error();
        assert_eq!(driver.state(), DeviceState::Error);

        let config = CaptureConfig::default();
        let err = driver.start_capture(&config).unwrap_err();
        assert!(matches!(err, DriverError::InvalidState { .. }));
    }

    #[test]
    fn fx2_device_rejects_fx3_firmware() {
        let mock = Box::new(MockTransport::new());
        let mut driver = SaleaeDriver::new(mock, DeviceType::LogicOriginal);

        let image = saleae_proto::Fx3Image {
            header: saleae_proto::firmware::fx3::Fx3Header {
                image_ctl: 0,
                image_type: saleae_proto::firmware::fx3::IMAGE_TYPE_NORMAL,
            },
            sections: vec![],
            entry_address: 0,
            checksum: 0,
        };

        let err = driver.upload_firmware(&image).unwrap_err();
        assert!(matches!(err, DriverError::NoFirmware { .. }));
    }
}
