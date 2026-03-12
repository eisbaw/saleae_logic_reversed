//! End-to-end integration tests with mock USB sequences.
//!
//! These tests exercise the full driver lifecycle through the public API,
//! using MockTransport with pre-programmed responses. They verify that the
//! correct USB operations are sent in the correct order for each phase of
//! device operation.
//!
//! Unlike unit tests inside each module, these tests:
//! - Only use public API (no `pub(crate)` access)
//! - Exercise multi-module sequences (firmware -> FPGA -> capture)
//! - Use `EncryptionMode::None` to test protocol flow without encryption
//!   (encryption correctness is separately tested in `obfuscation.rs`)

use std::sync::Arc;

use saleae_driver::capture::{CaptureConfig, EP2_CAPTURE_IN};
use saleae_driver::ep1::Ep1CommandChannel;
use saleae_driver::firmware::upload_fx3_firmware;
use saleae_driver::fpga::{common_device_init, fpga_post_init, program_fpga};
use saleae_proto::capture_data::{ChunkHeader, StartPacketFields};
use saleae_proto::device::{endpoint, UsbSpeed, LOGIC_PRO_8_PROFILE};
use saleae_proto::firmware::fx3::{Fx3Header, Fx3Image, Fx3Section, IMAGE_TYPE_NORMAL};
use saleae_proto::obfuscation::EncryptionMode;
use saleae_usb::mock::{MockTransport, UsbOperation};
use saleae_usb::transport::UsbTransport;
use saleae_usb::UsbError;

// ---------------------------------------------------------------------------
// Test infrastructure
// ---------------------------------------------------------------------------

/// Wrapper to share a MockTransport via Arc while implementing UsbTransport.
///
/// Integration tests (external to the crate) cannot access the internal
/// MockTransportWrapper used in unit tests. This provides the same pattern.
struct SharedMock(Arc<MockTransport>);

impl UsbTransport for SharedMock {
    fn control_transfer_out(
        &self,
        params: &saleae_usb::transport::ControlTransferParams,
        data: &[u8],
    ) -> saleae_usb::UsbResult<usize> {
        self.0.control_transfer_out(params, data)
    }

    fn control_transfer_in(
        &self,
        params: &saleae_usb::transport::ControlTransferParams,
        buf: &mut [u8],
    ) -> saleae_usb::UsbResult<usize> {
        self.0.control_transfer_in(params, buf)
    }

    fn bulk_transfer_out(&self, endpoint: u8, data: &[u8]) -> saleae_usb::UsbResult<usize> {
        self.0.bulk_transfer_out(endpoint, data)
    }

    fn bulk_transfer_in(&self, endpoint: u8, buf: &mut [u8]) -> saleae_usb::UsbResult<usize> {
        self.0.bulk_transfer_in(endpoint, buf)
    }

    fn reset(&self) -> saleae_usb::UsbResult<()> {
        self.0.reset()
    }
}

/// Create an Ep1CommandChannel with EncryptionMode::None, already initialized
/// via the public `initialize()` method.
///
/// Pre-loads the mock with the responses needed for the initialize() handshake:
/// - Ping response: 0xAA (complement of 0x55)
///
/// With EncryptionMode::None, the reseed is a raw 4-byte seed (no XOR), and
/// the ping command is sent as [0x7D, 0x55] with response 0xAA unencrypted.
fn make_initialized_channel(mock: &Arc<MockTransport>) -> Ep1CommandChannel {
    // Queue response for ping during initialize().
    mock.push_bulk_in_response(endpoint::BULK_IN, vec![0xAA]);

    let transport = Box::new(SharedMock(mock.clone()));
    let mut channel = Ep1CommandChannel::new(transport, EncryptionMode::None);
    channel.initialize().expect("initialize should succeed");
    assert!(channel.is_initialized());

    // Clear the recorded operations from initialization so tests
    // only see operations from the code under test.
    mock.clear();

    channel
}

/// Build a minimal test FX3 firmware image.
fn make_test_firmware() -> Fx3Image {
    Fx3Image {
        header: Fx3Header {
            image_ctl: 0x00,
            image_type: IMAGE_TYPE_NORMAL,
        },
        sections: vec![
            Fx3Section {
                address: 0x0000_0100,
                data: vec![0xAAu8; 256],
            },
            Fx3Section {
                address: 0x4000_3000,
                data: vec![0xBBu8; 128],
            },
        ],
        entry_address: 0x4000_e020,
        checksum: 0,
    }
}

