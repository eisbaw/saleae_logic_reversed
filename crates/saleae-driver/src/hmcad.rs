//! HMCAD1100 ADC initialization for non-Pro devices (Logic 4, Logic 8).
//!
//! The Saleae Logic 4 and Logic 8 use a Texas Instruments HMCAD1100 ADC.
//! This module implements the ADC initialization sequence extracted via
//! Ghidra decompilation of `Logic2FpgaDevice::InitHmcad1100` (ELF VA
//! 0x1c20ab0), `Hmcad1100Settings::SetDefault` (ELF VA 0x1c1f900), and
//! `Logic2FpgaDevice::WriteHmcadRegister` (ELF VA 0x1c207c0).
//!
//! # Register access
//!
//! The HMCAD1100 uses 16-bit SPI registers (8-bit address, 16-bit value),
//! accessed through the FPGA's 3-register proxy protocol:
//!
//! ```text
//! r0x03 = HMCAD register address (8 bits)
//! r0x04 = register value low byte
//! r0x05 = register value high byte
//! ```
//!
//! This differs from the AD9637 proxy which uses 4 registers (including
//! r0x16 as a trigger/address extension).
//!
//! # Decompilation evidence
//!
//! - `WriteHmcadRegister` builds a 3-entry batch write to FPGA registers
//!   0x03, 0x04, 0x05 (no trigger register 0x16).
//! - `Hmcad1100Settings::SetDefault` initializes defaults per DeviceType:
//!   - Logic4 (type_id=5): mode=5 (4-channel), channels=6
//!   - Logic8 (type_id=6/7): mode=4 (8-channel), channels=7
//! - `InitHmcad1100` writes registers in this order: r0x80 (reset test),
//!   r0x25+r0x45 (test patterns), r0x11+r0x12 (channel gain), r0x2a+r0x2b
//!   (LVDS config), r0x46 (coarse gain/input), r0x50 (fine gain),
//!   r0x55 (coarse gain select), r0x56 (power/sleep).
//!
//! # HMCAD1100 datasheet correlation
//!
//! All register addresses match the HMCAD1511/1100 datasheet register map:
//! - 0x11/0x12: Channel 1/2 digital gain control
//! - 0x25/0x45: ADC test pattern registers
//! - 0x2a/0x2b: LVDS data/phase configuration
//! - 0x46: Coarse gain / input configuration
//! - 0x50: Fine gain adjustment
//! - 0x55: Gain/input select
//! - 0x56: Sleep/power mode
//!
//! # Hardware testing
//!
//! NOT yet hardware-validated. This implementation is purely from
//! decompilation. Logic 4/8 hardware access is needed for validation.

use saleae_proto::device::DeviceType;
use saleae_proto::fpga_regs;
use tracing::{debug, info};

use crate::ep1::Ep1CommandChannel;
use crate::error::DriverResult;

// ---------------------------------------------------------------------------
// HMCAD1100 register addresses (from HMCAD1100 datasheet + decompilation)
// ---------------------------------------------------------------------------

/// Channel 1 digital gain control.
const HMCAD_CH1_DIGITAL_GAIN: u8 = 0x11;

/// Channel 2 digital gain control.
const HMCAD_CH2_DIGITAL_GAIN: u8 = 0x12;

/// Test pattern register 1.
const HMCAD_TEST_PATTERN_1: u8 = 0x25;

/// Test pattern register 2.
const HMCAD_TEST_PATTERN_2: u8 = 0x45;

/// LVDS data/misc configuration register A.
const HMCAD_LVDS_MISC_A: u8 = 0x2A;

/// LVDS data/misc configuration register B.
const HMCAD_LVDS_MISC_B: u8 = 0x2B;

/// Coarse gain and input configuration.
const HMCAD_COARSE_GAIN_CFG: u8 = 0x46;

/// Fine gain register.
const HMCAD_FINE_GAIN: u8 = 0x50;

