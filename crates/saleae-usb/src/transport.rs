//! The [`UsbTransport`] trait -- the core abstraction for USB operations.
//!
//! All higher-level code (saleae-driver) depends on this trait, never on
//! concrete USB implementations. This enables testing protocol logic against
//! recorded USB captures without physical hardware.

use crate::error::UsbResult;

/// Direction of a USB control transfer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Host to device.
    Out,
    /// Device to host.
    In,
}

/// Parameters for a USB control transfer.
///
/// Maps directly to the USB spec fields. See USB 2.0 spec section 9.3.
#[derive(Debug, Clone)]
pub struct ControlTransferParams {
    /// bmRequestType direction, type, and recipient packed into one byte.
    pub bm_request_type: u8,
    /// bRequest field.
    pub b_request: u8,
    /// wValue field.
    pub w_value: u16,
    /// wIndex field.
    pub w_index: u16,
}

/// Abstraction over USB device operations.
///
/// Implementations must handle:
/// - Control transfers (for firmware upload, register I/O)
/// - Bulk OUT transfers (for commands, FPGA bitstream)
/// - Bulk IN transfers (for responses, capture data)
///
/// All methods are synchronous. The nusb backend uses `futures_lite::block_on`
/// internally to bridge from the async nusb API.
pub trait UsbTransport: Send {
    /// Perform a USB control transfer with data sent to the device.
    ///
    /// Returns the number of bytes actually transferred.
    fn control_transfer_out(&self, params: &ControlTransferParams, data: &[u8])
        -> UsbResult<usize>;

    /// Perform a USB control transfer with data received from the device.
    ///
    /// `buf` is the receive buffer. Returns the number of bytes read.
    fn control_transfer_in(
        &self,
        params: &ControlTransferParams,
        buf: &mut [u8],
    ) -> UsbResult<usize>;

    /// Write data to a bulk OUT endpoint.
    ///
    /// Returns the number of bytes actually transferred.
    fn bulk_transfer_out(&self, endpoint: u8, data: &[u8]) -> UsbResult<usize>;

    /// Read data from a bulk IN endpoint.
    ///
    /// `buf` is the receive buffer. Returns the number of bytes read.
    fn bulk_transfer_in(&self, endpoint: u8, buf: &mut [u8]) -> UsbResult<usize>;

    /// Clear a halt / stall condition on a bulk or interrupt endpoint.
    ///
    /// Sends CLEAR_FEATURE(ENDPOINT_HALT) to the device. Required before
    /// EP1 communication on some firmware configurations.
    ///
    /// Default implementation is a no-op (returns Ok). Override in backends
    /// that support real USB devices.
    fn clear_halt(&self, _endpoint: u8) -> UsbResult<()> {
        Ok(())
    }

    /// Reset the USB device.
    fn reset(&self) -> UsbResult<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    // Verify that UsbTransport is object-safe (can be used as dyn trait).
    fn _assert_object_safe(_: &dyn UsbTransport) {}

    #[test]
    fn control_params_debug() {
        let params = ControlTransferParams {
            bm_request_type: 0x40,
            b_request: 0xA0,
            w_value: 0x0000,
            w_index: 0x0000,
        };
        let dbg = format!("{params:?}");
        assert!(dbg.contains("0x40") || dbg.contains("64"));
    }
}