/// Build an EP2 start packet as raw bytes (ready for mock bulk IN response).
fn build_start_packet(channel: u8, config_id: u16, timestamp: u32, time: u64) -> Vec<u8> {
    let header = ChunkHeader {
        channel,
        software: false,
        is_start: true,
        config_id,
    };
    let start = StartPacketFields { timestamp, time };
    let mut packet = Vec::new();
    packet.extend_from_slice(&header.to_bytes());
    packet.extend_from_slice(&start.to_bytes());
    packet
}

/// Build an EP2 data packet as raw bytes (no stats).
fn build_data_packet(channel: u8, config_id: u16, sample_data: &[u8]) -> Vec<u8> {
    let header = ChunkHeader {
        channel,
        software: false,
        is_start: false,
        config_id,
    };
    let mut packet = Vec::new();
    packet.extend_from_slice(&header.to_bytes());
    packet.extend_from_slice(sample_data);
    packet
}

/// Count BulkOut operations in a list of UsbOperations.
fn count_bulk_out(ops: &[UsbOperation]) -> usize {
    ops.iter()
        .filter(|op| matches!(op, UsbOperation::BulkOut { .. }))
        .count()
}

/// Count BulkIn operations in a list of UsbOperations.
fn count_bulk_in(ops: &[UsbOperation]) -> usize {
    ops.iter()
        .filter(|op| matches!(op, UsbOperation::BulkIn { .. }))
        .count()
}

/// Count ControlOut operations in a list of UsbOperations.
fn count_control_out(ops: &[UsbOperation]) -> usize {
    ops.iter()
        .filter(|op| matches!(op, UsbOperation::ControlOut { .. }))
        .count()
}

/// Extract all BulkOut data payloads from recorded operations.
fn extract_bulk_out_data(ops: &[UsbOperation]) -> Vec<Vec<u8>> {
    ops.iter()
        .filter_map(|op| match op {
            UsbOperation::BulkOut { data, .. } => Some(data.clone()),
            _ => None,
        })
        .collect()
}

// ===========================================================================
// Test 1: Firmware upload sequence
// ===========================================================================

#[test]
fn firmware_upload_sends_correct_control_transfers() {
    let mock = MockTransport::new();
    let image = make_test_firmware();

    upload_fx3_firmware(&mock, &image).unwrap();

    let ops = mock.operations();

    // Two sections (each fits in one chunk) + one zero-length entry transfer.
    assert_eq!(
        count_control_out(&ops),
        3,
        "expected 3 control OUT transfers (2 sections + 1 entry)"
    );

    // Verify all transfers use the FX3 vendor request format.
    for op in &ops {
        match op {
            UsbOperation::ControlOut {
                bm_request_type,
                b_request,
                ..
            } => {
                assert_eq!(
                    *bm_request_type, 0x40,
                    "bmRequestType should be vendor host-to-device"
                );
                assert_eq!(*b_request, 0xA0, "bRequest should be FX3 FW_UPLOAD");
            }
            _ => panic!("expected only ControlOut operations during firmware upload"),
        }
    }

    // First section: address 0x00000100.
    match &ops[0] {
        UsbOperation::ControlOut {
            w_value,
            w_index,
            data,
            ..
        } => {
            assert_eq!(*w_value, 0x0100, "section 1 w_value (low 16 of addr)");
            assert_eq!(*w_index, 0x0000, "section 1 w_index (high 16 of addr)");
            assert_eq!(data.len(), 256, "section 1 data length");
        }
        _ => unreachable!(),
    }

    // Second section: address 0x40003000.
    match &ops[1] {
        UsbOperation::ControlOut {
            w_value,
            w_index,
            data,
            ..
        } => {
            assert_eq!(*w_value, 0x3000, "section 2 w_value");
            assert_eq!(*w_index, 0x4000, "section 2 w_index");
            assert_eq!(data.len(), 128, "section 2 data length");
        }
        _ => unreachable!(),
    }

    // Entry trigger: zero-length transfer to entry address 0x4000e020.
    match &ops[2] {
        UsbOperation::ControlOut {
            w_value,
            w_index,
            data,
            ..
        } => {
            assert_eq!(*w_value, 0xe020, "entry w_value");
            assert_eq!(*w_index, 0x4000, "entry w_index");
            assert!(data.is_empty(), "entry transfer should be zero-length");
        }
        _ => unreachable!(),
    }
}

