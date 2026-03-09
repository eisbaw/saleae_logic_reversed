//! FPGA bitstream programming and post-init sequence.
//!
//! After the Cypress FX3 firmware is running, the next step in the device
//! boot sequence is to download an FPGA bitstream and initialize the FPGA
//! subsystems (DDR, JESD, GPIF). This module implements:
//!
//! 1. [`program_fpga`] -- reset FPGA, send bitstream, verify readback.
//! 2. [`fpga_post_init`] -- write init registers, reset JESD PHY, poll
//!    DDR ready.
//!
//! # Hardware testing
//!
//! This code has NOT been validated against hardware yet. The device is
//! currently in a stuck state (red LED blinking) and needs a power cycle.
//! All testing is via mock transport only.
//!
//! # Register path uncertainty
//!
//! The post-init sequence writes to registers at both 8-bit addresses
//! (e.g., 0x0040) and 16-bit addresses (e.g., 0x01C0). The 8-bit path
//! uses opcode 0x80 (WriteRegisters) which is well-understood. The
//! 16-bit path is less certain -- see [`Ep1CommandChannel::write_register_16`]
//! for details.

use std::time::{Duration, Instant};

use saleae_proto::fpga_regs;
use tracing::{debug, info};

use crate::ep1::Ep1CommandChannel;
use crate::error::{DriverError, DriverResult};

// ---------------------------------------------------------------------------
// Configuration constants
// ---------------------------------------------------------------------------

/// Default timeout for DDR ready polling.
const DDR_READY_TIMEOUT: Duration = Duration::from_secs(5);

/// Polling interval for DDR ready check.
const DDR_READY_POLL_INTERVAL: Duration = Duration::from_millis(1);

/// Default values for post-init register writes.
///
/// These are placeholder values based on what the original software writes.
/// The exact values may depend on device type and configuration. They will
/// be refined once USB captures are available.
mod init_values {
    /// COMMIT_CONTROL initial value.
    ///
    /// Observed: the original SW writes to this register during init and
    /// after digital config changes. Exact value TBD from USB capture.
    pub const COMMIT_CONTROL: u32 = 0x0000_0000;

    /// CAPTURE_CONTROL initial value (disable all capture).
    pub const CAPTURE_CONTROL: u32 = 0x0000_0000;

    /// INIT_CONFIG value.
    ///
    /// Written once after FPGA programming. Likely enables core FPGA
    /// subsystems. Exact bit layout TBD.
    pub const INIT_CONFIG: u32 = 0x0000_0000;

    /// GPIF_FLUSH_TIMEOUT in 100 MHz clock ticks.
    ///
    /// This determines how long the GPIF waits before flushing a partial
    /// packet. Common value: ~1 ms = 100,000 ticks = 0x186A0.
    pub const GPIF_FLUSH_TIMEOUT: u32 = 0x0001_86A0;

