//! Wire format types and packet parsing for Saleae Logic analyzers.
//!
//! This crate contains protocol definitions, device identification constants,
//! and parsers for the Saleae USB protocol. It has no USB dependency and is
//! fully testable with byte slices.

pub mod device;
pub mod error;
pub mod firmware;
pub mod obfuscation;

// Re-export key types at crate root for convenience.
pub use device::{BootState, DeviceType, SaleaeDevice, UsbId, UsbSpeed};
pub use error::{ProtoError, ProtoResult};
pub use firmware::fx3::{Fx3Image, Fx3Section};
pub use obfuscation::{EncryptionMode, SimpleEncryption};