#[test]
fn firmware_upload_large_section_chunks_at_4096() {
    let mock = MockTransport::new();
    let image = Fx3Image {
        header: Fx3Header {
            image_ctl: 0x00,
            image_type: IMAGE_TYPE_NORMAL,
        },
        sections: vec![Fx3Section {
            address: 0x4000_3000,
            // 10000 bytes = ceil(10000/4096) = 3 chunks
            data: vec![0xCCu8; 10000],
        }],
        entry_address: 0x4000_e020,
        checksum: 0,
    };

    upload_fx3_firmware(&mock, &image).unwrap();

    let ops = mock.operations();
    // 3 data chunks + 1 entry = 4.
    assert_eq!(count_control_out(&ops), 4);

    // Verify chunk addresses are sequential.
    let addrs: Vec<u32> = ops
        .iter()
        .filter_map(|op| match op {
            UsbOperation::ControlOut {
                w_value,
                w_index,
                data,
                ..
            } if !data.is_empty() => Some((*w_index as u32) << 16 | (*w_value as u32)),
            _ => None,
        })
        .collect();

    assert_eq!(addrs[0], 0x4000_3000, "first chunk at section start");
    assert_eq!(addrs[1], 0x4000_3000 + 4096, "second chunk offset by 4096");
    assert_eq!(addrs[2], 0x4000_3000 + 8192, "third chunk offset by 8192");
}

// ===========================================================================
// Test 2: FPGA programming sequence
// ===========================================================================

#[test]
fn fpga_programming_sends_reset_data_verify() {
    let mock = Arc::new(MockTransport::new());
    let mut channel = make_initialized_channel(&mock);

    let bitstream = vec![0xFF; 500]; // fits in one chunk (1020 max for Pro USB3)

    // Queue responses:
    // 1. ResetFpga ACK
    mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
    // 2. WriteConfigurationData ACK (one chunk)
    mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
    // 3-4. WriteRegisters ACKs (write 0xAA to r0x7F, twice)
    mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
    mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
    // 5. ReadRegisters response: 0xAA (success)
    mock.push_bulk_in_response(endpoint::BULK_IN, vec![0xAA]);

    program_fpga(
        &mut channel,
        &bitstream,
        &LOGIC_PRO_8_PROFILE,
        UsbSpeed::Super,
    )
    .unwrap();

    let ops = mock.operations();
    let out_data = extract_bulk_out_data(&ops);

    assert_eq!(
        out_data.len(),
        5,
        "expected 5 OUT transfers: reset + data + 2x write + read"
    );

    // With EncryptionMode::None, wire bytes are raw [opcode, payload...].
    // Op 0: ResetFpga (0x7E)
    assert_eq!(out_data[0][0], 0x7E, "first command: ResetFpga opcode");

    // Op 1: WriteConfigurationData (0x7F)
    assert_eq!(
        out_data[1][0], 0x7F,
        "second command: WriteConfigData opcode"
    );

    // Ops 2-3: WriteRegisters (0x80) writing 0xAA to r0x7F
    assert_eq!(out_data[2][0], 0x80, "third command: WriteRegisters opcode");
    assert_eq!(
        out_data[3][0], 0x80,
        "fourth command: WriteRegisters opcode"
    );

    // Op 4: ReadRegisters (0x81) to verify readback
    assert_eq!(out_data[4][0], 0x81, "fifth command: ReadRegisters opcode");
}

#[test]
fn fpga_programming_fails_on_wrong_readback() {
    let mock = Arc::new(MockTransport::new());
    let mut channel = make_initialized_channel(&mock);

    // Queue: reset ACK, config ACK, 2x write ACK, wrong readback
    mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
    mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
    mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]); // WriteRegisters ACK #1
    mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]); // WriteRegisters ACK #2
    mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x55]); // wrong! expected 0xAA

    let err = program_fpga(
        &mut channel,
        &[0xFF; 10],
        &LOGIC_PRO_8_PROFILE,
        UsbSpeed::Super,
    )
    .unwrap_err();

    let msg = err.to_string();
    assert!(
        msg.contains("0x55") && msg.contains("0xaa"),
        "error should reference actual and expected values: {msg}"
    );
}

