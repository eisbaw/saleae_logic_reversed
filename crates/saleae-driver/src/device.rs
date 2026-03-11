//! High-level device abstraction.
//!
//! The [`SaleaeDriver`] manages the full device lifecycle from discovery
//! through capture. It holds a [`UsbTransport`] and tracks device state
//! using a validated state machine matching the binary's
//! `variant<DSInitializing, DSReady, DSError>`.

use saleae_proto::device::{DeviceState, DeviceType};
use saleae_usb::transport::UsbTransport;

use crate::error::{DriverError, DriverResult};

/// Monotonic counter for device instance IDs.
///
/// Each new device connection gets a unique ID, even for the same
/// physical device. This matches the binary's `DeviceInstanceId` which
/// increments on every `AddNewDevice` call.
static NEXT_INSTANCE_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// High-level driver for a single Saleae device.
///
/// Manages the device state machine and provides methods for each
/// stage of the device lifecycle:
///
/// 1. [`upload_firmware`](SaleaeDriver::upload_firmware) -- upload FX3 firmware
/// 2. [`program_fpga`](SaleaeDriver::program_fpga) -- program FPGA bitstream (stub)
/// 3. [`start_capture`](SaleaeDriver::start_capture) -- start data capture (stub)
/// 4. [`stop_capture`](SaleaeDriver::stop_capture) -- stop data capture (stub)
///
/// State transitions are validated: invalid transitions return
/// [`DriverError::InvalidTransition`].
pub struct SaleaeDriver {
    transport: Box<dyn UsbTransport>,
    device_type: DeviceType,
    state: DeviceState,
    /// Unique identifier for this device connection. Increments on each
    /// new connection so stale references can be detected.
    device_instance_id: u64,
}

impl SaleaeDriver {
    /// Create a new driver instance for a device in bootloader mode.
    ///
    /// The device starts in `Bootloader` state (matching a freshly
    /// detected device with bcdDevice=0x0100).
    pub fn new_bootloader(transport: Box<dyn UsbTransport>, device_type: DeviceType) -> Self {
        Self {
            transport,
            device_type,
            state: DeviceState::Bootloader,
            device_instance_id: NEXT_INSTANCE_ID
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        }
    }

    /// Create a new driver instance for a device with firmware already running.
    ///
    /// The device starts in `Initializing` state (firmware running,
    /// waiting for FPGA programming).
    pub fn new_initializing(transport: Box<dyn UsbTransport>, device_type: DeviceType) -> Self {
        Self {
            transport,
            device_type,
            state: DeviceState::Initializing,
            device_instance_id: NEXT_INSTANCE_ID
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        }
    }

    /// Return the current device state.
    pub fn state(&self) -> &DeviceState {
        &self.state
    }

    /// Return the device type.
    pub fn device_type(&self) -> DeviceType {
        self.device_type
    }

    /// Return the device instance ID.
    ///
    /// This is a monotonic counter that increments on each new device
    /// connection. Stale references (from before a disconnect/reconnect)
    /// will have a lower ID than the current driver instance.
    pub fn device_instance_id(&self) -> u64 {
        self.device_instance_id
    }

    /// Return a reference to the underlying transport.
    pub fn transport(&self) -> &dyn UsbTransport {
        self.transport.as_ref()
    }

    /// Validate and perform a state transition.
    ///
    /// Returns `Err(DriverError::InvalidTransition)` if the transition
    /// is not allowed by the state machine.
    pub fn transition_to(&mut self, next: DeviceState) -> DriverResult<()> {
        if !self.state.can_transition_to(&next) {
            return Err(DriverError::InvalidTransition {
                from: self.state.to_string(),
                to: next.to_string(),
            });
        }
        self.state = next;
        Ok(())
    }

