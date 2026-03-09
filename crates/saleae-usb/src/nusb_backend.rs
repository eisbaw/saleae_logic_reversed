//! Real USB transport implementation backed by `nusb`.
//!
//! This module is only compiled when the `nusb-backend` feature is enabled.

use std::time::Duration;

use crate::error::{UsbError, UsbResult};
use crate::transport::{ControlTransferParams, UsbTransport};

/// Default timeout for USB control transfers.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

/// Real USB transport using the `nusb` crate.
pub struct NusbTransport {
    /// The open USB interface handle.
    interface: nusb::Interface,
}

impl NusbTransport {
    /// Open a USB device and claim interface 0.
    ///
    /// The `device` should already be identified as a Saleae device.
    pub fn open(device: nusb::Device) -> UsbResult<Self> {
        let interface = device
            .claim_interface(0)
            .map_err(|e| UsbError::ClaimFailed {
                interface: 0,
                context: e.to_string(),
            })?;

        Ok(Self { interface })
    }
}

impl UsbTransport for NusbTransport {
    fn control_transfer_out(
        &self,
        params: &ControlTransferParams,
        data: &[u8],
    ) -> UsbResult<usize> {
        let control = nusb::transfer::Control {
            control_type: control_type_from_bm(params.bm_request_type),
            recipient: recipient_from_bm(params.bm_request_type),
            request: params.b_request,
            value: params.w_value,
            index: params.w_index,
        };

        self.interface
            .control_out_blocking(control, data, DEFAULT_TIMEOUT)
            .map_err(|e| UsbError::ControlTransfer {
                context: format!("{e}"),
            })
    }

    fn control_transfer_in(
        &self,
        params: &ControlTransferParams,
        buf: &mut [u8],
    ) -> UsbResult<usize> {
        let control = nusb::transfer::Control {
            control_type: control_type_from_bm(params.bm_request_type),
            recipient: recipient_from_bm(params.bm_request_type),
            request: params.b_request,
            value: params.w_value,
            index: params.w_index,
        };

        self.interface
            .control_in_blocking(control, buf, DEFAULT_TIMEOUT)
            .map_err(|e| UsbError::ControlTransfer {
                context: format!("{e}"),
            })
    }

    fn bulk_transfer_out(&self, endpoint: u8, data: &[u8]) -> UsbResult<usize> {
        let completion =
            futures_lite::future::block_on(self.interface.bulk_out(endpoint, data.to_vec()));

        completion.status.map_err(|e| UsbError::BulkTransfer {
            endpoint,
            context: format!("{e}"),
        })?;

        Ok(completion.data.actual_length())
    }

    fn bulk_transfer_in(&self, endpoint: u8, buf: &mut [u8]) -> UsbResult<usize> {
        let completion = futures_lite::future::block_on(
            self.interface
                .bulk_in(endpoint, nusb::transfer::RequestBuffer::new(buf.len())),
        );

        completion.status.map_err(|e| UsbError::BulkTransfer {
            endpoint,
            context: format!("{e}"),
        })?;

        let len = completion.data.len().min(buf.len());
        buf[..len].copy_from_slice(&completion.data[..len]);
        Ok(len)
    }

    fn reset(&self) -> UsbResult<()> {
        // nusb does not expose a direct device reset on Interface.
        // This is a known limitation -- device reset requires dropping
        // and re-opening the device.
        Err(UsbError::ControlTransfer {
            context: "device reset not supported through nusb Interface handle; \
                      drop and re-open the device instead"
                .into(),
        })
    }
}

/// Extract the control type from bmRequestType bits [6:5].
fn control_type_from_bm(bm: u8) -> nusb::transfer::ControlType {
    match (bm >> 5) & 0x03 {
        0 => nusb::transfer::ControlType::Standard,
        1 => nusb::transfer::ControlType::Class,
        2 => nusb::transfer::ControlType::Vendor,
        _ => nusb::transfer::ControlType::Vendor, // reserved, treat as vendor
    }
}

/// Extract the recipient from bmRequestType bits [4:0].
fn recipient_from_bm(bm: u8) -> nusb::transfer::Recipient {
    match bm & 0x1F {
        0 => nusb::transfer::Recipient::Device,
        1 => nusb::transfer::Recipient::Interface,
        2 => nusb::transfer::Recipient::Endpoint,
        3 => nusb::transfer::Recipient::Other,
        _ => nusb::transfer::Recipient::Other,
    }
}