#[test]
fn fpga_post_init_writes_registers_in_order() {
    let mock = Arc::new(MockTransport::new());
    let mut channel = make_initialized_channel(&mock);

    // Non-Lattice path: 5 register writes, no JESD/DDR.
    for _ in 0..5 {
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
    }

    fpga_post_init(&mut channel, false).unwrap();

    let ops = mock.operations();
    let out_data = extract_bulk_out_data(&ops);
    assert_eq!(
        out_data.len(),
        5,
        "5 register writes for non-Lattice post-init"
    );

    // With EncryptionMode::None, wire format is raw bytes.
    // First write: COMMIT_CONTROL (0x01C0) via 16-bit path.
    // 16-bit path sends: [addr_hi, addr_lo, val3, val2, val1, val0]
    // addr_hi = 0x01 (as opcode), addr_lo = 0xC0
    assert_eq!(out_data[0][0], 0x01, "COMMIT_CONTROL addr_hi");
    assert_eq!(out_data[0][1], 0xC0, "COMMIT_CONTROL addr_lo");

    // Remaining writes use 8-bit path: opcode 0x80 (WriteRegisters).
    for (i, data) in out_data.iter().enumerate().skip(1) {
        assert_eq!(data[0], 0x80, "register write {i} should use opcode 0x80");
    }

    // Verify specific register addresses in the 8-bit path payload.
    // WriteRegisters payload: [count=1, addr, value]
    // So out_data[i] = [0x80, 0x01, addr, value]
    assert_eq!(out_data[1][2], 0x40, "write 2: CAPTURE_CONTROL addr=0x40");
    assert_eq!(out_data[2][2], 0xE0, "write 3: INIT_CONFIG addr=0xE0");
    assert_eq!(
        out_data[3][2], 0x24,
        "write 4: GPIF_FLUSH_TIMEOUT addr=0x24"
    );
    assert_eq!(out_data[4][2], 0x20, "write 5: GPIF_ENABLE addr=0x20");
}

#[test]
fn fpga_full_programming_and_post_init_sequence() {
    let mock = Arc::new(MockTransport::new());
    let mut channel = make_initialized_channel(&mock);

    let bitstream = vec![0xAB; 100];

    // FPGA programming: reset + data + 2x write + verify
    mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]); // reset ACK
    mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]); // config data ACK
    mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]); // WriteRegisters ACK #1
    mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]); // WriteRegisters ACK #2
    mock.push_bulk_in_response(endpoint::BULK_IN, vec![0xAA]); // readback OK

    program_fpga(
        &mut channel,
        &bitstream,
        &LOGIC_PRO_8_PROFILE,
        UsbSpeed::Super,
    )
    .unwrap();

    // Post-init (non-Lattice): 5 register writes
    for _ in 0..5 {
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
    }

    fpga_post_init(&mut channel, false).unwrap();

    // Total: 5 (programming) + 5 (post-init) = 10 OUT transfers
    let ops = mock.operations();
    assert_eq!(count_bulk_out(&ops), 10, "programming + post-init total");
}

// ===========================================================================
// Test 3: EP1 reseed + ping handshake
// ===========================================================================

#[test]
fn ep1_initialize_sends_reseed_then_ping() {
    let mock = Arc::new(MockTransport::new());

    // Queue the ping response for initialize().
    mock.push_bulk_in_response(endpoint::BULK_IN, vec![0xAA]);

    let transport = Box::new(SharedMock(mock.clone()));
    let mut channel = Ep1CommandChannel::new(transport, EncryptionMode::None);

    assert!(!channel.is_initialized());
    channel.initialize().unwrap();
    assert!(channel.is_initialized());

    let ops = mock.operations();

    // With EncryptionMode::None:
    //   Op 0: BulkOut - 4-byte seed (raw, no control byte)
    //   Op 1: BulkOut - ping command [0x7D, 0x55]
    //   Op 2: BulkIn - ping response (0xAA)
    assert_eq!(ops.len(), 3, "initialize: seed OUT + ping OUT + ping IN");

    // Verify seed is 4 bytes.
    match &ops[0] {
        UsbOperation::BulkOut { endpoint: ep, data } => {
            assert_eq!(*ep, endpoint::BULK_OUT);
            assert_eq!(data.len(), 4, "reseed should be 4 bytes in None mode");
            // Seed should not be zero (initialize() avoids zero).
            let seed = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
            assert_ne!(seed, 0, "seed must not be zero (LFSR would get stuck)");
        }
        other => panic!("expected BulkOut for reseed, got {other:?}"),
    }

    // Verify ping command.
    match &ops[1] {
        UsbOperation::BulkOut { data, .. } => {
            assert_eq!(data, &[0x7D, 0x55], "ping command in None mode");
        }
        other => panic!("expected BulkOut for ping, got {other:?}"),
    }

    // Verify ping response was read.
    match &ops[2] {
        UsbOperation::BulkIn {
            endpoint: ep,
            received_len,
        } => {
            assert_eq!(*ep, endpoint::BULK_IN);
            assert_eq!(*received_len, 1);
        }
        other => panic!("expected BulkIn for ping response, got {other:?}"),
    }
}

#[test]
fn ep1_initialize_fails_on_wrong_ping_response() {
    let mock = Arc::new(MockTransport::new());

    // Queue wrong ping response.
    mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x55]); // wrong! expected 0xAA

    let transport = Box::new(SharedMock(mock.clone()));
    let mut channel = Ep1CommandChannel::new(transport, EncryptionMode::None);

    let err = channel.initialize().unwrap_err();
    assert!(!channel.is_initialized());

    let msg = err.to_string();
    assert!(
        msg.contains("ping") || msg.contains("mismatch"),
        "error should describe ping failure: {msg}"
    );
}

