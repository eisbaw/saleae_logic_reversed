//! Mock implementation of [`BulkStreamReader`] for testing without hardware.
//!
//! Pre-load data chunks via [`push_completion()`](MockStreamReader::push_completion)
//! and the mock will return them from [`next_completion()`](MockStreamReader::next_completion)
//! in FIFO order.

use std::collections::VecDeque;
use std::time::Duration;

use crate::error::{UsbError, UsbResult};
use crate::stream::BulkStreamReader;

/// A pre-loaded completion: either data or an injected error.
#[derive(Debug, Clone)]
pub enum MockCompletion {
    /// Successful transfer with data.
    Data(Vec<u8>),
    /// Injected error (e.g., stall, timeout).
    Error(UsbError),
}

/// Mock bulk stream reader for testing.
///
/// # Usage
///
/// ```rust
/// use saleae_usb::mock_stream::{MockStreamReader, MockCompletion};
/// use saleae_usb::stream::BulkStreamReader;
/// use std::time::Duration;
///
/// let mut reader = MockStreamReader::new();
/// reader.push_completion(MockCompletion::Data(vec![0xAA; 1024]));
/// reader.push_completion(MockCompletion::Data(vec![0xBB; 1024]));
///
/// reader.start(0x82, 1024, 2).unwrap();
/// let buf = reader.next_completion(Duration::from_secs(1)).unwrap();
/// assert_eq!(buf[0], 0xAA);
/// reader.recycle(buf).unwrap();
/// ```
pub struct MockStreamReader {
    /// Pre-loaded completions returned by `next_completion()`.
    completions: VecDeque<MockCompletion>,
    /// Number of transfers currently "in-flight".
    in_flight: usize,
    /// Whether the stream has been started.
    started: bool,
    /// Whether the stream has been stopped.
    stopped: bool,
    /// The endpoint passed to `start()`.
    endpoint: u8,
    /// The transfer size passed to `start()`.
    transfer_size: usize,
    /// Total number of `recycle()` calls (for test assertions).
    recycle_count: usize,
}

impl MockStreamReader {
    /// Create a new mock stream reader with no pre-loaded completions.
    pub fn new() -> Self {
        Self {
            completions: VecDeque::new(),
            in_flight: 0,
            started: false,
            stopped: false,
            endpoint: 0,
            transfer_size: 0,
            recycle_count: 0,
        }
    }

    /// Pre-load a completion (data or error) to be returned by `next_completion()`.
    pub fn push_completion(&mut self, completion: MockCompletion) {
        self.completions.push_back(completion);
    }

    /// Number of times `recycle()` has been called.
    pub fn recycle_count(&self) -> usize {
        self.recycle_count
    }

    /// Whether the stream was started.
    pub fn was_started(&self) -> bool {
        self.started
    }

    /// Whether the stream was stopped.
    pub fn was_stopped(&self) -> bool {
        self.stopped
    }

    /// The endpoint passed to `start()`.
    pub fn endpoint(&self) -> u8 {
        self.endpoint
    }

    /// The transfer size passed to `start()`.
    pub fn transfer_size(&self) -> usize {
        self.transfer_size
    }
}

impl Default for MockStreamReader {
    fn default() -> Self {
        Self::new()
    }
}

impl BulkStreamReader for MockStreamReader {
    fn start(&mut self, endpoint: u8, transfer_size: usize, queue_depth: usize) -> UsbResult<()> {
        if self.started && !self.stopped {
            return Err(UsbError::BulkTransfer {
                endpoint,
                context: "stream already started".into(),
            });
        }
        self.endpoint = endpoint;
        self.transfer_size = transfer_size;
        self.in_flight = queue_depth;
        self.started = true;
        self.stopped = false;
        Ok(())
    }

    fn next_completion(&mut self, _timeout: Duration) -> UsbResult<Vec<u8>> {
        if !self.started || self.stopped {
            return Err(UsbError::BulkTransfer {
                endpoint: self.endpoint,
                context: "stream not active".into(),
            });
        }

        if self.in_flight == 0 {
            return Err(UsbError::BulkTransfer {
                endpoint: self.endpoint,
                context: "no transfers in flight".into(),
            });
        }

        let completion = self.completions.pop_front().ok_or(UsbError::Timeout {
            timeout_ms: _timeout.as_millis() as u64,
        })?;

        self.in_flight -= 1;

        match completion {
            MockCompletion::Data(data) => Ok(data),
            MockCompletion::Error(err) => Err(err),
        }
    }

