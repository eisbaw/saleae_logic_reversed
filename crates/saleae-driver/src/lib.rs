//! High-level driver for Saleae Logic analyzers.
//!
//! This crate provides a facade over the lower-level `saleae-proto` and
//! `saleae-usb` crates. It implements the device lifecycle:
//!
//! 1. Device discovery
//! 2. Firmware upload (FX3 boot image -> device RAM)
//! 3. FPGA bitstream programming
//! 4. Capture configuration and execution
//!
//! All USB operations go through the [`UsbTransport`] trait, making the
//! driver testable with mock transports.
//!
//! # Public API
//!
//! The [`facade`] module is the primary entry point. Key types and functions
//! are re-exported at the crate root for convenience:
//!
//! - [`ConnectedDevice`] -- a device with an initialized EP1 channel
//! - [`CaptureConfig`], [`CaptureResult`] -- capture parameters and results
//! - [`DriverError`], [`DriverResult`] -- error types

// -- Public modules (used by CLI and/or integration tests) --------------------
// TODO: CLI still reaches into these directly; ideally all access would go
// through `facade` re-exports, but that requires further refactoring.
pub mod adc;
pub mod calibrate;
pub mod calibration;
pub mod capture;
pub mod device_manager;
pub mod ep1;
pub mod error;
pub mod facade;
pub mod firmware;
pub mod fpga;

// -- Internal modules (not used outside this crate) ---------------------------
pub(crate) mod autoscale;
pub(crate) mod eeprom;
pub(crate) mod framed;
pub(crate) mod hmcad;
pub(crate) mod init;
pub(crate) mod jesd;
pub(crate) mod transfer_queue;

// -- Re-exports ---------------------------------------------------------------
pub use capture::{CaptureConfig, CaptureResult};
pub use error::{DriverError, DriverResult};
pub use facade::ConnectedDevice;
pub use transfer_queue::TransferConfig;