/// Coarse gain select register.
const HMCAD_COARSE_GAIN_SEL: u8 = 0x55;

/// Power/sleep mode register.
const HMCAD_POWER_SLEEP: u8 = 0x56;

/// ADC test mode / reset register (from decompilation: r0x80 written with
/// 0x0000 via `(**(vtable+0x10))(..., 0, 0x80, 0)` which maps to
/// `write_register_8(0x80, 0x00)` -- this is the FPGA register 0x80 for
/// HMCAD test/reset control, NOT an HMCAD SPI register).
///
/// This write goes through the 8-bit FPGA register path, not the SPI proxy.
const FPGA_HMCAD_RESET: u8 = 0x80;

// ---------------------------------------------------------------------------
// Test pattern lookup tables (from binary DAT_00572e94 and DAT_00572ea0)
// ---------------------------------------------------------------------------
// These are 16-bit values indexed by Hmcad1100Settings::AdcTestPattern (0..5).

/// Values for HMCAD test pattern register 1 (reg 0x25), indexed by pattern ID.
const TEST_PATTERN_1_VALUES: [u16; 6] = [0x0000, 0x0040, 0x0020, 0x0010, 0x0000, 0x0000];

/// Values for HMCAD test pattern register 2 (reg 0x45), indexed by pattern ID.
const TEST_PATTERN_2_VALUES: [u16; 6] = [0x0000, 0x0000, 0x0000, 0x0000, 0x0001, 0x0002];

// ---------------------------------------------------------------------------
// HMCAD1100 settings (from Hmcad1100Settings::SetDefault decompilation)
// ---------------------------------------------------------------------------

/// HMCAD1100 configuration settings.
///
/// Extracted from `Hmcad1100Settings` struct layout in the decompiled
/// `SetDefault` function. The struct is passed by value on the stack
/// to `InitHmcad1100`.
#[derive(Debug, Clone, Copy)]
pub struct Hmcad1100Settings {
    /// ADC channel mode (HMCAD1100 register 0x25 related).
    ///
    /// Logic4: 5 (4-channel mode), Logic8: 4 (8-channel mode).
    /// From decompiled `SetDefault`: first field written to `*param_1`.
    pub mode: u8,

    /// Channel count configuration.
    ///
    /// Logic4: 6, Logic8: 7.
    /// From decompiled: second field, maps to `param_1[1]`.
    pub channels: u8,

    /// Channel 1 digital gain (written to reg 0x11).
    ///
    /// Default: 1 (all devices). The value is replicated per nibble:
    /// `(gain << 8) | (gain << 4) | gain`.
    pub ch1_gain: u8,

    /// Channel 2 digital gain (written to reg 0x12).
    ///
    /// Default: 1 (all devices). Same replication as ch1_gain.
    pub ch2_gain: u8,

    /// LVDS misc configuration value (written to regs 0x2A and 0x2B).
    ///
    /// Default: 1 (all devices). Replicated: `(val << 12) | (val << 8) | (val << 4) | val`.
    pub lvds_misc: u8,

    /// Test pattern selection (0..5).
    ///
    /// Default: 0 (no test pattern). Indexes into TEST_PATTERN_*_VALUES.
    pub test_pattern: u8,

    /// Coarse gain mode (written to reg 0x46).
    ///
    /// Logic4: 2, Logic8: 0.
    /// Encoding: `(input_select << 2) | (cgain4x ? 8 : 0)`.
    pub coarse_gain_mode: u8,

    /// Fine gain value pair (high/low 16 bits at offset +0x18).
    ///
    /// Default: 0x0000_0000 (both zero). Written to reg 0x50.
    pub fine_gain: u16,

    /// Coarse gain select (written to reg 0x55).
    ///
    /// Default: 0 (all devices). Computed from dB gain via lookup table
    /// in FUN_01d1f9b0.
    pub coarse_gain_select: u16,

