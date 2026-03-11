//! Error types for USB transport operations.

/// Result type alias for USB operations.
pub type UsbResult<T> = Result<T, UsbError>;

/// Errors that can occur during USB transport operations.
#[derive(Debug, Clone, thiserror::Error)]
pub enum UsbError {
    /// No Saleae device found on the USB bus.
    #[error("no Saleae device found")]
    NoDeviceFound,

    /// Multiple Saleae devices found when exactly one was expected.
    #[error("multiple Saleae devices found ({count}); specify which one")]
    MultipleDevicesFound { count: usize },

    /// USB control transfer failed.
    #[error("control transfer failed: {context}")]
    ControlTransfer { context: String },

    /// USB bulk transfer failed.
    #[error("bulk transfer failed on endpoint {endpoint:#04x}: {context}")]
    BulkTransfer { endpoint: u8, context: String },

    /// USB device could not be opened.
    #[error("failed to open device: {context}")]
    OpenFailed { context: String },

    /// USB interface could not be claimed.
    #[error("failed to claim interface {interface}: {context}")]
    ClaimFailed { interface: u8, context: String },

    /// Operation timed out.
    #[error("USB operation timed out after {timeout_ms}ms")]
    Timeout { timeout_ms: u64 },

    /// Protocol-level error from saleae-proto.
    #[error("protocol error: {0}")]
    Proto(#[from] saleae_proto::ProtoError),

    /// Unexpected response from device.
    #[error("unexpected response: {context}")]
    UnexpectedResponse { context: String },

    /// The nusb backend is not available (compiled without `nusb-backend` feature).
    #[error("nusb backend not available (compile with feature `nusb-backend`)")]
    BackendNotAvailable,
}
