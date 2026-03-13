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

    /// An EP1 command failed (catch-all for miscellaneous failures).
    #[error("{command} failed: {detail}")]
    CommandFailed {
        command: &'static str,
        detail: String,
    },

    /// Device returned a NACK (non-zero acknowledgment byte).
    #[error("{command}: NACK response {actual:#04x} (expected 0x00)")]
    NackResponse { command: &'static str, actual: u8 },

    /// Response had the wrong number of bytes.
    #[error("{command}: wrong response length: expected {expected}, got {actual}")]
    WrongResponseLength {
        command: &'static str,
        expected: usize,
        actual: usize,
    },

    /// Response content did not match the expected value.
    #[error("{command}: unexpected response: expected {expected}, got {actual:02x?}")]
    UnexpectedResponse {
        command: &'static str,
        expected: String,
        actual: Vec<u8>,
    },

    /// FPGA register readback did not match the written value.
    #[error("register r{register:#04x} readback mismatch: wrote {expected:#04x}, read {actual:#04x}")]
    ReadbackMismatch {
        register: u8,
        expected: u8,
        actual: u8,
    },

    /// A polled operation did not complete within the allowed attempts.
    #[error("{operation}: poll timeout after {attempts} attempts")]
    PollTimeout {
        operation: &'static str,
        attempts: usize,
    },

    /// FPGA programming or post-init failed.
    #[error("FPGA programming failed: {context}")]
    FpgaProgramming { context: String },

    /// Calibration data could not be loaded or parsed.
    #[error("calibration error: {0}")]
    CalibrationError(String),

    /// HTTP error during calibration download.
    #[error("calibration download: HTTP error: {0}")]
    CalibrationDownload(String),

    /// I/O error (file read/write during calibration caching).
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// An argument to a driver function was out of range or otherwise invalid.
    #[error("invalid argument: {message}")]
    InvalidArgument { message: String },
}
