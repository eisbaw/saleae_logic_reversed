//! USB transport layer for Saleae Logic analyzers.
//!
//! This crate defines a [`UsbTransport`] trait that abstracts USB operations
//! (control transfers, bulk transfers), plus a real implementation backed by
//! the `nusb` crate and a mock implementation for testing without hardware.
//!
//! The `nusb` backend is behind the `nusb-backend` feature flag (enabled by
//! default). Disable it to build and test without USB system dependencies.

pub mod error;
pub mod mock;
pub mod mock_stream;
pub mod stream;
pub mod transport;

#[cfg(feature = "nusb-backend")]
pub mod nusb_backend;

#[cfg(feature = "nusb-backend")]
pub mod nusb_stream;

#[cfg(feature = "nusb-backend")]
pub mod discovery;

pub use error::{UsbError, UsbResult};
pub use stream::BulkStreamReader;
pub use transport::{ControlTransferParams, UsbTransport};