    /// GPIF_ENABLE value (enable the data path).
    pub const GPIF_ENABLE: u32 = 0x0000_0001;
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Program the FPGA with a bitstream via EP1.
///
/// Follows the sequence documented in `doc/protocol/fpga-bitstream.md`:
///
/// 1. Send ResetFpga command (opcode 0x7E).
/// 2. Send bitstream via WriteConfigurationData (opcode 0x7F) in 509-byte
///    chunks (USB 3.0).
/// 3. Verify: read register 0x00, expect 0xAA.
///
/// # Errors
///
/// Returns [`DriverError::FpgaProgramming`] if the readback verification
/// fails after bitstream upload.
pub fn program_fpga(ep1: &mut Ep1CommandChannel, bitstream: &[u8]) -> DriverResult<()> {
    info!(
        bitstream_len = bitstream.len(),
        "Programming FPGA bitstream"
    );

    // Step 1: Reset the FPGA.
    debug!("Resetting FPGA");
    ep1.reset_fpga()?;

    // Step 2: Send the bitstream data.
    debug!("Sending bitstream data ({} bytes)", bitstream.len());
    ep1.write_config_data(bitstream)?;

    // Step 3: Verify readback.
    //
    // The original software reads a register and expects 0xAA. Based on
    // the error string "FPGA readback got {:#x} instead of 0xAA", this
    // is a simple status check via the 8-bit register path.
    //
    // Register address 0x00 is used here as a best guess -- the exact
    // address needs USB capture confirmation. It may be a dedicated
    // "programming status" register that returns 0xAA when the FPGA
    // configuration is complete and valid.
    debug!("Verifying FPGA programming (expecting 0xAA readback)");
    let status = ep1.read_registers(&[0x00])?;

    if status.is_empty() {
        return Err(DriverError::FpgaProgramming {
            context: "readback returned no data".into(),
        });
    }

    if status[0] != fpga_regs::FPGA_READBACK_OK {
        return Err(DriverError::FpgaProgramming {
            context: format!(
                "readback got {:#04x} instead of {:#04x}",
                status[0],
                fpga_regs::FPGA_READBACK_OK
            ),
        });
    }

    info!("FPGA programming verified (readback = 0xAA)");
    Ok(())
}

/// Post-programming initialization sequence.
///
/// After the FPGA bitstream is loaded, the host must initialize the FPGA
/// subsystems before the device is ready for capture. The sequence:
///
/// 1. Write COMMIT_CONTROL (0x01C0)
/// 2. Write CAPTURE_CONTROL (0x0040)
/// 3. Write INIT_CONFIG (0x00E0)
/// 4. Write GPIF_FLUSH_TIMEOUT (0x0024)
/// 5. Write GPIF_ENABLE (0x0020)
/// 6. Reset JESD PHY transceivers (write reg 0x0008)
/// 7. Poll DDR ready: (reg_0x0008 & 0x18) == 0x18, with timeout
///
/// Steps 6-7 are Lattice ECP5 specific. For Xilinx devices, JESD reset
/// may not be needed (or may use a different mechanism).
///
/// # Arguments
///
/// - `ep1`: Initialized EP1 command channel.
/// - `is_lattice`: Whether the FPGA is a Lattice ECP5 part. If true,
///   performs JESD PHY reset and DDR polling.
///
/// # Errors
///
/// Returns [`DriverError::FpgaProgramming`] if DDR fails to become ready
/// within the timeout.
pub fn fpga_post_init(ep1: &mut Ep1CommandChannel, is_lattice: bool) -> DriverResult<()> {
    info!("Running FPGA post-programming init sequence");

    // Step 1: COMMIT_CONTROL (0x01C0) -- address > 0xFF, needs 16-bit path.
    debug!("Writing COMMIT_CONTROL");
    ep1.write_fpga_register(fpga_regs::REG_COMMIT_CONTROL, init_values::COMMIT_CONTROL)?;

    // Step 2: CAPTURE_CONTROL (0x0040) -- fits in 8-bit path.
    debug!("Writing CAPTURE_CONTROL");
    ep1.write_fpga_register(
        fpga_regs::REG_CAPTURE_CONTROL,
        init_values::CAPTURE_CONTROL,
    )?;

    // Step 3: INIT_CONFIG (0x00E0) -- fits in 8-bit path.
    debug!("Writing INIT_CONFIG");
    ep1.write_fpga_register(fpga_regs::REG_INIT_CONFIG, init_values::INIT_CONFIG)?;

    // Step 4: GPIF_FLUSH_TIMEOUT (0x0024) -- fits in 8-bit path.
    debug!("Writing GPIF_FLUSH_TIMEOUT");
    ep1.write_fpga_register(
        fpga_regs::REG_GPIF_FLUSH_TIMEOUT,
        init_values::GPIF_FLUSH_TIMEOUT,
    )?;

    // Step 5: GPIF_ENABLE (0x0020) -- fits in 8-bit path.
    debug!("Writing GPIF_ENABLE");
    ep1.write_fpga_register(fpga_regs::REG_GPIF_ENABLE, init_values::GPIF_ENABLE)?;

    // Steps 6-7: Lattice-specific JESD PHY reset and DDR polling.
    if is_lattice {
        reset_jesd_phy(ep1)?;
        wait_for_ddr_ready(ep1, DDR_READY_TIMEOUT)?;
    }

    info!("FPGA post-init complete");
    Ok(())
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Reset JESD PHY transceivers.
///
/// Writes to register 0x0008 (STATUS_DDR_JESD) to toggle the JESD PHY
/// reset bits. The exact sequence is: write a reset pattern, then clear it.
///
/// This is Lattice ECP5 specific. The original software logs:
/// "Lattice PLL and SERDES reset complete".
fn reset_jesd_phy(ep1: &mut Ep1CommandChannel) -> DriverResult<()> {
    debug!("Resetting JESD PHY transceivers");

    // Assert reset (write a value with reset bits set).
    // The exact bit pattern is TBD from USB capture. Using bit 0 as a
    // common pattern for reset assertion.
    ep1.write_fpga_register(fpga_regs::REG_STATUS_DDR_JESD, 0x0000_0001)?;

    // Deassert reset.
    ep1.write_fpga_register(fpga_regs::REG_STATUS_DDR_JESD, 0x0000_0000)?;

    debug!("JESD PHY reset complete");
    Ok(())
}

/// Poll register 0x0008 until DDR is ready.
///
/// DDR ready = (reg_value & 0x18) == 0x18 (both LOCKED and CALIBRATED bits).
///
/// Polls with 1 ms intervals. The original software logs:
/// "Locked and calibrated DDR after {} ms".
fn wait_for_ddr_ready(ep1: &mut Ep1CommandChannel, timeout: Duration) -> DriverResult<()> {
    debug!("Waiting for DDR ready (timeout: {:?})", timeout);

    let start = Instant::now();

    loop {
        let status = ep1.read_fpga_register(fpga_regs::REG_STATUS_DDR_JESD)?;
        let ddr_bits = status & fpga_regs::DDR_READY_MASK;

        if ddr_bits == fpga_regs::DDR_READY_MASK {
            let elapsed = start.elapsed();
            info!(
                elapsed_ms = elapsed.as_millis(),
                "Locked and calibrated DDR after {} ms",
                elapsed.as_millis()
            );
            return Ok(());
        }

        if start.elapsed() >= timeout {
            return Err(DriverError::FpgaProgramming {
                context: format!(
                    "DDR not ready after {:?}: status register = {status:#010x}, \
                     DDR bits = {ddr_bits:#010x} (expected {:#010x})",
                    timeout,
                    fpga_regs::DDR_READY_MASK,
                ),
            });
        }

        std::thread::sleep(DDR_READY_POLL_INTERVAL);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use saleae_proto::obfuscation::EncryptionMode;
    use saleae_usb::mock::{MockTransport, UsbOperation};
    use saleae_proto::device::endpoint;

    /// Create a mock channel with XorAll encryption and seed=0 (identity XOR).
    fn mock_channel(mock: &std::sync::Arc<MockTransport>) -> Ep1CommandChannel {
        let transport = Box::new(MockTransportWrapper(mock.clone()));
        let mut channel = Ep1CommandChannel::new(transport, EncryptionMode::XorAll);
        // Seed 0 makes XOR identity -- simplifies assertions.
        channel.encryption.reseed(0);
        channel.initialized = true;
        channel
    }

    /// Wrapper to clone a MockTransport via Arc for test purposes.
    struct MockTransportWrapper(std::sync::Arc<MockTransport>);

    impl saleae_usb::transport::UsbTransport for MockTransportWrapper {
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

    // -----------------------------------------------------------------------
    // program_fpga tests
    // -----------------------------------------------------------------------

    #[test]
    fn program_fpga_sends_reset_then_data_then_verify() {
        let mock = std::sync::Arc::new(MockTransport::new());

        // Queue responses:
        // 1. ResetFpga ACK
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
        // 2. WriteConfigurationData ACK (small bitstream fits in one chunk)
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
        // 3. ReadRegisters response: 0xAA (success)
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0xAA]);

        let mut channel = mock_channel(&mock);
        let bitstream = vec![0xFF; 100]; // 100-byte test bitstream

        program_fpga(&mut channel, &bitstream).unwrap();

        // Verify sequence of operations.
        let ops = mock.operations();
        let out_ops: Vec<_> = ops
            .iter()
            .filter(|op| matches!(op, UsbOperation::BulkOut { .. }))
            .collect();

        // Should be 3 bulk OUTs: reset, config_data, read_registers
        assert_eq!(
            out_ops.len(),
            3,
            "expected 3 OUT transfers (reset + data + read_regs)"
        );

        // First OUT: ResetFpga (opcode 0x7E)
        match out_ops[0] {
            UsbOperation::BulkOut { data, .. } => {
                // data[1] = opcode with seed=0
                assert_eq!(data[1], 0x7E, "first command should be ResetFpga (0x7E)");
            }
            _ => unreachable!(),
        }

        // Second OUT: WriteConfigurationData (opcode 0x7F)
        match out_ops[1] {
            UsbOperation::BulkOut { data, .. } => {
                assert_eq!(
                    data[1], 0x7F,
                    "second command should be WriteConfigurationData (0x7F)"
                );
            }
            _ => unreachable!(),
        }

        // Third OUT: ReadRegisters (opcode 0x81)
        match out_ops[2] {
            UsbOperation::BulkOut { data, .. } => {
                assert_eq!(
                    data[1], 0x81,
                    "third command should be ReadRegisters (0x81)"
                );
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn program_fpga_fails_on_wrong_readback() {
        let mock = std::sync::Arc::new(MockTransport::new());

        // Queue responses:
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]); // ResetFpga ACK
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]); // Config data ACK
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x55]); // Wrong readback!

