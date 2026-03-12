//! Framed protocol handshake after firmware upload.
//!
//! After the FX3 boots with uploaded firmware, it starts in a "framed
//! protocol" mode with 0xA7/0xFE prefix packets. The host must complete
//! this handshake before EP1 encrypted commands will work. Without it,
//! the EP2 DMA channel may not be configured for capture data.
//!
//! The handshake uses raw bulk transfers on EP1 OUT (0x01) / EP1 IN (0x81),
//! *not* the encrypted command channel. It must complete before the first
//! ReseedDevice/Ping.
//!
//! # Protocol
//!
//! Each framed packet is: `[0xA7, 0xFE, opcode, payload...]`.
//! Poll packets are: `[0xA8]`.
//!
//! Sequence observed in pcap:
//! 1. Opcode 0x28: device query (`[0x11, 0x00]`)
//! 2. Opcode 0x29: session identifier (`[0x65, "!SAL-DRIVER-0001"]`)
//! 3. Poll 3 times (150ms apart) for async responses
//! 4. Opcode 0x2A: enable subsystem A (`[0x41, 0x00]`)
//! 5. Opcode 0x2B: enable subsystem B (`[0xA1, 0x00]`)

use std::time::Duration;

use saleae_proto::device::endpoint;
use saleae_usb::transport::UsbTransport;
use tracing::{debug, info};

use crate::error::DriverResult;

/// Maximum response size for framed protocol packets.
const FRAMED_RESPONSE_BUF_SIZE: usize = 256;

/// Session identifier sent during framed handshake.
///
/// The official software sends a UUID; we use a fixed string since the
/// device doesn't appear to validate it beyond presence.
const SESSION_ID: &[u8] = b"!SAL-DRIVER-0001";

/// Number of poll rounds after the session identifier.
const POLL_ROUNDS: usize = 3;

/// Delay between poll rounds.
const POLL_DELAY: Duration = Duration::from_millis(150);

/// Perform the framed protocol handshake on a raw transport.
///
/// This must be called after firmware upload and re-enumeration, but
/// before creating an `Ep1CommandChannel` or sending any encrypted
/// commands.
///
/// The transport must already have the USB interface claimed.
pub fn framed_handshake(transport: &dyn UsbTransport) -> DriverResult<()> {
    info!("Starting framed protocol handshake");

    // Opcode 0x28: device query
    let resp = send_framed(transport, 0x28, &[0x11, 0x00])?;
    debug!("framed 0x28 response: {} bytes: {:02x?}", resp.len(), resp);

    // Opcode 0x29: session identifier
    let mut payload = vec![0x65];
    payload.extend_from_slice(SESSION_ID);
    let resp = send_framed(transport, 0x29, &payload)?;
    debug!("framed 0x29 response: {} bytes: {:02x?}", resp.len(), resp);

    // Poll for async responses
    for i in 0..POLL_ROUNDS {
        std::thread::sleep(POLL_DELAY);
        let resp = poll_framed(transport)?;
        debug!(
            "framed poll #{}: {} bytes: {:02x?}",
            i + 1,
            resp.len(),
            resp
        );
    }

    // Opcode 0x2A: enable subsystem A
    let resp = send_framed(transport, 0x2A, &[0x41, 0x00])?;
    debug!("framed 0x2A response: {} bytes: {:02x?}", resp.len(), resp);

    // Opcode 0x2B: enable subsystem B
    let resp = send_framed(transport, 0x2B, &[0xA1, 0x00])?;
    debug!("framed 0x2B response: {} bytes: {:02x?}", resp.len(), resp);

    info!("Framed protocol handshake complete");
    Ok(())
}

/// Send a framed protocol packet and read the response.
///
/// Packet format: `[0xA7, 0xFE, opcode, payload...]`
fn send_framed(transport: &dyn UsbTransport, opcode: u8, payload: &[u8]) -> DriverResult<Vec<u8>> {
    let mut pkt = Vec::with_capacity(3 + payload.len());
    pkt.push(0xA7);
    pkt.push(0xFE);
    pkt.push(opcode);
    pkt.extend_from_slice(payload);

    debug!(
        "framed send: opcode={opcode:#04x}, {} payload bytes",
        payload.len()
    );
    transport.bulk_transfer_out(endpoint::BULK_OUT, &pkt)?;

    let mut resp = vec![0u8; FRAMED_RESPONSE_BUF_SIZE];
    let n = transport.bulk_transfer_in(endpoint::BULK_IN, &mut resp)?;
    resp.truncate(n);
    Ok(resp)
}

/// Poll for an async framed response.
///
/// Sends `[0xA8]` and reads the response. Used to drain async
/// notifications from the device after the session identifier.
fn poll_framed(transport: &dyn UsbTransport) -> DriverResult<Vec<u8>> {
    transport.bulk_transfer_out(endpoint::BULK_OUT, &[0xA8])?;

    let mut resp = vec![0u8; FRAMED_RESPONSE_BUF_SIZE];
    let n = transport.bulk_transfer_in(endpoint::BULK_IN, &mut resp)?;
    resp.truncate(n);
    Ok(resp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use saleae_usb::mock::{MockTransport, UsbOperation};

    #[test]
    fn framed_handshake_sends_correct_packets() {
        let mock = MockTransport::new();

        // Pre-load bulk IN responses for all 8 read operations:
        // 2 framed commands (0x28, 0x29) + 3 polls + 2 framed commands (0x2A, 0x2B)
        let ep_in = endpoint::BULK_IN;
        mock.push_bulk_in_response(ep_in, vec![0x01]); // 0x28 response
        mock.push_bulk_in_response(ep_in, vec![0x02]); // 0x29 response
        mock.push_bulk_in_response(ep_in, vec![0x00]); // poll 1
        mock.push_bulk_in_response(ep_in, vec![0x00]); // poll 2
        mock.push_bulk_in_response(ep_in, vec![0x00]); // poll 3
        mock.push_bulk_in_response(ep_in, vec![0x03]); // 0x2A response
        mock.push_bulk_in_response(ep_in, vec![0x04]); // 0x2B response

        let result = framed_handshake(&mock);
        assert!(result.is_ok(), "framed handshake failed: {result:?}");

        // Verify the outbound packets.
        let ops = mock.operations();
        let bulk_outs: Vec<&Vec<u8>> = ops
            .iter()
            .filter_map(|op| {
                if let UsbOperation::BulkOut { data, .. } = op {
                    Some(data)
                } else {
                    None
                }
            })
            .collect();

        assert_eq!(bulk_outs.len(), 7, "expected 7 bulk OUT transfers");

        // 0x28: device query
        assert_eq!(bulk_outs[0], &vec![0xA7, 0xFE, 0x28, 0x11, 0x00]);

        // 0x29: session identifier
        let mut expected_session = vec![0xA7, 0xFE, 0x29, 0x65];
        expected_session.extend_from_slice(SESSION_ID);
        assert_eq!(bulk_outs[1], &expected_session);

        // 3 polls
        for i in 0..3 {
            assert_eq!(bulk_outs[2 + i], &vec![0xA8]);
        }

        // 0x2A, 0x2B
        assert_eq!(bulk_outs[5], &vec![0xA7, 0xFE, 0x2A, 0x41, 0x00]);
        assert_eq!(bulk_outs[6], &vec![0xA7, 0xFE, 0x2B, 0xA1, 0x00]);
    }
}
