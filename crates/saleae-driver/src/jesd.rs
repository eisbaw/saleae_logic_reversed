//! JESD204B link management and ADC indirect register access.
//!
//! This module provides the pcap-validated register sequences for:
//!
//! 1. **JESD power sequence** -- staged subsystem release on register 0x00
//!    (`0x1e -> 0x1c -> 0x18 -> 0x08 -> 0x00`). Used during init (Phase 3c,
//!    3f), capture start (Phase 7), and teardown (Phase 8).
//!
//! 2. **ADC indirect register access** -- write to internal ADC registers
//!    via the FPGA's 4-register proxy protocol (registers 0x03, 0x16, 0x04,
//!    0x05).
//!
//! 3. **Calibration coefficient writes** -- sequential writes to register
//!    0x17 to load device-specific calibration data into the FPGA.
//!
//! All register values and ordering are from `re/notes/init-sequence-pcap.md`
//! (928 EP1 frames, 0 unknown opcodes, Logic Pro 8 capture).
//!
//! # Register paths
//!
//! These functions use the 8-bit FpgaDeviceFeatures register path (EP1
//! opcodes 0x80/0x81), not the 16-bit MsoFpga path. This is the correct
//! path for Logic 4/8 and Logic Pro 8/16 devices.

use saleae_proto::fpga_regs;
use tracing::debug;

use crate::ep1::Ep1CommandChannel;
use crate::error::DriverResult;

/// Execute the JESD204B link power sequence.
///
/// Writes the 5-step staged power sequence to register 0x00:
/// ```text
/// 0x1e -> 0x1c -> 0x18 -> 0x08 -> 0x00
/// ```
///
/// Each step releases one more FPGA subsystem from reset. The sequence
/// is identical for power-up and power-down (the FPGA interprets the
/// final state, not the direction of change).
///
/// After the sequence completes, the caller should poll register 0x40
/// for `0xE0` (JESD link established). The pcap shows the first attempt
/// after FPGA load may fail (returns 0x00), but succeeds after a ~50ms
/// delay and second power cycle.
///
/// # Evidence
///
/// From pcap Phases 3c, 3f, 7, 8 -- the same 5-value sequence appears
/// every time the JESD link is cycled.
pub fn jesd_power_sequence(ep1: &mut Ep1CommandChannel) -> DriverResult<()> {
    debug!(
        "JESD power sequence: {:#04x} -> {:#04x} -> {:#04x} -> {:#04x} -> {:#04x}",
        fpga_regs::JESD_POWER_SEQUENCE[0],
        fpga_regs::JESD_POWER_SEQUENCE[1],
        fpga_regs::JESD_POWER_SEQUENCE[2],
        fpga_regs::JESD_POWER_SEQUENCE[3],
        fpga_regs::JESD_POWER_SEQUENCE[4],
    );

    for &value in &fpga_regs::JESD_POWER_SEQUENCE {
        ep1.write_register_8(fpga_regs::FEAT_CAPTURE_INIT, value)?;
    }

    Ok(())
}

/// Write a value to an ADC register via the FPGA's indirect access proxy.
///
/// The FPGA provides indirect ADC register access through a 4-register
/// batch write protocol:
///
/// ```text
/// r0x03 = bank (bank/register address select)
/// r0x16 = 0x00 (address high byte, always zero in pcap)
/// r0x04 = value (register data)
/// r0x05 = 0x00 (data high byte, always zero in pcap)
/// ```
///
/// These 4 registers are written as a single batch to ensure atomicity.
///
/// # Arguments
///
/// - `ep1`: EP1 command channel.
/// - `bank`: ADC bank/register address (written to register 0x03).
/// - `value`: Data value to write (written to register 0x04).
///
/// # Evidence
///
/// From pcap Phase 3b: multiple 4-register writes using the pattern
/// `[r0x03, r0x16, r0x04, r0x05]`. The trigger register (0x16) is
/// always 0x00, and the data high byte (0x05) is always 0x00.
pub fn write_adc_register(
    ep1: &mut Ep1CommandChannel,
    bank: u8,
    value: u8,
) -> DriverResult<()> {
    debug!(
        "ADC indirect write: bank={:#04x} value={:#04x}",
        bank, value
    );

    ep1.write_registers(&[
        (fpga_regs::ADC_BANK_SELECT, bank),
        (fpga_regs::ADC_REG_TRIGGER, 0x00),
        (fpga_regs::ADC_REG_ADDR, value),
        (fpga_regs::ADC_REG_DATA, 0x00),
    ])
}

