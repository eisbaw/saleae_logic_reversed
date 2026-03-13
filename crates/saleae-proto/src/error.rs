//! Error types for protocol parsing.

use std::fmt;
use std::sync::Arc;

/// Result type alias for protocol operations.
pub type ProtoResult<T> = Result<T, ProtoError>;

/// Errors that can occur during protocol parsing.
#[derive(Debug, Clone, thiserror::Error)]
pub enum ProtoError {
    /// Input buffer is too short to contain the expected data.
    #[error("buffer too short: need {need} bytes, have {have}")]
    BufferTooShort { need: usize, have: usize },

    /// FX3 firmware image has invalid magic bytes (expected b"CY").
    #[error("invalid FX3 magic: expected 0x4359 (\"CY\"), got {got:#06x}")]
    InvalidFx3Magic { got: u16 },

    /// FX3 firmware image has unexpected image_type field.
    #[error("unexpected FX3 image_type: expected 0xB0, got {got:#04x}")]
    UnexpectedFx3ImageType { got: u8 },

    /// A firmware section address or length is out of the expected range.
    #[error("firmware section at offset {offset} has invalid {field}: {value:#010x}")]
    InvalidFirmwareSection {
        offset: usize,
        field: &'static str,
        value: u32,
    },

    /// Firmware image checksum mismatch.
    #[error("firmware checksum mismatch: computed {computed:#010x}, stored {stored:#010x}")]
    ChecksumMismatch { computed: u32, stored: u32 },

    /// Unknown or unsupported VID:PID combination.
    #[error("unknown device: VID={vid:#06x} PID={pid:#06x}")]
    UnknownDevice { vid: u16, pid: u16 },

    /// Invalid sample rate combination for a device.
    #[error("invalid sample rate: {reason}")]
    InvalidSampleRate { reason: String },

    /// A selection was attempted on an empty list of available rates.
    #[error("empty selection list: no available rates to choose from")]
    EmptySelection,

    /// Data content is invalid (e.g. bad length, unexpected values).
    #[error("invalid data: {reason}")]
    InvalidData { reason: String },

    /// Generic parse error with context.
    #[error("parse error: {context}")]
    Parse { context: String },

    /// I/O error (e.g. reading a `.cal` or `.sal` file).
    ///
    /// Wrapped in `Arc` so that `ProtoError` remains `Clone`.
    #[error("I/O error: {0}")]
    Io(Arc<std::io::Error>),
}

impl ProtoError {
    /// Create a parse error with the given context message.
    pub fn parse(context: impl fmt::Display) -> Self {
        Self::Parse {
            context: context.to_string(),
        }
    }
}

impl From<std::io::Error> for ProtoError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(Arc::new(err))
    }
}
