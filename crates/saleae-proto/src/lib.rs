//! Wire format types and packet parsing for Saleae Logic analyzers.
//!
//! This crate contains protocol definitions, device identification constants,
//! and parsers for the Saleae USB protocol. It has no USB dependency and is
//! fully testable with byte slices.

pub mod analog;
pub mod biquad;
pub mod calibration;
pub mod capture_data;
pub mod compare;
pub mod device;
pub mod disaggregator;
pub mod error;
pub mod export;
pub mod firmware;
pub mod fpga_regs;
pub mod gpif;
pub mod graph_time;
pub mod obfuscation;
pub mod sal;
pub mod sample_rate;
pub mod trigger;
pub mod trigger_buffer;

// Re-export key types at crate root for convenience.
pub use analog::AnalogProcessor;
pub use biquad::BiquadFilter;
pub use calibration::{
    BiquadSection, CalibrationData, CalibrationSet, VoltageRange, VoltageTransform,
};
pub use compare::{
    compare_analog, compare_digital, compare_digital_transitions, AnalogCompareResult,
    CompareReport, DigitalCompareResult,
};
pub use device::{
    BootState, DeviceProfile, DeviceType, FpgaType, SaleaeDevice, UsbId, UsbSpeed, LOGIC_4_PROFILE,
    LOGIC_8_PROFILE, LOGIC_PRO_16_PROFILE, LOGIC_PRO_8_PROFILE,
};
pub use disaggregator::{DisaggregatedData, Disaggregator, MixRateResult};
pub use error::{ProtoError, ProtoResult};
pub use export::{
    AnalogBinaryWriter, AnalogCsvWriter, AnalogExporter, DigitalBinaryWriter, DigitalCsvWriter,
    DigitalExporter, ExportConfig, ExportFormat,
};
pub use firmware::fx3::{Fx3Image, Fx3Section};
pub use gpif::{GpifBusWidth, GpifConfig};
pub use graph_time::GraphTime;
pub use obfuscation::{EncryptionMode, SimpleEncryption};
pub use sal::{SalAnalogChannel, SalCaptureInfo, SalDigitalChannel, SalMeta, SalReader, SalWriter};
pub use sample_rate::{PerformanceTier, SampleRateOption};
pub use trigger::{
    EdgeDetector, EdgeType, PulseDetector, PulsePolarity, TriggerCondition, TriggerConfig,
    TriggerEngine,
};
pub use trigger_buffer::{RingBuffer, TriggerCapture, TriggerCaptureResult, TriggerCaptureState};