/// Write calibration coefficients to the FPGA.
///
/// Sequentially writes each coefficient value to register 0x17. The pcap
/// shows 16 individual writes (one per coefficient), not a batch write.
///
/// # Arguments
///
/// - `ep1`: EP1 command channel.
/// - `coefficients`: Calibration coefficient values (up to 16).
///
/// # Panics
///
/// Panics if `coefficients.len() > 16`.
///
/// # Evidence
///
/// From pcap Phase 3e: 16 individual writes to r0x17 with values:
/// `[0x46, 0xff, 0xc6, 0xa6, 0x60, 0xb7, 0xde, 0x07,
///   0x67, 0x8b, 0x03, 0x66, 0x11, 0x35, 0x7c, 0x7f]`
pub fn write_calibration_coefficients(
    ep1: &mut Ep1CommandChannel,
    coefficients: &[u8],
) -> DriverResult<()> {
    assert!(
        coefficients.len() <= fpga_regs::CAL_COEFF_MAX_COUNT,
        "calibration coefficient count {} exceeds maximum {}",
        coefficients.len(),
        fpga_regs::CAL_COEFF_MAX_COUNT,
    );

    debug!(
        "Writing {} calibration coefficients to register {:#04x}",
        coefficients.len(),
        fpga_regs::CAL_COEFF_REG,
    );

    for (i, &coeff) in coefficients.iter().enumerate() {
        debug!(
            "  coeff[{i}] = {:#04x}",
            coeff
        );
        ep1.write_register_8(fpga_regs::CAL_COEFF_REG, coeff)?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use saleae_proto::device::endpoint;
    use saleae_proto::obfuscation::EncryptionMode;
    use saleae_usb::mock::{MockTransport, UsbOperation};

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

    /// Create a mock EP1 channel in None mode (no encryption, simpler assertions).
    fn mock_channel_none(mock: &std::sync::Arc<MockTransport>) -> Ep1CommandChannel {
        let transport = Box::new(MockTransportWrapper(mock.clone()));
        Ep1CommandChannel::new(transport, EncryptionMode::None)
    }

    /// Extract outbound bulk transfer payloads from mock operations.
    fn extract_out_payloads(mock: &MockTransport) -> Vec<Vec<u8>> {
        mock.operations()
            .iter()
            .filter_map(|op| match op {
                UsbOperation::BulkOut { data, .. } => Some(data.clone()),
                _ => None,
            })
            .collect()
    }

    // -------------------------------------------------------------------
    // JESD power sequence tests
    // -------------------------------------------------------------------

    #[test]
    fn jesd_power_sequence_writes_exact_5_step_order() {
        let mock = std::sync::Arc::new(MockTransport::new());

        // 5 register writes, each gets an ACK.
        for _ in 0..5 {
            mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
        }

        let mut ep1 = mock_channel_none(&mock);
        jesd_power_sequence(&mut ep1).unwrap();

        let payloads = extract_out_payloads(&mock);
        assert_eq!(payloads.len(), 5, "must send exactly 5 writes");

        // In None mode with write_register_8: each command is
        // [opcode=0x80, count=1, addr, value]. Verify addr=0x00 and
        // values match the power sequence.
        let expected_values = [0x1E, 0x1C, 0x18, 0x08, 0x00];
        for (i, payload) in payloads.iter().enumerate() {
            assert_eq!(
                payload[0], 0x80,
                "step {i}: opcode should be WriteRegisters (0x80)"
            );
            assert_eq!(
                payload[1], 0x01,
                "step {i}: count should be 1"
            );
            assert_eq!(
                payload[2], 0x00,
                "step {i}: register address should be 0x00"
            );
            assert_eq!(
                payload[3], expected_values[i],
                "step {i}: value should be {:#04x}",
                expected_values[i]
            );
        }
    }

    #[test]
    fn jesd_power_sequence_uses_named_constants() {
        // Verify the function uses the fpga_regs constants, not magic numbers.
        // This test validates the constant values match pcap expectations.
        assert_eq!(fpga_regs::JESD_POWER_SEQUENCE[0], 0x1E);
        assert_eq!(fpga_regs::JESD_POWER_SEQUENCE[1], 0x1C);
        assert_eq!(fpga_regs::JESD_POWER_SEQUENCE[2], 0x18);
        assert_eq!(fpga_regs::JESD_POWER_SEQUENCE[3], 0x08);
        assert_eq!(fpga_regs::JESD_POWER_SEQUENCE[4], 0x00);
    }

    // -------------------------------------------------------------------
    // ADC indirect register access tests
    // -------------------------------------------------------------------

    #[test]
    fn write_adc_register_sends_4_registers_in_correct_order() {
        let mock = std::sync::Arc::new(MockTransport::new());

        // write_registers sends a single batch -> 1 ACK
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);

        let mut ep1 = mock_channel_none(&mock);
        write_adc_register(&mut ep1, 0x08, 0x03).unwrap();

        let payloads = extract_out_payloads(&mock);
        assert_eq!(payloads.len(), 1, "should send 1 batch write");

        let payload = &payloads[0];
        // In None mode, write_registers: [0x80, count, addr0, val0, addr1, val1, ...]
        assert_eq!(payload[0], 0x80, "opcode should be WriteRegisters");
        assert_eq!(payload[1], 0x04, "count should be 4 (4 register pairs)");

        // Pair 0: bank select (r0x03 = 0x08)
        assert_eq!(payload[2], fpga_regs::ADC_BANK_SELECT, "pair 0 addr");
        assert_eq!(payload[3], 0x08, "pair 0 value (bank)");

        // Pair 1: trigger (r0x16 = 0x00)
        assert_eq!(payload[4], fpga_regs::ADC_REG_TRIGGER, "pair 1 addr");
        assert_eq!(payload[5], 0x00, "pair 1 value (trigger, always 0)");

        // Pair 2: address/value (r0x04 = 0x03)
        assert_eq!(payload[6], fpga_regs::ADC_REG_ADDR, "pair 2 addr");
        assert_eq!(payload[7], 0x03, "pair 2 value (data)");

        // Pair 3: data high (r0x05 = 0x00)
        assert_eq!(payload[8], fpga_regs::ADC_REG_DATA, "pair 3 addr");
        assert_eq!(payload[9], 0x00, "pair 3 value (data high, always 0)");
    }

    #[test]
    fn write_adc_register_different_bank_values() {
        let mock = std::sync::Arc::new(MockTransport::new());

        // Two separate calls, each gets 1 ACK.
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);

        let mut ep1 = mock_channel_none(&mock);

        // Write bank 0x0D, value 0x00
        write_adc_register(&mut ep1, 0x0D, 0x00).unwrap();
        // Write bank 0x15, value 0x21
        write_adc_register(&mut ep1, 0x15, 0x21).unwrap();

        let payloads = extract_out_payloads(&mock);
        assert_eq!(payloads.len(), 2);

        // First write: bank=0x0D, value=0x00
        assert_eq!(payloads[0][3], 0x0D, "first write bank");
        assert_eq!(payloads[0][7], 0x00, "first write value");

        // Second write: bank=0x15, value=0x21
        assert_eq!(payloads[1][3], 0x15, "second write bank");
        assert_eq!(payloads[1][7], 0x21, "second write value");
    }

    // -------------------------------------------------------------------
    // Calibration coefficient tests
    // -------------------------------------------------------------------

    #[test]
    fn write_calibration_coefficients_sends_sequential_writes_to_reg_0x17() {
        let mock = std::sync::Arc::new(MockTransport::new());

        let coefficients = [0x46, 0xFF, 0xC6, 0xA6];

        // Each coefficient write gets 1 ACK.
        for _ in 0..coefficients.len() {
            mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
        }

        let mut ep1 = mock_channel_none(&mock);
        write_calibration_coefficients(&mut ep1, &coefficients).unwrap();

        let payloads = extract_out_payloads(&mock);
        assert_eq!(
            payloads.len(),
            coefficients.len(),
            "should send one write per coefficient"
        );

        for (i, payload) in payloads.iter().enumerate() {
            assert_eq!(payload[0], 0x80, "coeff {i}: opcode");
            assert_eq!(payload[1], 0x01, "coeff {i}: count");
            assert_eq!(
                payload[2], fpga_regs::CAL_COEFF_REG,
                "coeff {i}: register address should be 0x17"
            );
            assert_eq!(
                payload[3], coefficients[i],
                "coeff {i}: value"
            );
        }
    }

    #[test]
    fn write_calibration_coefficients_full_16_pcap_values() {
        let mock = std::sync::Arc::new(MockTransport::new());

        // The exact 16 values from pcap Phase 3e.
        let coefficients: [u8; 16] = [
            0x46, 0xFF, 0xC6, 0xA6, 0x60, 0xB7, 0xDE, 0x07,
            0x67, 0x8B, 0x03, 0x66, 0x11, 0x35, 0x7C, 0x7F,
        ];

        for _ in 0..16 {
            mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
        }

        let mut ep1 = mock_channel_none(&mock);
        write_calibration_coefficients(&mut ep1, &coefficients).unwrap();

        let payloads = extract_out_payloads(&mock);
        assert_eq!(payloads.len(), 16, "should send exactly 16 writes");

        // Verify all go to register 0x17.
        for (i, payload) in payloads.iter().enumerate() {
            assert_eq!(
                payload[2], 0x17,
                "coeff {i}: all writes must target register 0x17"
            );
        }
    }

    #[test]
    fn write_calibration_coefficients_empty_is_noop() {
        let mock = std::sync::Arc::new(MockTransport::new());

        let mut ep1 = mock_channel_none(&mock);
        write_calibration_coefficients(&mut ep1, &[]).unwrap();

        let payloads = extract_out_payloads(&mock);
        assert_eq!(payloads.len(), 0, "empty coefficients should send nothing");
    }

    #[test]
    #[should_panic(expected = "exceeds maximum")]
    fn write_calibration_coefficients_panics_on_too_many() {
        let mock = std::sync::Arc::new(MockTransport::new());
        let mut ep1 = mock_channel_none(&mock);

        // 17 coefficients -- exceeds the 16 maximum.
        let _ = write_calibration_coefficients(&mut ep1, &[0x00; 17]);
    }
}