    /// Power/sleep mode (written to reg 0x56).
    ///
    /// Default: 0 (normal operation, from `in_stack_00000020`).
    pub sleep_mode: u16,
}

impl Hmcad1100Settings {
    /// Create default settings for a given device type.
    ///
    /// Matches `Hmcad1100Settings::SetDefault(DeviceType)` (ELF VA 0x1c1f900).
    ///
    /// # Arguments
    ///
    /// - `device_type`: Logic4 or Logic8.
    ///
    /// # Panics
    ///
    /// Panics if `device_type` is not Logic4 or Logic8.
    pub fn defaults(device_type: DeviceType) -> Self {
        match device_type {
            DeviceType::Logic4 => Self {
                mode: 5,
                channels: 6,
                ch1_gain: 1,
                ch2_gain: 1,
                lvds_misc: 1,
                test_pattern: 0,
                coarse_gain_mode: 2,
                fine_gain: 0,
                coarse_gain_select: 0,
                sleep_mode: 0,
            },
            DeviceType::Logic8 => Self {
                mode: 4,
                channels: 7,
                ch1_gain: 1,
                ch2_gain: 1,
                lvds_misc: 1,
                test_pattern: 0,
                coarse_gain_mode: 0,
                fine_gain: 0,
                coarse_gain_select: 0,
                sleep_mode: 0,
            },
            other => panic!("HMCAD1100 is only used by Logic4/Logic8, got {:?}", other),
        }
    }
}

// ---------------------------------------------------------------------------
// FPGA proxy: write 16-bit value to HMCAD register
// ---------------------------------------------------------------------------