#[test]
fn ep1_ping_after_initialize_works() {
    let mock = Arc::new(MockTransport::new());
    let mut channel = make_initialized_channel(&mock);

    // Queue ping response.
    mock.push_bulk_in_response(endpoint::BULK_IN, vec![0xAA]);

    channel.ping().unwrap();

    let ops = mock.operations();
    assert_eq!(count_bulk_out(&ops), 1, "one ping OUT");
    assert_eq!(count_bulk_in(&ops), 1, "one ping IN");
}

#[test]
fn ep1_register_write_then_read_roundtrip() {
    let mock = Arc::new(MockTransport::new());
    let mut channel = make_initialized_channel(&mock);

    // WriteRegisters: queue ACK.
    mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
    // ReadRegisters: queue register value.
    mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x42]);

    channel.write_registers(&[(0x20, 0x42)]).unwrap();
    let values = channel.read_registers(&[0x20]).unwrap();

    assert_eq!(values, vec![0x42]);

    let ops = mock.operations();
    // 2 OUTs (write + read commands), 2 INs (write ACK + read response)
    assert_eq!(count_bulk_out(&ops), 2);
    assert_eq!(count_bulk_in(&ops), 2);
}

// ===========================================================================
// Test 4: Capture start -> stream EP2 data -> capture stop
// ===========================================================================

#[test]
fn capture_lifecycle_start_stream_stop() {
    let mock = Arc::new(MockTransport::new());
    let mut channel = make_initialized_channel(&mock);

    let config = CaptureConfig {
        sample_rate: 1_000_000,
        digital_channels: vec![0, 1],
        duration_samples: 0,
        voltage_threshold: 1.65,
        stats_enabled: false,
        samples_per_chunk: 256,
        ..CaptureConfig::default()
    };

    // --- Phase 1: Start capture ---
    // configure_capture writes:
    //   1. WriteRegisters (opcode 0x80): 1 OUT + 1 IN (ACK)
    //   2. ReadRegisters (opcode 0x81): 1 OUT + 1 IN (readback)
    // Then start_sampling (1 write + 1 response)
    // Total: 2 config + 1 start = 3 OUTs, 3 INs
    queue_capture_start_responses(&mock, &config);

    let mut stream = saleae_driver::capture::start_digital_capture(&mut channel, &config).unwrap();

    assert!(!stream.is_stopped());
    assert_eq!(stream.total_samples(), 0);

    // Record ops count after start.
    let ops_after_start = mock.operation_count();

    // --- Phase 2: Read data from EP2 ---
    // Send a start packet followed by a data packet.
    let start_pkt = build_start_packet(0, 1, 100, 200);
    mock.push_bulk_in_response(EP2_CAPTURE_IN, start_pkt);

    let chunk = stream.read_chunk(mock.as_ref()).unwrap();
    assert!(chunk.is_some(), "should receive start packet");
    let chunk = chunk.unwrap();
    assert!(chunk.header.is_start);
    assert_eq!(chunk.header.channel, 0);
    assert_eq!(chunk.start.unwrap().timestamp, 100);

    // Data packet with 32 bytes of sample data.
    let sample_data = vec![0xFF; 32];
    let data_pkt = build_data_packet(0, 1, &sample_data);
    mock.push_bulk_in_response(EP2_CAPTURE_IN, data_pkt);

    let chunk = stream.read_chunk(mock.as_ref()).unwrap();
    assert!(chunk.is_some(), "should receive data packet");
    let chunk = chunk.unwrap();
    assert!(!chunk.header.is_start);
    assert!(!chunk.sample_data.is_empty());

    // --- Phase 3: Stop capture ---
    // StopDataPath (0x86): sends command, tries to read response.
    mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
    // StopStream (0x02): sends command, reads response.
    mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);

    stream.stop(&mut channel).unwrap();
    assert!(stream.is_stopped());

    // Verify stop commands were sent.
    let all_ops = mock.operations();
    let stop_outs: Vec<_> = all_ops[ops_after_start..]
        .iter()
        .filter_map(|op| match op {
            UsbOperation::BulkOut { data, endpoint: ep } if *ep == endpoint::BULK_OUT => {
                Some(data.clone())
            }
            _ => None,
        })
        .collect();

    // The last two EP1 OUT commands should be StopDataPath and StopStream.
    // There are 2 EP2 BulkIn reads in between (for the data packets).
    assert!(
        stop_outs.len() >= 2,
        "should have at least 2 EP1 OUT commands after streaming"
    );

    // The final two should be 0x86 and 0x02.
    let last_two: Vec<_> = stop_outs.iter().rev().take(2).collect();
    assert_eq!(last_two[0][0], 0x02, "last command: StopStream (0x02)");
    assert_eq!(last_two[1][0], 0x86, "second-to-last: StopDataPath (0x86)");
}

