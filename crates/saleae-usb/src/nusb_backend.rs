//! Real USB transport implementation backed by `nusb`.
//!
//! This module is only compiled when the `nusb-backend` feature is enabled.

use std::time::Duration;

use crate::error::{UsbError, UsbResult};
use crate::transport::{ControlTransferParams, UsbTransport};

/// Default timeout for USB control transfers.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

/// Timeout for bulk transfers (EP1 commands/responses).
/// Set long enough for FPGA bitstream chunks but short enough to detect hangs.
const BULK_TIMEOUT: Duration = Duration::from_secs(10);

/// Real USB transport using the `nusb` crate.
pub struct NusbTransport {
    /// The open USB interface handle.
    interface: nusb::Interface,
}

impl NusbTransport {
    /// Open a USB device, set configuration 1, and claim interface 0.
    ///
    /// The explicit SET_CONFIGURATION is required because the FX3 firmware's
    /// SETCONF event handler creates the EP1 DMA channels. Without it, the
    /// command processing thread blocks in `GetCommandRxBuffer()` and EP1
    /// bulk transfers hang.
    ///
    /// The `device` should already be identified as a Saleae device.
    pub fn open(device: nusb::Device) -> UsbResult<Self> {
        // SET_CONFIGURATION(1) triggers the FX3 firmware's CY_U3P_USB_EVENT_SETCONF
        // handler, which calls CyU3PSetEpConfig() and CyU3PDmaChannelCreate() for
        // the EP1 OUT/IN endpoints. Without this, EP1 DMA is never started.
        device
            .set_configuration(1)
            .map_err(|e| UsbError::ClaimFailed {
                interface: 0,
                context: format!("set_configuration(1) failed: {e}"),
            })?;

        let interface = device
            .claim_interface(0)
            .map_err(|e| UsbError::ClaimFailed {
                interface: 0,
                context: e.to_string(),
            })?;

        // Clear any stall condition on EP1 IN only.
        // The pcap shows CLEAR_FEATURE(ENDPOINT_HALT) on EP1 IN (0x81) only,
        // NOT on EP1 OUT. Clearing halt on EP1 OUT confuses the FX3 firmware's
        // DMA state and causes EP1 IN reads to hang.
        if let Err(e) = interface.clear_halt(0x81) {
            tracing::debug!("clear_halt(EP1 IN) during open: {e} (non-fatal)");
        }

        Ok(Self { interface })
    }
}

/// Run an async future with a timeout, returning a timeout error if the
/// deadline expires first.
///
/// When the timeout fires, the `TransferFuture` is dropped. nusb cancels
/// the underlying kernel URB on drop (via `usbdevfs_discardurb`), so the
/// transfer does not remain in-flight after a timeout.
fn bulk_with_timeout<T, F>(
    transfer: F,
    endpoint: u8,
    direction: &str,
) -> UsbResult<nusb::transfer::Completion<T>>
where
    F: std::future::Future<Output = nusb::transfer::Completion<T>>,
{
    futures_lite::future::block_on(async {
        futures_lite::future::or(
            async {
                let c = transfer.await;
                Ok(c)
            },
            async {
                async_io::Timer::after(BULK_TIMEOUT).await;
                tracing::warn!(
                    endpoint,
                    direction,
                    timeout_secs = BULK_TIMEOUT.as_secs(),
                    "bulk transfer timed out"
                );
                Err(UsbError::BulkTransfer {
                    endpoint,
                    context: format!(
                        "bulk {direction} timed out after {:?}",
                        BULK_TIMEOUT
                    ),
                })
            },
        )
        .await
    })
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
        let transfer = self.interface.bulk_out(endpoint, data.to_vec());
        let completion = bulk_with_timeout(transfer, endpoint, "OUT")?;

        completion.status.map_err(|e| UsbError::BulkTransfer {
            endpoint,
            context: format!("{e}"),
        })?;

        Ok(completion.data.actual_length())
    }

    fn bulk_transfer_in(&self, endpoint: u8, buf: &mut [u8]) -> UsbResult<usize> {
        let transfer = self
            .interface
            .bulk_in(endpoint, nusb::transfer::RequestBuffer::new(buf.len()));
        let completion = bulk_with_timeout(transfer, endpoint, "IN")?;

        completion.status.map_err(|e| UsbError::BulkTransfer {
            endpoint,
            context: format!("{e}"),
        })?;

        let len = completion.data.len().min(buf.len());
        buf[..len].copy_from_slice(&completion.data[..len]);
        Ok(len)
    }

    fn clear_halt(&self, endpoint: u8) -> UsbResult<()> {
        self.interface
            .clear_halt(endpoint)
            .map_err(|e| UsbError::ControlTransfer {
                context: format!("clear_halt(ep={endpoint:#04x}) failed: {e}"),
            })
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