/// Write a 16-bit value to an HMCAD1100 register via the FPGA proxy.
///
/// The FPGA provides indirect HMCAD register access through a 3-register
/// batch write:
///
/// ```text
/// r0x03 = HMCAD register address
/// r0x04 = value low byte
/// r0x05 = value high byte
/// ```
///
/// This differs from the AD9637 proxy (4 registers including r0x16) because
/// the HMCAD1100 SPI interface doesn't need the address extension register.
///
/// # Decompilation evidence
///
/// From `WriteHmcadRegister` (ELF VA 0x1c207c0):
/// - Builds a vector of 3 `ushort` entries: `{(addr<<8)|0x03, (val_lo<<8)|0x04, (val_hi<<8)|0x05}`
/// - Calls `WriteRegisters` vtable at `*(*(param_1+0x180)+0x18)`
/// - No r0x16 write (unlike WriteAd9637Register which has 4 entries)
pub fn write_hmcad_register(ep1: &mut Ep1CommandChannel, addr: u8, value: u16) -> DriverResult<()> {
    debug!(
        "HMCAD indirect write: addr={:#04x} value={:#06x}",
        addr, value
    );

    let value_lo = (value & 0xFF) as u8;
    let value_hi = ((value >> 8) & 0xFF) as u8;

    ep1.write_registers(&[
        (fpga_regs::ADC_BANK_SELECT, addr), // r0x03 = HMCAD register address
        (fpga_regs::ADC_REG_ADDR, value_lo), // r0x04 = value low byte
        (fpga_regs::ADC_REG_DATA, value_hi), // r0x05 = value high byte
    ])
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Initialize the HMCAD1100 ADC with the given settings.
///
/// This is the Rust equivalent of `Logic2FpgaDevice::InitHmcad1100`
/// (ELF VA 0x1c20ab0).
///
/// # Register write sequence
///
/// 1. FPGA r0x80 = 0x00 (reset HMCAD test mode via FPGA register, NOT SPI)
/// 2. r0x25 = test pattern 1 value (from lookup table, usually 0x0000)
/// 3. r0x45 = test pattern 2 value (from lookup table, usually 0x0000)
/// 4. r0x11 = ch1 digital gain (replicated per nibble)
/// 5. r0x12 = ch2 digital gain (replicated per nibble)
/// 6. r0x2a = LVDS misc (replicated per nibble x4)
/// 7. r0x2b = LVDS misc (replicated per nibble x4)
/// 8. r0x46 = coarse gain / input config
/// 9. r0x50 = fine gain
/// 10. r0x55 = coarse gain select
/// 11. r0x56 = sleep/power mode
///
/// # Assertions (from decompilation)
///
/// - `test_pattern < 6` (indices into 6-entry lookup tables)
/// - `lvds_misc < 0x0D` (fits in 4-bit nibble replicated field)
pub fn init_hmcad1100(
    ep1: &mut Ep1CommandChannel,
    settings: &Hmcad1100Settings,
) -> DriverResult<()> {
    info!(
        "Initializing HMCAD1100 ADC (mode={}, channels={}, test_pattern={})",
        settings.mode, settings.channels, settings.test_pattern
    );

    // Validate constraints from decompilation
    assert!(
        settings.test_pattern < 6,
        "HMCAD test pattern must be 0..5, got {}",
        settings.test_pattern
    );
    assert!(
        settings.lvds_misc < 0x0D,
        "HMCAD LVDS misc must be < 13, got {}",
        settings.lvds_misc
    );

    // Step 1: Reset HMCAD test mode via FPGA register (not SPI proxy).
    // From decompilation: `(*(vtable+0x10))(..., 0, 0x80, 0)` which maps to
    // write_register_8(0x80, 0x00). This is a direct FPGA register write,
    // not an HMCAD SPI write.
    debug!("FPGA r0x80=0x00 (reset HMCAD test mode)");
    ep1.write_register_8(FPGA_HMCAD_RESET, 0x00)?;

    // Step 2-3: Test pattern registers (from lookup tables)
    let tp1 = TEST_PATTERN_1_VALUES[settings.test_pattern as usize];
    let tp2 = TEST_PATTERN_2_VALUES[settings.test_pattern as usize];
    debug!("Test patterns: r0x25={:#06x} r0x45={:#06x}", tp1, tp2);
    write_hmcad_register(ep1, HMCAD_TEST_PATTERN_1, tp1)?;
    write_hmcad_register(ep1, HMCAD_TEST_PATTERN_2, tp2)?;

    // Step 4-5: Channel digital gain (replicated per nibble).
    // From decompilation: `(gain << 8) | (gain << 4) | gain`
    let ch1_gain_val = replicate_nibble_3(settings.ch1_gain);
    let ch2_gain_val = replicate_nibble_3(settings.ch2_gain);
    debug!(
        "Channel gains: r0x11={:#06x} r0x12={:#06x}",
        ch1_gain_val, ch2_gain_val
    );
    write_hmcad_register(ep1, HMCAD_CH1_DIGITAL_GAIN, ch1_gain_val)?;
    write_hmcad_register(ep1, HMCAD_CH2_DIGITAL_GAIN, ch2_gain_val)?;

    // Step 6-7: LVDS misc configuration (replicated per nibble x4).
    // From decompilation: `(val << 12) | (val << 8) | (val << 4) | val`
    let lvds_val = replicate_nibble_4(settings.lvds_misc);
    debug!("LVDS misc: r0x2a={:#06x} r0x2b={:#06x}", lvds_val, lvds_val);
    write_hmcad_register(ep1, HMCAD_LVDS_MISC_A, lvds_val)?;
    write_hmcad_register(ep1, HMCAD_LVDS_MISC_B, lvds_val)?;

    // Step 8: Coarse gain / input configuration.
    // From decompilation: `(input_select << 2) | (cgain4x_flag)`.
    // The exact encoding depends on settings fields; we pass the
    // pre-computed value from the settings struct.
    debug!(
        "Coarse gain config: r0x46={:#06x}",
        settings.coarse_gain_mode
    );
    write_hmcad_register(ep1, HMCAD_COARSE_GAIN_CFG, settings.coarse_gain_mode as u16)?;

    // Step 9: Fine gain.
    debug!("Fine gain: r0x50={:#06x}", settings.fine_gain);
    write_hmcad_register(ep1, HMCAD_FINE_GAIN, settings.fine_gain)?;

    // Step 10: Coarse gain select (from dB lookup in FUN_01d1f9b0).
    debug!(
        "Coarse gain select: r0x55={:#06x}",
        settings.coarse_gain_select
    );
    write_hmcad_register(ep1, HMCAD_COARSE_GAIN_SEL, settings.coarse_gain_select)?;

    // Step 11: Sleep/power mode.
    debug!("Sleep mode: r0x56={:#06x}", settings.sleep_mode);
    write_hmcad_register(ep1, HMCAD_POWER_SLEEP, settings.sleep_mode)?;

    info!("HMCAD1100 initialization complete");
    Ok(())
}

/// Set HMCAD1100 test pattern.
///
/// Equivalent to `Logic2FpgaDevice::SetTestPattenHmcad1100` (ELF VA 0x1c20cf0).
///
/// # Arguments
///
/// - `pattern`: Test pattern ID (0..5). 0 = disabled.
///
/// # Panics
///
/// Panics if `pattern >= 6`.
pub fn set_test_pattern(ep1: &mut Ep1CommandChannel, pattern: u8) -> DriverResult<()> {
    assert!(
        pattern < 6,
        "HMCAD test pattern must be 0..5, got {}",
        pattern
    );

    let tp1 = TEST_PATTERN_1_VALUES[pattern as usize];
    let tp2 = TEST_PATTERN_2_VALUES[pattern as usize];

    debug!(
        "Setting HMCAD test pattern {}: r0x25={:#06x} r0x45={:#06x}",
        pattern, tp1, tp2
    );
    write_hmcad_register(ep1, HMCAD_TEST_PATTERN_1, tp1)?;
    write_hmcad_register(ep1, HMCAD_TEST_PATTERN_2, tp2)?;

    Ok(())
}

/// Complete HMCAD1100 init sequence for non-Pro devices (Logic 4/8).
///
/// This is the top-level init function corresponding to the HMCAD path
/// in `CommonDeviceInit` (ELF VA 0x1c21fb0). For AdcType==0 (HMCAD),
/// CommonDeviceInit calls:
///
/// 1. `Hmcad1100Settings::SetDefault(device_type)`
/// 2. `InitHmcad1100(settings)`
/// 3. `ResetMainDcm()` (JESD/DCM power sequence)
///
/// # Arguments
///
/// - `ep1`: EP1 command channel (must be initialized).
/// - `device_type`: Logic4 or Logic8.
pub fn init_hmcad1100_full(
    ep1: &mut Ep1CommandChannel,
    device_type: DeviceType,
) -> DriverResult<()> {
    info!("Starting HMCAD1100 init for {:?}", device_type);

    // Step 1: Get default settings for this device type
    let settings = Hmcad1100Settings::defaults(device_type);
    debug!("HMCAD1100 defaults: {:?}", settings);

    // Step 2: Write ADC configuration
    init_hmcad1100(ep1, &settings)?;

    // Step 3: Reset DCM / JESD power sequence
    // For HMCAD (AdcType==0), ResetMainDcm writes:
    //   r0x00 = 0x1E -> 0x1C -> 0x18 -> 0x08 -> 0x00
    // This is the same JESD power sequence used by AD9637.
    info!("Running JESD power sequence for HMCAD");
    crate::jesd::jesd_power_sequence(ep1)?;

    info!("HMCAD1100 init complete for {:?}", device_type);
    Ok(())
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Replicate a value into 3 nibble positions: `(v << 8) | (v << 4) | v`.
///
/// From decompiled InitHmcad1100:
/// ```text
/// (gain << 8 | gain << 4 | gain) & 0xffff
/// ```
fn replicate_nibble_3(v: u8) -> u16 {
    let v = v as u16;
    (v << 8) | (v << 4) | v
}

/// Replicate a value into 4 nibble positions: `(v << 12) | (v << 8) | (v << 4) | v`.
///
/// From decompiled InitHmcad1100:
/// ```text
/// (val << 0xc | val << 8 | val << 4 | val) & 0xffff
/// ```
fn replicate_nibble_4(v: u8) -> u16 {
    let v = v as u16;
    (v << 12) | (v << 8) | (v << 4) | v
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

    fn mock_channel_none(mock: &std::sync::Arc<MockTransport>) -> Ep1CommandChannel {
        let transport = Box::new(MockTransportWrapper(mock.clone()));
        Ep1CommandChannel::new(transport, EncryptionMode::None)
    }

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
    // Helper function tests
    // -------------------------------------------------------------------

    #[test]
    fn replicate_nibble_3_values() {
        // gain=1: (1<<8)|(1<<4)|1 = 0x111
        assert_eq!(replicate_nibble_3(0), 0x000);
        assert_eq!(replicate_nibble_3(1), 0x111);
        assert_eq!(replicate_nibble_3(2), 0x222);
        assert_eq!(replicate_nibble_3(0xF), 0xFFF);
    }

    #[test]
    fn replicate_nibble_4_values() {
        // val=1: (1<<12)|(1<<8)|(1<<4)|1 = 0x1111
        assert_eq!(replicate_nibble_4(0), 0x0000);
        assert_eq!(replicate_nibble_4(1), 0x1111);
        assert_eq!(replicate_nibble_4(2), 0x2222);
        assert_eq!(replicate_nibble_4(0xF), 0xFFFF);
    }

    // -------------------------------------------------------------------
    // Settings tests
    // -------------------------------------------------------------------

    #[test]
    fn defaults_logic4() {
        let s = Hmcad1100Settings::defaults(DeviceType::Logic4);
        assert_eq!(s.mode, 5);
        assert_eq!(s.channels, 6);
        assert_eq!(s.ch1_gain, 1);
        assert_eq!(s.ch2_gain, 1);
        assert_eq!(s.lvds_misc, 1);
        assert_eq!(s.test_pattern, 0);
        assert_eq!(s.coarse_gain_mode, 2);
        assert_eq!(s.fine_gain, 0);
        assert_eq!(s.coarse_gain_select, 0);
        assert_eq!(s.sleep_mode, 0);
    }

    #[test]
    fn defaults_logic8() {
        let s = Hmcad1100Settings::defaults(DeviceType::Logic8);
        assert_eq!(s.mode, 4);
        assert_eq!(s.channels, 7);
        assert_eq!(s.ch1_gain, 1);
        assert_eq!(s.ch2_gain, 1);
        assert_eq!(s.lvds_misc, 1);
        assert_eq!(s.test_pattern, 0);
        assert_eq!(s.coarse_gain_mode, 0);
        assert_eq!(s.fine_gain, 0);
        assert_eq!(s.coarse_gain_select, 0);
        assert_eq!(s.sleep_mode, 0);
    }

    #[test]
    #[should_panic(expected = "HMCAD1100 is only used by Logic4/Logic8")]
    fn defaults_panics_for_pro() {
        let _ = Hmcad1100Settings::defaults(DeviceType::LogicPro8);
    }

    // -------------------------------------------------------------------
    // Test pattern lookup tables
    // -------------------------------------------------------------------

    #[test]
    fn test_pattern_tables_from_binary() {
        // Values extracted from DAT_00572e94 and DAT_00572ea0 in the binary.
        assert_eq!(
            TEST_PATTERN_1_VALUES,
            [0x0000, 0x0040, 0x0020, 0x0010, 0x0000, 0x0000]
        );
        assert_eq!(
            TEST_PATTERN_2_VALUES,
            [0x0000, 0x0000, 0x0000, 0x0000, 0x0001, 0x0002]
        );
    }

    // -------------------------------------------------------------------
    // Proxy register write tests
    // -------------------------------------------------------------------

    #[test]
    fn write_hmcad_register_sends_3_register_batch() {
        let mock = std::sync::Arc::new(MockTransport::new());

        // 1 batch write -> 1 ACK
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);

        let mut ep1 = mock_channel_none(&mock);
        write_hmcad_register(&mut ep1, 0x11, 0x0111).unwrap();

        let payloads = extract_out_payloads(&mock);
        assert_eq!(payloads.len(), 1, "should send 1 batch write");

        let p = &payloads[0];
        // WriteRegisters: [0x80, count=3, addr0, val0, addr1, val1, addr2, val2]
        assert_eq!(p[0], 0x80, "opcode should be WriteRegisters");
        assert_eq!(p[1], 0x03, "count should be 3");

        // Pair 0: r0x03 = 0x11 (HMCAD register address)
        assert_eq!(p[2], 0x03, "pair 0 addr (ADC_BANK_SELECT)");
        assert_eq!(p[3], 0x11, "pair 0 value (HMCAD addr)");

        // Pair 1: r0x04 = 0x11 (value low byte)
        assert_eq!(p[4], 0x04, "pair 1 addr (ADC_REG_ADDR)");
        assert_eq!(p[5], 0x11, "pair 1 value (low byte)");

        // Pair 2: r0x05 = 0x01 (value high byte)
        assert_eq!(p[6], 0x05, "pair 2 addr (ADC_REG_DATA)");
        assert_eq!(p[7], 0x01, "pair 2 value (high byte)");
    }

    #[test]
    fn write_hmcad_register_zero_value() {
        let mock = std::sync::Arc::new(MockTransport::new());
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);

        let mut ep1 = mock_channel_none(&mock);
        write_hmcad_register(&mut ep1, 0x25, 0x0000).unwrap();

        let payloads = extract_out_payloads(&mock);
        let p = &payloads[0];
        assert_eq!(p[3], 0x25, "HMCAD addr");
        assert_eq!(p[5], 0x00, "value low byte");
        assert_eq!(p[7], 0x00, "value high byte");
    }

    // -------------------------------------------------------------------
    // Init sequence tests
    // -------------------------------------------------------------------

    #[test]
    fn init_hmcad1100_default_logic8_sends_correct_sequence() {
        let mock = std::sync::Arc::new(MockTransport::new());

        // init_hmcad1100 sends:
        // 1x write_register_8 (FPGA r0x80) -> 1 ACK
        // 10x write_hmcad_register (proxy) -> 10 ACKs
        for _ in 0..11 {
            mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
        }

        let settings = Hmcad1100Settings::defaults(DeviceType::Logic8);
        let mut ep1 = mock_channel_none(&mock);
        init_hmcad1100(&mut ep1, &settings).unwrap();

        let payloads = extract_out_payloads(&mock);
        assert_eq!(payloads.len(), 11, "should send 11 commands");

        // First command: FPGA r0x80=0x00 (direct register write, count=1)
        assert_eq!(payloads[0][0], 0x80, "cmd 0: opcode WriteRegisters");
        assert_eq!(payloads[0][1], 0x01, "cmd 0: count=1");
        assert_eq!(payloads[0][2], 0x80, "cmd 0: FPGA r0x80");
        assert_eq!(payloads[0][3], 0x00, "cmd 0: value=0x00");

        // Commands 1-10: HMCAD SPI writes (3-register batches)
        for i in 1..11 {
            assert_eq!(payloads[i][0], 0x80, "cmd {}: opcode", i);
            assert_eq!(payloads[i][1], 0x03, "cmd {}: count=3 (HMCAD proxy)", i);
        }

        // Verify specific register addresses in the proxy writes
        assert_eq!(payloads[1][3], HMCAD_TEST_PATTERN_1, "cmd 1: r0x25");
        assert_eq!(payloads[2][3], HMCAD_TEST_PATTERN_2, "cmd 2: r0x45");
        assert_eq!(payloads[3][3], HMCAD_CH1_DIGITAL_GAIN, "cmd 3: r0x11");
        assert_eq!(payloads[4][3], HMCAD_CH2_DIGITAL_GAIN, "cmd 4: r0x12");
        assert_eq!(payloads[5][3], HMCAD_LVDS_MISC_A, "cmd 5: r0x2a");
        assert_eq!(payloads[6][3], HMCAD_LVDS_MISC_B, "cmd 6: r0x2b");
        assert_eq!(payloads[7][3], HMCAD_COARSE_GAIN_CFG, "cmd 7: r0x46");
        assert_eq!(payloads[8][3], HMCAD_FINE_GAIN, "cmd 8: r0x50");
        assert_eq!(payloads[9][3], HMCAD_COARSE_GAIN_SEL, "cmd 9: r0x55");
        assert_eq!(payloads[10][3], HMCAD_POWER_SLEEP, "cmd 10: r0x56");

        // Verify gain values for default settings (gain=1)
        // r0x11 = replicate_nibble_3(1) = 0x111
        assert_eq!(payloads[3][5], 0x11, "cmd 3: gain low byte");
        assert_eq!(payloads[3][7], 0x01, "cmd 3: gain high byte");

        // r0x2a = replicate_nibble_4(1) = 0x1111
        assert_eq!(payloads[5][5], 0x11, "cmd 5: LVDS low byte");
        assert_eq!(payloads[5][7], 0x11, "cmd 5: LVDS high byte");
    }

    #[test]
    fn set_test_pattern_writes_two_registers() {
        let mock = std::sync::Arc::new(MockTransport::new());
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);

        let mut ep1 = mock_channel_none(&mock);
        set_test_pattern(&mut ep1, 1).unwrap();

        let payloads = extract_out_payloads(&mock);
        assert_eq!(payloads.len(), 2);

        // Pattern 1: r0x25=0x0040, r0x45=0x0000
        assert_eq!(payloads[0][3], 0x25, "r0x25");
        assert_eq!(payloads[0][5], 0x40, "value low = 0x40");
        assert_eq!(payloads[0][7], 0x00, "value high = 0x00");

        assert_eq!(payloads[1][3], 0x45, "r0x45");
        assert_eq!(payloads[1][5], 0x00, "value low = 0x00");
        assert_eq!(payloads[1][7], 0x00, "value high = 0x00");
    }

    #[test]
    #[should_panic(expected = "HMCAD test pattern must be 0..5")]
    fn set_test_pattern_panics_on_invalid() {
        let mock = std::sync::Arc::new(MockTransport::new());
        let mut ep1 = mock_channel_none(&mock);
        let _ = set_test_pattern(&mut ep1, 6);
    }

    #[test]
    fn init_hmcad1100_full_sends_init_plus_jesd() {
        let mock = std::sync::Arc::new(MockTransport::new());

        // 11 commands for init_hmcad1100 + 5 for JESD power sequence = 16
        for _ in 0..16 {
            mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
        }

        let mut ep1 = mock_channel_none(&mock);
        init_hmcad1100_full(&mut ep1, DeviceType::Logic8).unwrap();

        let payloads = extract_out_payloads(&mock);
        assert_eq!(payloads.len(), 16, "11 HMCAD init + 5 JESD power sequence");

        // Last 5 should be JESD power sequence (count=1, addr=0x00)
        let jesd_expected = [0x1E, 0x1C, 0x18, 0x08, 0x00];
        for i in 0..5 {
            let p = &payloads[11 + i];
            assert_eq!(p[1], 0x01, "JESD step {}: count=1", i);
            assert_eq!(p[2], 0x00, "JESD step {}: addr=0x00", i);
            assert_eq!(p[3], jesd_expected[i], "JESD step {}: value", i);
        }
    }
}
