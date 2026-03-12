//! Mock USB transport for testing without hardware.
//!
//! The [`MockTransport`] records all operations and can be pre-loaded with
//! expected responses. This enables deterministic testing of protocol logic
//! against known byte sequences.

use std::collections::VecDeque;
use std::sync::Mutex;

use crate::error::{UsbError, UsbResult};
use crate::transport::{ControlTransferParams, UsbTransport};

/// A recorded USB operation.
#[derive(Debug, Clone)]
pub enum UsbOperation {
    /// Control transfer OUT with params and data.
    ControlOut {
        bm_request_type: u8,
        b_request: u8,
        w_value: u16,
        w_index: u16,
        data: Vec<u8>,
    },
    /// Control transfer IN with params.
    ControlIn {
        bm_request_type: u8,
        b_request: u8,
        w_value: u16,
        w_index: u16,
        received_len: usize,
    },
    /// Bulk transfer OUT.
    BulkOut { endpoint: u8, data: Vec<u8> },
    /// Bulk transfer IN.
    BulkIn { endpoint: u8, received_len: usize },
    /// Device reset.
    Reset,
}

/// A pre-loaded bulk IN response: either data or an injected error.
#[derive(Debug, Clone)]
pub enum BulkInResponse {
    /// Successful response with data.
    Data(u8, Vec<u8>),
    /// Injected error for the given endpoint.
    Error(u8, UsbError),
}

/// Mock USB transport for testing.
///
/// # Usage
///
/// ```rust
/// use saleae_usb::mock::MockTransport;
///
/// let mock = MockTransport::new();
///
/// // Pre-load responses for control IN and bulk IN transfers.
/// mock.push_control_in_response(vec![0xAA, 0xBB]);
/// mock.push_bulk_in_response(0x81, vec![0x01, 0x02, 0x03]);
///
/// // Use `mock` as a `dyn UsbTransport` in driver code.
/// // After the test, inspect `mock.operations()` to verify what was sent.
/// ```
pub struct MockTransport {
    /// Recorded operations (in order).
    operations: Mutex<Vec<UsbOperation>>,
    /// Pre-loaded responses for control IN transfers.
    control_in_responses: Mutex<VecDeque<Vec<u8>>>,
    /// Pre-loaded responses for bulk IN transfers, keyed by endpoint.
    bulk_in_responses: Mutex<VecDeque<BulkInResponse>>,
}

impl MockTransport {
    /// Create a new mock transport with no pre-loaded responses.
    pub fn new() -> Self {
        Self {
            operations: Mutex::new(Vec::new()),
            control_in_responses: Mutex::new(VecDeque::new()),
            bulk_in_responses: Mutex::new(VecDeque::new()),
        }
    }

    /// Pre-load a response that will be returned by the next `control_transfer_in` call.
    pub fn push_control_in_response(&self, data: Vec<u8>) {
        self.control_in_responses.lock().unwrap().push_back(data);
    }

    /// Pre-load a response for a bulk IN transfer on the given endpoint.
    pub fn push_bulk_in_response(&self, endpoint: u8, data: Vec<u8>) {
        self.bulk_in_responses
            .lock()
            .unwrap()
            .push_back(BulkInResponse::Data(endpoint, data));
    }

    /// Pre-load an error for the next bulk IN transfer on the given endpoint.
    ///
    /// When the driver reads from this endpoint, the mock will return this
    /// error instead of data. Useful for testing error recovery paths.
    pub fn push_bulk_in_error(&self, endpoint: u8, error: UsbError) {
        self.bulk_in_responses
            .lock()
            .unwrap()
            .push_back(BulkInResponse::Error(endpoint, error));
    }

    /// Return all recorded operations.
    pub fn operations(&self) -> Vec<UsbOperation> {
        self.operations.lock().unwrap().clone()
    }

    /// Return the number of recorded operations.
    pub fn operation_count(&self) -> usize {
        self.operations.lock().unwrap().len()
    }

    /// Clear all recorded operations.
    pub fn clear(&self) {
        self.operations.lock().unwrap().clear();
    }
}

impl Default for MockTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl UsbTransport for MockTransport {
    fn control_transfer_out(
        &self,
        params: &ControlTransferParams,
        data: &[u8],
    ) -> UsbResult<usize> {
        self.operations
            .lock()
            .unwrap()
            .push(UsbOperation::ControlOut {
                bm_request_type: params.bm_request_type,
                b_request: params.b_request,
                w_value: params.w_value,
                w_index: params.w_index,
                data: data.to_vec(),
            });
        Ok(data.len())
    }

    fn control_transfer_in(
        &self,
        params: &ControlTransferParams,
        buf: &mut [u8],
    ) -> UsbResult<usize> {
        let response = self
            .control_in_responses
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| UsbError::UnexpectedResponse {
                context: "no mock control IN response queued".into(),
            })?;

        let len = response.len().min(buf.len());
        buf[..len].copy_from_slice(&response[..len]);

        self.operations
            .lock()
            .unwrap()
            .push(UsbOperation::ControlIn {
                bm_request_type: params.bm_request_type,
                b_request: params.b_request,
                w_value: params.w_value,
                w_index: params.w_index,
                received_len: len,
            });