/// Helper: queue mock responses for start_digital_capture.
///
/// configure_capture does:
/// 1. WriteRegisters (0x80): 1 OUT + 1 IN (1-byte ACK)
/// 2. ReadRegisters (0x81): 1 OUT + 1 IN (N-byte readback matching written values)
/// Then start_sampling: 1 OUT + 1 IN (2-byte response)
fn queue_capture_start_responses(mock: &MockTransport, config: &CaptureConfig) {
    // WriteRegisters ACK
    mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
    // ReadRegisters readback: return the written values so verification passes
    let pairs = saleae_driver::capture::build_capture_register_pairs(config);
    let readback: Vec<u8> = pairs.iter().map(|&(_, v)| v).collect();
    mock.push_bulk_in_response(endpoint::BULK_IN, readback);
    // StartSampling response
    mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00, 0x00]);
}

#[test]
fn capture_stream_returns_none_after_stop() {
    let mock = Arc::new(MockTransport::new());
    let mut channel = make_initialized_channel(&mock);

    let config = CaptureConfig {
        digital_channels: vec![0, 1],
        stats_enabled: false,
        ..CaptureConfig::default()
    };

    // Start capture.
    queue_capture_start_responses(&mock, &config);
    let mut stream = saleae_driver::capture::start_digital_capture(&mut channel, &config).unwrap();

    // Stop the stream.
    mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]); // StopDataPath
    mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]); // StopStream
    stream.stop(&mut channel).unwrap();

    // Subsequent reads should return None.
    let result = stream.read_chunk(mock.as_ref()).unwrap();
    assert!(result.is_none(), "read after stop should return None");
}

#[test]
fn capture_stop_is_idempotent() {
    let mock = Arc::new(MockTransport::new());
    let mut channel = make_initialized_channel(&mock);

    let config = CaptureConfig {
        digital_channels: vec![0, 1],
        stats_enabled: false,
        ..CaptureConfig::default()
    };

    // Start capture.
    queue_capture_start_responses(&mock, &config);
    let mut stream = saleae_driver::capture::start_digital_capture(&mut channel, &config).unwrap();

    // First stop.
    mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
    mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
    stream.stop(&mut channel).unwrap();

    let ops_after_first_stop = mock.operation_count();

    // Second stop should be a no-op (no additional operations).
    stream.stop(&mut channel).unwrap();
    assert_eq!(
        mock.operation_count(),
        ops_after_first_stop,
        "second stop should not send any operations"
    );
}

// ===========================================================================
// Test 5: Error recovery (USB disconnect during capture)
// ===========================================================================

#[test]
fn capture_stream_read_handles_usb_error() {
    let mock = Arc::new(MockTransport::new());
    let mut channel = make_initialized_channel(&mock);

    let config = CaptureConfig {
        digital_channels: vec![0, 1],
        stats_enabled: false,
        ..CaptureConfig::default()
    };

    // Start capture.
    queue_capture_start_responses(&mock, &config);
    let mut stream = saleae_driver::capture::start_digital_capture(&mut channel, &config).unwrap();

    // Inject a USB error on EP2 IN (simulating disconnect).
    mock.push_bulk_in_error(
        EP2_CAPTURE_IN,
        UsbError::BulkTransfer {
            endpoint: EP2_CAPTURE_IN,
            context: "device disconnected".into(),
        },
    );

    let result = stream.read_chunk(mock.as_ref());
    assert!(result.is_err(), "USB error should propagate");

    let err = result.unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("USB") || msg.contains("bulk") || msg.contains("disconnect"),
        "error should mention USB/bulk transfer issue: {msg}"
    );
}

#[test]
fn ep1_command_handles_bulk_out_failure() {
    let mock = Arc::new(MockTransport::new());

    // Initialize channel successfully first.
    mock.push_bulk_in_response(endpoint::BULK_IN, vec![0xAA]);
    let transport = Box::new(SharedMock(mock.clone()));
    let mut channel = Ep1CommandChannel::new(transport, EncryptionMode::None);
    channel.initialize().unwrap();
    mock.clear();

    // Now the channel is initialized but the next bulk IN will fail.
    // Queue a ping, but the response will be an error.
    mock.push_bulk_in_error(endpoint::BULK_IN, UsbError::Timeout { timeout_ms: 1000 });

    let result = channel.ping();
    assert!(result.is_err(), "timeout should propagate as error");

    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("USB") || msg.contains("timed out"),
        "error should mention timeout: {msg}"
    );
}