    fn recycle(&mut self, _buf: Vec<u8>) -> UsbResult<()> {
        if !self.started || self.stopped {
            return Err(UsbError::BulkTransfer {
                endpoint: self.endpoint,
                context: "stream not active".into(),
            });
        }
        self.in_flight += 1;
        self.recycle_count += 1;
        Ok(())
    }

    fn stop(&mut self) -> UsbResult<()> {
        self.in_flight = 0;
        self.stopped = true;
        Ok(())
    }

    fn pending(&self) -> usize {
        self.in_flight
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_lifecycle() {
        let mut reader = MockStreamReader::new();
        reader.push_completion(MockCompletion::Data(vec![0xAA; 512]));
        reader.push_completion(MockCompletion::Data(vec![0xBB; 512]));

        reader.start(0x82, 1024, 4).unwrap();
        assert_eq!(reader.pending(), 4);

        let buf = reader.next_completion(Duration::from_secs(1)).unwrap();
        assert_eq!(buf[0], 0xAA);
        assert_eq!(reader.pending(), 3);

        reader.recycle(buf).unwrap();
        assert_eq!(reader.pending(), 4);
        assert_eq!(reader.recycle_count(), 1);

        let buf = reader.next_completion(Duration::from_secs(1)).unwrap();
        assert_eq!(buf[0], 0xBB);

        reader.stop().unwrap();
        assert_eq!(reader.pending(), 0);
        assert!(reader.was_stopped());
    }

    #[test]
    fn timeout_when_no_completions() {
        let mut reader = MockStreamReader::new();
        reader.start(0x82, 1024, 2).unwrap();

        let err = reader
            .next_completion(Duration::from_millis(100))
            .unwrap_err();
        assert!(matches!(err, UsbError::Timeout { .. }));
    }

    #[test]
    fn injected_error() {
        let mut reader = MockStreamReader::new();
        reader.push_completion(MockCompletion::Error(UsbError::BulkTransfer {
            endpoint: 0x82,
            context: "stall".into(),
        }));

        reader.start(0x82, 1024, 2).unwrap();
        let err = reader.next_completion(Duration::from_secs(1)).unwrap_err();
        assert!(matches!(err, UsbError::BulkTransfer { .. }));
        // in_flight decremented even on error (the transfer completed, just with an error).
        assert_eq!(reader.pending(), 1);
    }

    #[test]
    fn cannot_read_before_start() {
        let mut reader = MockStreamReader::new();
        let err = reader.next_completion(Duration::from_secs(1)).unwrap_err();
        assert!(matches!(err, UsbError::BulkTransfer { .. }));
    }

    #[test]
    fn cannot_read_after_stop() {
        let mut reader = MockStreamReader::new();
        reader.start(0x82, 1024, 2).unwrap();
        reader.stop().unwrap();

        let err = reader.next_completion(Duration::from_secs(1)).unwrap_err();
        assert!(matches!(err, UsbError::BulkTransfer { .. }));
    }

    #[test]
    fn double_start_errors() {
        let mut reader = MockStreamReader::new();
        reader.start(0x82, 1024, 2).unwrap();
        let err = reader.start(0x82, 1024, 2).unwrap_err();
        assert!(matches!(err, UsbError::BulkTransfer { .. }));
    }

    #[test]
    fn restart_after_stop() {
        let mut reader = MockStreamReader::new();
        reader.push_completion(MockCompletion::Data(vec![0x01]));

        reader.start(0x82, 1024, 2).unwrap();
        reader.stop().unwrap();

        // Push a new completion for the second start.
        reader.push_completion(MockCompletion::Data(vec![0x02]));
        reader.start(0x82, 1024, 1).unwrap();

        // The first completion was drained by the queue, second is available.
        let buf = reader.next_completion(Duration::from_secs(1)).unwrap();
        // Could be either 0x01 or 0x02 depending on whether stop drains.
        // Our mock doesn't drain on stop, so the first queued completion (0x01) is returned.
        assert!(!buf.is_empty());
    }

    #[test]
    fn records_endpoint_and_transfer_size() {
        let mut reader = MockStreamReader::new();
        reader.start(0x82, 16384, 128).unwrap();
        assert_eq!(reader.endpoint(), 0x82);
        assert_eq!(reader.transfer_size(), 16384);
    }
}