        let mut channel = mock_channel(&mock);
        let err = program_fpga(&mut channel, &[0xFF; 10]).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("0x55") && msg.contains("0xaa"),
            "error should mention both actual and expected values: {msg}"
        );
    }

    #[test]
    fn program_fpga_fails_on_empty_readback() {
        let mock = std::sync::Arc::new(MockTransport::new());

        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]); // ResetFpga ACK
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]); // Config data ACK
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![]); // Empty!

        let mut channel = mock_channel(&mock);
        let err = program_fpga(&mut channel, &[0xFF; 10]).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("no data"),
            "error should mention missing data: {msg}"
        );
    }

    #[test]
    fn program_fpga_handles_large_bitstream() {
        let mock = std::sync::Arc::new(MockTransport::new());

        // A realistic Lattice bitstream is ~100KB-200KB.
        let bitstream_size = 150_000;
        let bitstream = vec![0xAB; bitstream_size];

        // Number of chunks: ceil(150000 / 509) = 295
        let num_chunks = (bitstream_size + 508) / 509;

        // Queue responses:
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]); // ResetFpga
        for _ in 0..num_chunks {
            mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]); // Config data ACKs
        }
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0xAA]); // Readback OK

        let mut channel = mock_channel(&mock);
        program_fpga(&mut channel, &bitstream).unwrap();

        // Count OUT operations
        let ops = mock.operations();
        let out_count = ops
            .iter()
            .filter(|op| matches!(op, UsbOperation::BulkOut { .. }))
            .count();
        // 1 (reset) + num_chunks (data) + 1 (read_registers) = num_chunks + 2
        assert_eq!(
            out_count,
            num_chunks + 2,
            "expected {} OUT transfers for {} byte bitstream",
            num_chunks + 2,
            bitstream_size
        );
    }

    // -----------------------------------------------------------------------
    // fpga_post_init tests
    // -----------------------------------------------------------------------

    #[test]
    fn post_init_non_lattice_writes_5_registers() {
        let mock = std::sync::Arc::new(MockTransport::new());

        // Queue 5 ACK responses (one per register write).
        for _ in 0..5 {
            mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
        }

        let mut channel = mock_channel(&mock);
        fpga_post_init(&mut channel, false).unwrap();

        // For non-Lattice, no JESD reset or DDR polling.
        let ops = mock.operations();
        let out_count = ops
            .iter()
            .filter(|op| matches!(op, UsbOperation::BulkOut { .. }))
            .count();
        assert_eq!(out_count, 5, "expected 5 register writes for non-Lattice");
    }

    #[test]
    fn post_init_lattice_includes_jesd_and_ddr() {
        let mock = std::sync::Arc::new(MockTransport::new());

        // Queue responses:
        // 5 register writes (COMMIT, CAPTURE, INIT, GPIF_TIMEOUT, GPIF_ENABLE)
        for _ in 0..5 {
            mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
        }
        // 2 JESD PHY reset writes (assert + deassert)
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
        // DDR ready poll: return ready on first attempt.
        // read_fpga_register(0x0008) -> uses 8-bit path -> ReadRegisters opcode 0x81
        // Return 0x18 (both DDR bits set).
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x18]);

        let mut channel = mock_channel(&mock);
        fpga_post_init(&mut channel, true).unwrap();

        // 5 init writes + 2 JESD writes + 1 DDR read = 8 command-response pairs
        let ops = mock.operations();
        let out_count = ops
            .iter()
            .filter(|op| matches!(op, UsbOperation::BulkOut { .. }))
            .count();
        assert_eq!(
            out_count, 8,
            "expected 8 OUT transfers for Lattice post-init"
        );
    }

    #[test]
    fn post_init_ddr_polling_retries() {
        let mock = std::sync::Arc::new(MockTransport::new());

        // Queue initial register write ACKs.
        for _ in 0..5 {
            mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
        }
        // JESD reset ACKs.
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);

        // DDR poll: not ready first 2 times, then ready.
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]); // neither bit
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x10]); // only locked
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x18]); // both bits

        let mut channel = mock_channel(&mock);
        fpga_post_init(&mut channel, true).unwrap();

        // 5 init + 2 JESD + 3 DDR reads = 10
        let ops = mock.operations();
        let out_count = ops
            .iter()
            .filter(|op| matches!(op, UsbOperation::BulkOut { .. }))
            .count();
        assert_eq!(out_count, 10, "expected 10 OUT transfers with DDR retries");
    }

    #[test]
    fn post_init_register_write_order() {
        let mock = std::sync::Arc::new(MockTransport::new());

        // Queue ACKs for non-Lattice path (5 writes, no JESD/DDR).
        for _ in 0..5 {
            mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
        }

        let mut channel = mock_channel(&mock);
        fpga_post_init(&mut channel, false).unwrap();

        let ops = mock.operations();
        let out_ops: Vec<_> = ops
            .iter()
            .filter_map(|op| match op {
                UsbOperation::BulkOut { data, .. } => Some(data.clone()),
                _ => None,
            })
            .collect();

        assert_eq!(out_ops.len(), 5);

        // Op 0: COMMIT_CONTROL via 16-bit path (addr_hi=0x01 as opcode)
        assert_eq!(
            out_ops[0][1], 0x01,
            "first write should use addr_hi=0x01 (COMMIT_CONTROL 0x01C0)"
        );
        assert_eq!(
            out_ops[0][2], 0xC0,
            "first write should have addr_lo=0xC0"
        );

        // Op 1: CAPTURE_CONTROL via 8-bit path (opcode 0x80)
        assert_eq!(
            out_ops[1][1], 0x80,
            "second write should be opcode 0x80 (WriteRegisters)"
        );
        // addr byte inside payload: data[3] should be 0x40 (CAPTURE_CONTROL)
        assert_eq!(out_ops[1][3], 0x40);

        // Op 2: INIT_CONFIG via 8-bit path
        assert_eq!(out_ops[2][1], 0x80);
        assert_eq!(out_ops[2][3], 0xE0); // INIT_CONFIG = 0xE0

        // Op 3: GPIF_FLUSH_TIMEOUT via 8-bit path
        assert_eq!(out_ops[3][1], 0x80);
        assert_eq!(out_ops[3][3], 0x24); // GPIF_FLUSH_TIMEOUT = 0x24

        // Op 4: GPIF_ENABLE via 8-bit path
        assert_eq!(out_ops[4][1], 0x80);
        assert_eq!(out_ops[4][3], 0x20); // GPIF_ENABLE = 0x20
    }

    // -----------------------------------------------------------------------
    // Edge cases
    // -----------------------------------------------------------------------

    #[test]
    fn program_fpga_with_empty_bitstream() {
        let mock = std::sync::Arc::new(MockTransport::new());

        // ResetFpga ACK
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
        // No config data chunks needed for empty bitstream.
        // ReadRegisters response
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0xAA]);

        let mut channel = mock_channel(&mock);
        // An empty bitstream should still succeed (reset + verify, no data).
        program_fpga(&mut channel, &[]).unwrap();
    }
}