#[test]
fn fpga_programming_handles_usb_error_during_bitstream() {
    let mock = Arc::new(MockTransport::new());
    let mut channel = make_initialized_channel(&mock);

    // ResetFpga succeeds.
    mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
    // WriteConfigurationData ACK fails with USB error.
    mock.push_bulk_in_error(
        endpoint::BULK_IN,
        UsbError::BulkTransfer {
            endpoint: endpoint::BULK_IN,
            context: "pipe stall".into(),
        },
    );

    let result = program_fpga(
        &mut channel,
        &[0xFF; 100],
        &LOGIC_PRO_8_PROFILE,
        UsbSpeed::Super,
    );

    assert!(result.is_err(), "USB error during bitstream should fail");
}

// ===========================================================================
// Test 6: CommonDeviceInit (EEPROM device ID read)
// ===========================================================================

/// Queue mock responses for the Xilinx (non-Lattice) common_device_init path.
///
/// Returns the expected device ID.
fn queue_xilinx_common_device_init(mock: &MockTransport) -> u64 {
    let device_id: u64 = 0x764051b51bd4d9f1;

    // Helper: queue one dcm_cascade (5-step JESD: 5 write ACKs + r0x40 read + r0x4a read).
    let queue_one_cascade = |mock: &MockTransport, r40: u8| {
        // 5 writes: JESD power sequence (0x1E, 0x1C, 0x18, 0x08, 0x00)
        for _ in 0..5 {
            mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
        }
        // 2 reads: r0x40, r0x4a
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![r40]);
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]); // r0x4a
    };

    // Helper: queue reset_main_dcm (5-step JESD cascade, no SERDES retry).
    let queue_reset_main_dcm = |mock: &MockTransport, r40: u8| {
        queue_one_cascade(mock, r40);
    };

    // Calibration EEPROM read (ReadVersion) -> 16 bytes
    let cal_data = vec![
        0x46, 0xFF, 0xC6, 0xA6, 0x60, 0xB7, 0xDE, 0x07, 0x67, 0x8B, 0x03, 0x66, 0x11, 0x35, 0x7C,
        0x7F,
    ];
    mock.push_bulk_in_response(endpoint::BULK_IN, cal_data);

    // configure_ad9637: 9 ACKs (2 reset proxy + 7 config proxy)
    for _ in 0..9 {
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
    }

    // r0x15=0x00 (ADC power off): 1 ACK
    mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);

    // set_ad9637_enabled_false: 1 proxy ACK + single dcm_cascade (not retried)
    mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
    queue_one_cascade(mock, 0x00);

    // set_ad9637_enabled_true: 3 proxy ACKs + reset_main_dcm (5-step JESD, r0x40=0xE0) + 3 reads
    for _ in 0..3 {
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
    }
    queue_reset_main_dcm(mock, 0xE0);
    mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]); // r0x19
    mock.push_bulk_in_response(endpoint::BULK_IN, vec![0xE0]); // r0x40
    mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]); // r0x4a

    // write_calibration_and_cascade: 2 direct + 16 cal + set_enabled_false + 1 read + 1 write
    mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]); // r0x00=0x00
    mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]); // r0x00=0x40
    for _ in 0..16 {
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]); // cal coeff writes
    }
    mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]); // SPI transfer proxy ACK
    queue_one_cascade(mock, 0x00); // dcm_cascade (5-step JESD), r0x40=0x00 expected post-cal
    mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]); // r0x40 post-cal
    mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]); // r0x12 write ACK

    // Device ID EEPROM read
    mock.push_bulk_in_response(endpoint::BULK_IN, device_id.to_le_bytes().to_vec());

    device_id
}

#[test]
fn common_device_init_non_lattice_reads_eeprom() {
    let mock = Arc::new(MockTransport::new());
    let mut channel = make_initialized_channel(&mock);

    let expected_id = queue_xilinx_common_device_init(&mock);

    let result = common_device_init(&mut channel, false).unwrap();

    assert_eq!(result.device_id, expected_id);
    assert!(!result.jesd_locked, "non-Lattice: no JESD");
    assert!(!result.ddr_ready, "non-Lattice: no DDR check");
    assert!(result.dcm_locked, "Xilinx: DCM should be locked");

    // Verify the EEPROM device ID read opcode (0x88) was sent.
    // It's the last OUT transfer in the sequence.
    let ops = mock.operations();
    let out_data = extract_bulk_out_data(&ops);
    let last = out_data.last().unwrap();
    assert_eq!(last[0], 0x88, "last command should be EEPROM read (0x88)");
}

