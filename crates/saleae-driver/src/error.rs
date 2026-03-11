//! Error types for the driver layer.

/// Result type alias for driver operations.
pub type DriverResult<T> = Result<T, DriverError>;

/// Errors that can occur in the driver layer.
#[derive(Debug, thiserror::Error)]
pub enum DriverError {
    /// Protocol-level error.
    #[error("protocol error: {0}")]
    Proto(#[from] saleae_proto::ProtoError),

    /// USB transport error.
    #[error("USB error: {0}")]
    Usb(#[from] saleae_usb::UsbError),

    /// Device is in the wrong state for the requested operation.
    #[error("invalid state: device is {current}, but operation requires {required}")]
    InvalidState { current: String, required: String },

    /// A state transition was rejected by the state machine.
    #[error("invalid state transition: {from} -> {to}")]
    InvalidTransition { from: String, to: String },

    /// Firmware image not found or not available for this device type.
    #[error("no firmware available for {device_type}")]
    NoFirmware {
        device_type: saleae_proto::DeviceType,
    },

    /// The device did not re-enumerate after firmware upload within the timeout.
    #[error("device did not re-enumerate after firmware upload")]
    ReEnumerationTimeout,

    /// Capture configuration is invalid.
    #[error("invalid capture config: {reason}")]
    InvalidCaptureConfig { reason: String },

    /// An EP1 command failed.
    #[error("{command} failed: {detail}")]
    CommandFailed {
        command: &'static str,
        detail: String,
    },

    /// FPGA programming or post-init failed.
    #[error("FPGA programming failed: {context}")]
    FpgaProgramming { context: String },

    /// HTTP error during calibration download.
    #[error("calibration download failed: {0}")]
    CalibrationDownload(String),

    /// I/O error (file read/write during calibration caching).
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
