//! The [`BulkStreamReader`] trait -- continuous bulk-in streaming with queued transfers.
//!
//! This is a separate trait from [`UsbTransport`](crate::transport::UsbTransport)
//! because streaming is fundamentally different from request/response. A stream
//! reader maintains a queue of outstanding USB bulk IN transfers and returns
//! completed buffers to the caller for processing. The caller recycles buffers
//! back into the queue to maintain throughput.
//!
//! The design mirrors the original Saleae two-thread model
//! (`BulkReadStream_Read` + `ReadStream_QueueBuffersThread`) but uses
//! `nusb::transfer::Queue` which combines both roles.

use std::time::Duration;

use crate::error::UsbResult;

/// A continuous bulk-in stream reader using pre-queued transfers.
///
/// The implementation maintains a queue of outstanding USB bulk IN transfers.
/// As each transfer completes, the caller processes the data and returns the
/// buffer for reuse via [`recycle()`](BulkStreamReader::recycle).
///
/// # Lifecycle
///
/// 1. [`start()`](BulkStreamReader::start) -- pre-submit `queue_depth` transfers.
/// 2. Loop: [`next_completion()`](BulkStreamReader::next_completion) -> process -> [`recycle()`](BulkStreamReader::recycle).
/// 3. [`stop()`](BulkStreamReader::stop) -- cancel pending transfers.
///
/// # Buffer ownership
///
/// Buffers are owned by the caller between `next_completion()` and `recycle()`.
/// The implementation tracks how many are in-flight via [`pending()`](BulkStreamReader::pending).
pub trait BulkStreamReader: Send {
    /// Start the stream: pre-submit `queue_depth` transfers of `transfer_size` bytes each.
    ///
    /// After this call, `pending()` should equal `queue_depth`.
    fn start(&mut self, endpoint: u8, transfer_size: usize, queue_depth: usize) -> UsbResult<()>;

    /// Wait for the next completed transfer. Returns the data buffer.
    ///
    /// The returned `Vec<u8>` contains the actual data received (may be shorter
    /// than `transfer_size` for short transfers). The caller owns this buffer
    /// until it calls [`recycle()`](BulkStreamReader::recycle).
    ///
    /// Returns `Err(UsbError::Timeout)` if no transfer completes within `timeout`.
    /// Returns `Err(UsbError::BulkTransfer)` on stall or other USB error.
    fn next_completion(&mut self, timeout: Duration) -> UsbResult<Vec<u8>>;

    /// Return a processed buffer for reuse (resubmits as a new transfer).
    ///
    /// The buffer's contents are discarded; only the allocation is reused.
    /// This avoids malloc/free per transfer, which is critical for sustained
    /// throughput at high sample rates.
    fn recycle(&mut self, buf: Vec<u8>) -> UsbResult<()>;

    /// Cancel all pending transfers and stop the stream.
    ///
    /// After this call, `pending()` returns 0 and `next_completion()` will
    /// return an error.
    fn stop(&mut self) -> UsbResult<()>;

    /// Number of transfers currently in-flight (submitted but not yet completed).
    fn pending(&self) -> usize;
}

// Verify that BulkStreamReader is object-safe.
fn _assert_object_safe(_: &dyn BulkStreamReader) {}

#[cfg(test)]
mod tests {
    use super::*;

    // Compile-time check that the trait is object-safe.
    fn _use_as_dyn(_reader: Box<dyn BulkStreamReader>) {}
}