// ===========================================================================
// Test 7: Full device lifecycle (firmware -> init -> FPGA -> post-init -> EEPROM)
// ===========================================================================

#[test]
fn full_device_lifecycle_firmware_to_eeprom() {
    // This test exercises the complete happy-path lifecycle:
    // 1. Upload firmware (via control transfers on bootloader transport)
    // 2. Initialize EP1 channel (reseed + ping)
    // 3. Program FPGA (reset + bitstream + verify)
    // 4. Post-init (register writes)
    // 5. CommonDeviceInit (EEPROM read)
    //
    // Steps 1 and 2-5 use separate transports (the device re-enumerates
    // after firmware upload), matching real hardware behavior.

    // --- Step 1: Firmware upload on bootloader transport ---
    let boot_mock = MockTransport::new();
    let image = make_test_firmware();
    upload_fx3_firmware(&boot_mock, &image).unwrap();

    let boot_ops = boot_mock.operations();
    assert_eq!(
        count_control_out(&boot_ops),
        3,
        "firmware: 2 sections + 1 entry"
    );

    // --- Step 2: Initialize EP1 on re-enumerated transport ---
    let device_mock = Arc::new(MockTransport::new());
    let mut channel = make_initialized_channel(&device_mock);

    // --- Step 3: Program FPGA ---
    let bitstream = vec![0xAB; 100];
    device_mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]); // reset
    device_mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]); // config data
    device_mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]); // WriteRegisters ACK #1
    device_mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]); // WriteRegisters ACK #2
    device_mock.push_bulk_in_response(endpoint::BULK_IN, vec![0xAA]); // readback OK
    program_fpga(
        &mut channel,
        &bitstream,
        &LOGIC_PRO_8_PROFILE,
        UsbSpeed::Super,
    )
    .unwrap();

    // --- Step 4: Post-init (non-Lattice) ---
    for _ in 0..5 {
        device_mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
    }
    fpga_post_init(&mut channel, false).unwrap();

    // --- Step 5: CommonDeviceInit (Xilinx path with AD9637 init) ---
    let expected_id = queue_xilinx_common_device_init(&device_mock);
    let result = common_device_init(&mut channel, false).unwrap();

    assert_eq!(result.device_id, expected_id);
    assert_eq!(
        format!("{:x}", result.device_id),
        "764051b51bd4d9f1",
        "device ID hex should match expected calibration filename"
    );
    assert!(result.dcm_locked, "DCM should be locked after AD9637 init");

    // Verify total operation count is reasonable.
    // After clear(): 5 (FPGA) + 5 (post-init) + many (AD9637 init) + 1 (EEPROM)
    let ops = device_mock.operations();
    let total_outs = count_bulk_out(&ops);
    assert!(
        total_outs > 11,
        "total EP1 OUTs should include AD9637 init commands"
    );
}

// ===========================================================================
// Test 8: Capture configuration validation
// ===========================================================================

#[test]
fn capture_config_rejects_invalid_sample_rate() {
    let mock = Arc::new(MockTransport::new());
    let mut channel = make_initialized_channel(&mock);

    let config = CaptureConfig {
        sample_rate: 0,
        ..CaptureConfig::default()
    };

    let result = saleae_driver::capture::start_digital_capture(&mut channel, &config);
    assert!(result.is_err(), "zero sample rate should be rejected");
    let err = result.err().unwrap();
    assert!(err.to_string().contains("sample rate"));
}

#[test]
fn capture_config_rejects_no_channels() {
    let mock = Arc::new(MockTransport::new());
    let mut channel = make_initialized_channel(&mock);

    let config = CaptureConfig {
        digital_channels: vec![],
        ..CaptureConfig::default()
    };

    let result = saleae_driver::capture::start_digital_capture(&mut channel, &config);
    assert!(result.is_err(), "no channels should be rejected");
    let err = result.err().unwrap();
    assert!(err.to_string().contains("channel"));
}

#[test]
fn capture_config_rejects_channel_out_of_range() {
    let mock = Arc::new(MockTransport::new());
    let mut channel = make_initialized_channel(&mock);

    let config = CaptureConfig {
        digital_channels: vec![0, 16], // 16 is out of range
        ..CaptureConfig::default()
    };

    let result = saleae_driver::capture::start_digital_capture(&mut channel, &config);
    assert!(result.is_err(), "channel 16 should be rejected");
    let err = result.err().unwrap();
    assert!(err.to_string().contains("16"));
}