    /// Upload FX3 firmware to the device.
    ///
    /// The device must be in `Bootloader` state. After upload, the
    /// device re-enumerates -- the caller should transition to
    /// `Initializing` once the device reappears.
    pub fn upload_firmware(&mut self, image: &saleae_proto::Fx3Image) -> DriverResult<()> {
        if self.state != DeviceState::Bootloader {
            return Err(DriverError::InvalidState {
                current: self.state.to_string(),
                required: "Bootloader".into(),
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
        // Transition to Initializing since firmware is now running.
        self.transition_to(DeviceState::Initializing)?;
        Ok(())
    }

    /// Program the FPGA bitstream.
    ///
    /// Stub -- not yet implemented. Requires RE of the bitstream download
    /// protocol (see TASK-0008).
    pub fn program_fpga(&mut self, _bitstream: &[u8]) -> DriverResult<()> {
        if self.state != DeviceState::Initializing {
            return Err(DriverError::InvalidState {
                current: self.state.to_string(),
                required: "Initializing".into(),
            });
        }

        // TODO: implement FPGA bitstream download protocol
        // After successful FPGA programming + ADC init + crypto:
        self.transition_to(DeviceState::Ready)?;
        Ok(())
    }

    /// Start a digital capture.
    ///
    /// Stub -- not yet implemented. Requires RE of the capture start
    /// sequence (see backlog tasks).
    pub fn start_capture(&mut self, _config: &CaptureConfig) -> DriverResult<()> {
        if self.state != DeviceState::Ready {
            return Err(DriverError::InvalidState {
                current: self.state.to_string(),
                required: "Ready".into(),
            });
        }

        // TODO: implement capture start sequence
        self.transition_to(DeviceState::Capturing)?;
        Ok(())
    }

    /// Stop the current capture.
    ///
    /// Transitions from Capturing back to Ready.
    pub fn stop_capture(&mut self) -> DriverResult<()> {
        if self.state != DeviceState::Capturing {
            return Err(DriverError::InvalidState {
                current: self.state.to_string(),
                required: "Capturing".into(),
            });
        }

        // TODO: implement capture stop sequence
        self.transition_to(DeviceState::Ready)?;
        Ok(())
    }

    /// Transition the device to error state.
    ///
    /// Once in error state, the device cannot recover without going
    /// through Disconnected first (matching Saleae's `DSError` behavior).
    pub fn set_error(&mut self, reason: impl Into<String>) {
        // Error is always a valid transition from any state.
        self.state = DeviceState::Error(reason.into());
    }

    /// Mark the device as disconnected.
    ///
    /// This is valid from any state and resets the device for potential
    /// reconnection.
    pub fn set_disconnected(&mut self) {
        // Disconnected is always a valid transition from any state.
        self.state = DeviceState::Disconnected;
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
    fn new_bootloader_driver_starts_in_bootloader() {
        let mock = Box::new(MockTransport::new());
        let driver = SaleaeDriver::new_bootloader(mock, DeviceType::LogicPro8);
        assert_eq!(driver.state(), &DeviceState::Bootloader);
        assert_eq!(driver.device_type(), DeviceType::LogicPro8);
    }

    #[test]
    fn new_initializing_driver_starts_in_initializing() {
        let mock = Box::new(MockTransport::new());
        let driver = SaleaeDriver::new_initializing(mock, DeviceType::LogicPro8);
        assert_eq!(driver.state(), &DeviceState::Initializing);
    }

    #[test]
    fn cannot_start_capture_while_initializing() {
        let mock = Box::new(MockTransport::new());
        let mut driver = SaleaeDriver::new_initializing(mock, DeviceType::LogicPro8);
        let config = CaptureConfig::default();
        let err = driver.start_capture(&config).unwrap_err();
        assert!(matches!(err, DriverError::InvalidState { .. }));
    }

    #[test]
    fn fpga_program_transitions_to_ready() {
        let mock = Box::new(MockTransport::new());
        let mut driver = SaleaeDriver::new_initializing(mock, DeviceType::LogicPro8);
        driver.program_fpga(&[]).unwrap();
        assert_eq!(driver.state(), &DeviceState::Ready);
    }

    #[test]
    fn capture_transitions_ready_to_capturing() {
        let mock = Box::new(MockTransport::new());
        let mut driver = SaleaeDriver::new_initializing(mock, DeviceType::LogicPro8);
        driver.program_fpga(&[]).unwrap();
        let config = CaptureConfig::default();
        driver.start_capture(&config).unwrap();
        assert_eq!(driver.state(), &DeviceState::Capturing);
    }

    #[test]
    fn stop_capture_transitions_back_to_ready() {
        let mock = Box::new(MockTransport::new());
        let mut driver = SaleaeDriver::new_initializing(mock, DeviceType::LogicPro8);
        driver.program_fpga(&[]).unwrap();
        let config = CaptureConfig::default();
        driver.start_capture(&config).unwrap();
        driver.stop_capture().unwrap();
        assert_eq!(driver.state(), &DeviceState::Ready);
    }

    #[test]
    fn error_state_blocks_capture() {
        let mock = Box::new(MockTransport::new());
        let mut driver = SaleaeDriver::new_initializing(mock, DeviceType::LogicPro8);
        driver.set_error("USB disconnected");
        assert!(matches!(driver.state(), DeviceState::Error(_)));

        let config = CaptureConfig::default();
        let err = driver.start_capture(&config).unwrap_err();
        assert!(matches!(err, DriverError::InvalidState { .. }));
    }

    #[test]
    fn error_preserves_reason() {
        let mock = Box::new(MockTransport::new());
        let mut driver = SaleaeDriver::new_initializing(mock, DeviceType::LogicPro8);
        driver.set_error("bulk transfer timed out");
        match driver.state() {
            DeviceState::Error(msg) => assert_eq!(msg, "bulk transfer timed out"),
            other => panic!("expected Error, got {other}"),
        }
    }

    #[test]
    fn fx2_device_rejects_fx3_firmware() {
        let mock = Box::new(MockTransport::new());
        let mut driver = SaleaeDriver::new_bootloader(mock, DeviceType::LogicOriginal);

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

    // -----------------------------------------------------------------------
    // State transition validation tests
    // -----------------------------------------------------------------------

    #[test]
    fn valid_transitions_succeed() {
        // Disconnected -> Bootloader
        assert!(DeviceState::Disconnected.can_transition_to(&DeviceState::Bootloader));
        // Bootloader -> Initializing
        assert!(DeviceState::Bootloader.can_transition_to(&DeviceState::Initializing));
        // Initializing -> Ready
        assert!(DeviceState::Initializing.can_transition_to(&DeviceState::Ready));
        // Ready -> Capturing
        assert!(DeviceState::Ready.can_transition_to(&DeviceState::Capturing));
        // Capturing -> Ready
        assert!(DeviceState::Capturing.can_transition_to(&DeviceState::Ready));
    }

    #[test]
    fn any_state_can_transition_to_error() {
        let err = DeviceState::Error("test".into());
        assert!(DeviceState::Disconnected.can_transition_to(&err));
        assert!(DeviceState::Bootloader.can_transition_to(&err));
        assert!(DeviceState::Initializing.can_transition_to(&err));
        assert!(DeviceState::Ready.can_transition_to(&err));
        assert!(DeviceState::Capturing.can_transition_to(&err));
    }

    #[test]
    fn any_state_can_transition_to_disconnected() {
        assert!(DeviceState::Bootloader.can_transition_to(&DeviceState::Disconnected));
        assert!(DeviceState::Initializing.can_transition_to(&DeviceState::Disconnected));
        assert!(DeviceState::Ready.can_transition_to(&DeviceState::Disconnected));
        assert!(DeviceState::Capturing.can_transition_to(&DeviceState::Disconnected));
        let err = DeviceState::Error("test".into());
        assert!(err.can_transition_to(&DeviceState::Disconnected));
    }

    #[test]
    fn invalid_transitions_rejected() {
        // Bootloader -> Ready (skip Initializing)
        assert!(!DeviceState::Bootloader.can_transition_to(&DeviceState::Ready));
        // Bootloader -> Capturing
        assert!(!DeviceState::Bootloader.can_transition_to(&DeviceState::Capturing));
        // Ready -> Initializing (cannot go backwards)
        assert!(!DeviceState::Ready.can_transition_to(&DeviceState::Initializing));
        // Initializing -> Capturing (must go through Ready)
        assert!(!DeviceState::Initializing.can_transition_to(&DeviceState::Capturing));
        // Error -> Ready (must go through Disconnected)
        let err = DeviceState::Error("test".into());
        assert!(!err.can_transition_to(&DeviceState::Ready));
        // Error -> Bootloader (must go through Disconnected)
        assert!(!err.can_transition_to(&DeviceState::Bootloader));
    }

    #[test]
    fn transition_to_validates() {
        let mock = Box::new(MockTransport::new());
        let mut driver = SaleaeDriver::new_initializing(mock, DeviceType::LogicPro8);

        // Invalid: Initializing -> Capturing
        let err = driver.transition_to(DeviceState::Capturing).unwrap_err();
        assert!(matches!(err, DriverError::InvalidTransition { .. }));

        // Valid: Initializing -> Ready
        driver.transition_to(DeviceState::Ready).unwrap();
        assert_eq!(driver.state(), &DeviceState::Ready);
    }

    // -----------------------------------------------------------------------
    // DeviceInstanceId tests
    // -----------------------------------------------------------------------

    #[test]
    fn instance_id_is_unique_per_driver() {
        let mock1 = Box::new(MockTransport::new());
        let driver1 = SaleaeDriver::new_bootloader(mock1, DeviceType::LogicPro8);

        let mock2 = Box::new(MockTransport::new());
        let driver2 = SaleaeDriver::new_bootloader(mock2, DeviceType::LogicPro8);

        assert_ne!(
            driver1.device_instance_id(),
            driver2.device_instance_id(),
            "each driver should get a unique instance ID"
        );
        assert!(
            driver2.device_instance_id() > driver1.device_instance_id(),
            "instance IDs should be monotonically increasing"
        );
    }

    #[test]
    fn instance_id_increments_across_constructors() {
        let mock1 = Box::new(MockTransport::new());
        let d1 = SaleaeDriver::new_bootloader(mock1, DeviceType::LogicPro8);

        let mock2 = Box::new(MockTransport::new());
        let d2 = SaleaeDriver::new_initializing(mock2, DeviceType::LogicPro8);

        assert!(d2.device_instance_id() > d1.device_instance_id());
    }

    // -----------------------------------------------------------------------
    // Error recovery path test
    // -----------------------------------------------------------------------

    #[test]
    fn error_recovery_full_cycle() {
        let mock = Box::new(MockTransport::new());
        let mut driver = SaleaeDriver::new_initializing(mock, DeviceType::LogicPro8);

        // Go through init to Ready.
        driver.program_fpga(&[]).unwrap();
        assert_eq!(driver.state(), &DeviceState::Ready);

        // Error during operation.
        driver.set_error("USB bulk transfer failed");
        assert!(matches!(driver.state(), DeviceState::Error(_)));

        // Error -> Disconnected (reset for reconnect).
        driver.set_disconnected();
        assert_eq!(driver.state(), &DeviceState::Disconnected);

        // Disconnected -> Bootloader (device found again).
        driver.transition_to(DeviceState::Bootloader).unwrap();
        assert_eq!(driver.state(), &DeviceState::Bootloader);

        // Bootloader -> Initializing (firmware re-uploaded).
        driver.transition_to(DeviceState::Initializing).unwrap();
        assert_eq!(driver.state(), &DeviceState::Initializing);

        // Initializing -> Ready (FPGA programmed again).
        driver.transition_to(DeviceState::Ready).unwrap();
        assert_eq!(driver.state(), &DeviceState::Ready);
    }

    #[test]
    fn error_cannot_go_directly_to_ready() {
        let mock = Box::new(MockTransport::new());
        let mut driver = SaleaeDriver::new_initializing(mock, DeviceType::LogicPro8);
        driver.set_error("something broke");

        let err = driver.transition_to(DeviceState::Ready).unwrap_err();
        assert!(matches!(err, DriverError::InvalidTransition { .. }));
        // State should remain Error.
        assert!(matches!(driver.state(), DeviceState::Error(_)));
    }

    #[test]
    fn stop_capture_from_non_capturing_fails() {
        let mock = Box::new(MockTransport::new());
        let mut driver = SaleaeDriver::new_initializing(mock, DeviceType::LogicPro8);
        driver.program_fpga(&[]).unwrap();

        // Ready state, but not capturing.
        let err = driver.stop_capture().unwrap_err();
        assert!(matches!(err, DriverError::InvalidState { .. }));
    }

    // -----------------------------------------------------------------------
    // DeviceState Display tests
    // -----------------------------------------------------------------------

    #[test]
    fn device_state_display() {
        assert_eq!(DeviceState::Disconnected.to_string(), "Disconnected");
        assert_eq!(DeviceState::Bootloader.to_string(), "Bootloader");
        assert_eq!(DeviceState::Initializing.to_string(), "Initializing");
        assert_eq!(DeviceState::Ready.to_string(), "Ready");
        assert_eq!(DeviceState::Capturing.to_string(), "Capturing");
        assert_eq!(
            DeviceState::Error("test".into()).to_string(),
            "Error: test"
        );
    }

    #[test]
    fn device_state_name() {
        assert_eq!(DeviceState::Disconnected.name(), "Disconnected");
        assert_eq!(DeviceState::Error("x".into()).name(), "Error");
    }
}
