//! Wire format types and packet parsing for Saleae Logic analyzers.
//!
//! This crate contains protocol definitions, device identification constants,
//! and parsers for the Saleae USB protocol. It has no USB dependency and is
//! fully testable with byte slices.

pub mod analog;
pub mod biquad;
pub mod calibration;
pub mod capture_data;
pub mod device;
pub mod disaggregator;
pub mod error;
pub mod firmware;
pub mod fpga_regs;
pub mod graph_time;
pub mod obfuscation;

// Re-export key types at crate root for convenience.
pub use analog::AnalogProcessor;
pub use biquad::BiquadFilter;
pub use calibration::{
    BiquadSection, CalibrationData, CalibrationSet, VoltageRange, VoltageTransform,
};
pub use device::{
    BootState, DeviceProfile, DeviceType, FpgaType, SaleaeDevice, UsbId, UsbSpeed, LOGIC_4_PROFILE,
    LOGIC_8_PROFILE, LOGIC_PRO_16_PROFILE, LOGIC_PRO_8_PROFILE,
};
pub use disaggregator::{DisaggregatedData, Disaggregator, MixRateResult};
pub use error::{ProtoError, ProtoResult};
pub use firmware::fx3::{Fx3Image, Fx3Section};
pub use graph_time::GraphTime;
pub use obfuscation::{EncryptionMode, SimpleEncryption};
