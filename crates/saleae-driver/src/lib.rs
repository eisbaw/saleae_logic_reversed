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

pub mod autoscale;
pub mod calibration;
pub mod capture;
pub mod device;
pub mod eeprom;
pub mod ep1;
pub mod error;
pub mod firmware;
pub mod fpga;
pub mod init;
pub mod transfer_queue;

pub use error::{DriverError, DriverResult};
