//! Error types for protocol parsing.

use std::fmt;

/// Result type alias for protocol operations.
pub type ProtoResult<T> = Result<T, ProtoError>;

/// Errors that can occur during protocol parsing.
#[derive(Debug, thiserror::Error)]
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

    /// Generic parse error with context.
    #[error("parse error: {context}")]
    Parse { context: String },
}

impl ProtoError {
    /// Create a parse error with the given context message.
    pub fn parse(context: impl fmt::Display) -> Self {
        Self::Parse {
            context: context.to_string(),
        }
    }
}