        Ok(len)
    }

    fn bulk_transfer_out(&self, endpoint: u8, data: &[u8]) -> UsbResult<usize> {
        self.operations.lock().unwrap().push(UsbOperation::BulkOut {
            endpoint,
            data: data.to_vec(),
        });
        Ok(data.len())
    }

    fn bulk_transfer_in(&self, endpoint: u8, buf: &mut [u8]) -> UsbResult<usize> {
        let queued = self
            .bulk_in_responses
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| UsbError::UnexpectedResponse {
                context: format!("no mock bulk IN response queued for endpoint {endpoint:#04x}"),
            })?;

        match queued {
            BulkInResponse::Error(resp_ep, err) => {
                if resp_ep != endpoint {
                    return Err(UsbError::UnexpectedResponse {
                        context: format!(
                            "mock bulk IN error was for endpoint {resp_ep:#04x}, \
                             but read was on {endpoint:#04x}"
                        ),
                    });
                }
                Err(err)
            }
            BulkInResponse::Data(resp_ep, response) => {
                if resp_ep != endpoint {
                    return Err(UsbError::UnexpectedResponse {
                        context: format!(
                            "mock bulk IN response was for endpoint {resp_ep:#04x}, \
                             but read was on {endpoint:#04x}"
                        ),
                    });
                }

                let len = response.len().min(buf.len());
                buf[..len].copy_from_slice(&response[..len]);

                self.operations.lock().unwrap().push(UsbOperation::BulkIn {
                    endpoint,
                    received_len: len,
                });

                Ok(len)
            }
        }
    }

    fn reset(&self) -> UsbResult<()> {
        self.operations.lock().unwrap().push(UsbOperation::Reset);
        Ok(())
    }
}

/// Blanket impl so `Arc<MockTransport>` can be used as `Box<dyn UsbTransport>`
/// without a manual wrapper struct. This eliminates the need for the
/// `MockTransportWrapper` newtype that was previously copy-pasted across every
/// test module in `saleae-driver`.
impl UsbTransport for std::sync::Arc<MockTransport> {
    fn control_transfer_out(
        &self,
        params: &ControlTransferParams,
        data: &[u8],
    ) -> UsbResult<usize> {
        (**self).control_transfer_out(params, data)
    }

    fn control_transfer_in(
        &self,
        params: &ControlTransferParams,
        buf: &mut [u8],
    ) -> UsbResult<usize> {
        (**self).control_transfer_in(params, buf)
    }

    fn bulk_transfer_out(&self, endpoint: u8, data: &[u8]) -> UsbResult<usize> {
        (**self).bulk_transfer_out(endpoint, data)
    }

    fn bulk_transfer_in(&self, endpoint: u8, buf: &mut [u8]) -> UsbResult<usize> {
        (**self).bulk_transfer_in(endpoint, buf)
    }

    fn reset(&self) -> UsbResult<()> {
        (**self).reset()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_records_control_out() {
        let mock = MockTransport::new();
        let params = ControlTransferParams {
            bm_request_type: 0x40,
            b_request: 0xA0,
            w_value: 0x1234,
            w_index: 0x0000,
        };

        let n = mock.control_transfer_out(&params, &[0x01, 0x02]).unwrap();
        assert_eq!(n, 2);
        assert_eq!(mock.operation_count(), 1);

        let ops = mock.operations();
        match &ops[0] {
            UsbOperation::ControlOut {
                b_request,
                w_value,
                data,
                ..
            } => {
                assert_eq!(*b_request, 0xA0);
                assert_eq!(*w_value, 0x1234);
                assert_eq!(data, &[0x01, 0x02]);
            }
            other => panic!("expected ControlOut, got {other:?}"),
        }
    }

    #[test]
    fn mock_returns_queued_control_in() {
        let mock = MockTransport::new();
        mock.push_control_in_response(vec![0xAA, 0xBB, 0xCC]);

        let params = ControlTransferParams {
            bm_request_type: 0xC0,
            b_request: 0xA0,
            w_value: 0,
            w_index: 0,
        };

        let mut buf = [0u8; 4];
        let n = mock.control_transfer_in(&params, &mut buf).unwrap();
        assert_eq!(n, 3);
        assert_eq!(buf[..3], [0xAA, 0xBB, 0xCC]);
    }

    #[test]
    fn mock_returns_queued_bulk_in() {
        let mock = MockTransport::new();
        mock.push_bulk_in_response(0x81, vec![0x01, 0x02]);

        let mut buf = [0u8; 16];
        let n = mock.bulk_transfer_in(0x81, &mut buf).unwrap();
        assert_eq!(n, 2);
        assert_eq!(buf[..2], [0x01, 0x02]);
    }

    #[test]
    fn mock_bulk_in_wrong_endpoint_errors() {
        let mock = MockTransport::new();
        mock.push_bulk_in_response(0x81, vec![0x01]);

        let mut buf = [0u8; 16];
        let err = mock.bulk_transfer_in(0x82, &mut buf).unwrap_err();
        assert!(matches!(err, UsbError::UnexpectedResponse { .. }));
    }

    #[test]
    fn mock_no_response_errors() {
        let mock = MockTransport::new();
        let params = ControlTransferParams {
            bm_request_type: 0xC0,
            b_request: 0xA0,
            w_value: 0,
            w_index: 0,
        };
        let mut buf = [0u8; 4];
        let err = mock.control_transfer_in(&params, &mut buf).unwrap_err();
        assert!(matches!(err, UsbError::UnexpectedResponse { .. }));
    }

    #[test]
    fn mock_records_bulk_out() {
        let mock = MockTransport::new();
        let n = mock.bulk_transfer_out(0x01, &[0xDE, 0xAD]).unwrap();
        assert_eq!(n, 2);

        let ops = mock.operations();
        match &ops[0] {
            UsbOperation::BulkOut { endpoint, data } => {
                assert_eq!(*endpoint, 0x01);
                assert_eq!(data, &[0xDE, 0xAD]);
            }
            other => panic!("expected BulkOut, got {other:?}"),
        }
    }

    #[test]
    fn mock_records_reset() {
        let mock = MockTransport::new();
        mock.reset().unwrap();
        assert!(matches!(mock.operations()[0], UsbOperation::Reset));
    }
}
