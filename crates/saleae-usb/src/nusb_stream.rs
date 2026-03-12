//! Real [`BulkStreamReader`] implementation backed by `nusb::transfer::Queue`.
//!
//! This module is only compiled when the `nusb-backend` feature is enabled.
//!
//! The `nusb::transfer::Queue<RequestBuffer>` manages the kernel URB
//! submit/complete cycle. We pre-submit `queue_depth` transfers at `start()`,
//! then cycle between `next_complete()` and `submit()` for sustained throughput.
//!
//! # Stall recovery
//!
//! On USB stall (EPIPE), we call `clear_halt()` on the endpoint and resubmit
//! the transfer. The original Saleae code does the same in its event loop.

use std::time::Duration;

use nusb::transfer::RequestBuffer;

use crate::error::{UsbError, UsbResult};
use crate::stream::BulkStreamReader;

/// Real bulk stream reader using `nusb::transfer::Queue`.
pub struct NusbStreamReader {
    /// The USB interface handle (needed for `clear_halt`).
    interface: nusb::Interface,
    /// The transfer queue. `None` before `start()` or after `stop()`.
    queue: Option<nusb::transfer::Queue<RequestBuffer>>,
    /// Number of transfers currently submitted to the kernel.
    in_flight: usize,
    /// The endpoint address (e.g., 0x82 for EP2 IN).
    endpoint: u8,
    /// The transfer buffer size in bytes.
    transfer_size: usize,
}

impl NusbStreamReader {
    /// Create a new stream reader from an already-claimed USB interface.
    ///
    /// The interface must have been claimed via `device.claim_interface(0)`.
    /// The stream is not started until [`start()`](BulkStreamReader::start) is called.
    pub fn new(interface: nusb::Interface) -> Self {
        Self {
            interface,
            queue: None,
            in_flight: 0,
            endpoint: 0,
            transfer_size: 0,
        }
    }
}

impl BulkStreamReader for NusbStreamReader {
    fn start(&mut self, endpoint: u8, transfer_size: usize, queue_depth: usize) -> UsbResult<()> {
        if self.queue.is_some() {
            return Err(UsbError::BulkTransfer {
                endpoint,
                context: "stream already started".into(),
            });
        }

        self.endpoint = endpoint;
        self.transfer_size = transfer_size;

        let mut queue = self.interface.bulk_in_queue(endpoint);

        // Pre-submit initial transfers.
        for _ in 0..queue_depth {
            queue.submit(RequestBuffer::new(transfer_size));
        }
        self.in_flight = queue_depth;
        self.queue = Some(queue);

        tracing::debug!(
            endpoint = endpoint,
            transfer_size = transfer_size,
            queue_depth = queue_depth,
            "Bulk stream started"
        );

        Ok(())
    }

    fn next_completion(&mut self, timeout: Duration) -> UsbResult<Vec<u8>> {
        let queue = self.queue.as_mut().ok_or(UsbError::BulkTransfer {
            endpoint: self.endpoint,
            context: "stream not active".into(),
        })?;

        // Use futures_lite::future::block_on with a timeout race.
        let completion = futures_lite::future::block_on(async {
            futures_lite::future::or(
                async {
                    let c = queue.next_complete().await;
                    Ok(c)
                },
                async {
                    async_io::Timer::after(timeout).await;
                    Err(UsbError::Timeout {
                        timeout_ms: timeout.as_millis() as u64,
                    })
                },
            )
            .await
        })?;

        self.in_flight -= 1;

        // Check transfer status.
        if let Err(e) = completion.status {
            let err_str = e.to_string();
            // Check for stall (EPIPE). nusb reports this as a TransferError.
            if err_str.contains("stall") || err_str.contains("PIPE") {
                tracing::warn!(
                    endpoint = self.endpoint,
                    "USB stall detected, clearing halt"
                );
                // Best-effort clear_halt.
                let _ = self.interface.clear_halt(self.endpoint);
            }
            return Err(UsbError::BulkTransfer {
                endpoint: self.endpoint,
                context: format!("transfer failed: {e}"),
            });
        }

        // For Queue<RequestBuffer>, completion.data is Vec<u8>.
        Ok(completion.data)
    }

    fn recycle(&mut self, buf: Vec<u8>) -> UsbResult<()> {
        let queue = self.queue.as_mut().ok_or(UsbError::BulkTransfer {
            endpoint: self.endpoint,
            context: "stream not active".into(),
        })?;

        queue.submit(RequestBuffer::reuse(buf, self.transfer_size));
        self.in_flight += 1;
        Ok(())
    }

    fn stop(&mut self) -> UsbResult<()> {
        // Dropping the Queue cancels all pending transfers.
        if let Some(queue) = self.queue.take() {
            tracing::debug!(
                endpoint = self.endpoint,
                in_flight = self.in_flight,
                "Stopping bulk stream"
            );
            drop(queue);
        }
        self.in_flight = 0;
        Ok(())
    }

    fn pending(&self) -> usize {
        self.in_flight
    }
}
