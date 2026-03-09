//! FPGA register address constants.
//!
//! These addresses are extracted from `libgraph_server_shared.so` v2.4.40
//! via disassembly of `movw` instructions in the MsoFpga code region
//! (VA 0x2d62000-0x2d68000). See `doc/protocol/fpga-registers.md`.
//!
//! Addresses are 16-bit. Values are 32-bit on the wire (big-endian).
//!
//! Registers at addresses <= 0xFF can use the opcode-based 8-bit path
//! (opcodes 0x80/0x81). Registers at addresses > 0xFF require the raw
//! 16-bit path (direct EP1 packets without opcode framing).

// ---------------------------------------------------------------------------
// System registers
// ---------------------------------------------------------------------------

/// SPI bridge to ADC.
///
/// Bits [22:0] = address + data, bit 23 = R/W, bits [31:24] = 0 for writes.
pub const REG_SPI_DATA: u16 = 0x0004;

/// DDR ready status / JESD PHY control / BER test control.
///
/// DDR_LOCKED = bit 4, DDR_CALIBRATED = bit 3.
/// DDR_READY = (value & 0x18) == 0x18.
pub const REG_STATUS_DDR_JESD: u16 = 0x0008;

/// Enables GPIF data interface between FX3 and FPGA.
pub const REG_GPIF_ENABLE: u16 = 0x0020;

/// GPIF flush timeout in 100 MHz clock ticks.
pub const REG_GPIF_FLUSH_TIMEOUT: u16 = 0x0024;

/// Post-programming initialization register.
pub const REG_INIT_CONFIG: u16 = 0x00E0;

/// JESD204B Bit Error Rate test control.
pub const REG_JESD_BER_TEST: u16 = 0x00FC;

// ---------------------------------------------------------------------------
// Capture configuration registers
// ---------------------------------------------------------------------------

/// Master capture control.
pub const REG_CAPTURE_CONTROL: u16 = 0x0040;

/// Scope configuration ID register 0.
pub const REG_SCOPE_CONFIG_ID_0: u16 = 0x0044;

/// Scope configuration ID register 1.
pub const REG_SCOPE_CONFIG_ID_1: u16 = 0x0048;

/// Scope configuration ID register 2.
pub const REG_SCOPE_CONFIG_ID_2: u16 = 0x004C;

/// Scope configuration ID register 3.
pub const REG_SCOPE_CONFIG_ID_3: u16 = 0x0050;

// ---------------------------------------------------------------------------
// Analog channel configuration registers
// ---------------------------------------------------------------------------

/// Analog channel configuration 0.
pub const REG_ANALOG_CH_CONFIG_0: u16 = 0x0054;

/// Analog channel configuration 1.
pub const REG_ANALOG_CH_CONFIG_1: u16 = 0x0058;

/// Analog channel configuration 2.
pub const REG_ANALOG_CH_CONFIG_2: u16 = 0x005C;

/// Analog channel configuration 3.
pub const REG_ANALOG_CH_CONFIG_3: u16 = 0x0060;

/// Analog channel configuration 4.
pub const REG_ANALOG_CH_CONFIG_4: u16 = 0x0064;

/// Analog channel configuration 5.
pub const REG_ANALOG_CH_CONFIG_5: u16 = 0x0068;

// ---------------------------------------------------------------------------
// Trigger registers
// ---------------------------------------------------------------------------

/// Trigger enable/mode. Packed fields: TriggerType, TriggerMode, DownsampleMode.
pub const REG_TRIGGER_CONTROL: u16 = 0x0070;

/// Normal-mode holdoff (must be multiple of 8 samples).
pub const REG_TRIGGER_NORMAL_HOLDOFF: u16 = 0x0074;

/// Normal-mode timeout (must be multiple of 8 samples).
pub const REG_TRIGGER_NORMAL_TIMEOUT: u16 = 0x0078;

/// Auto-mode holdoff/timeout configuration.
pub const REG_TRIGGER_AUTO_CONFIG: u16 = 0x007C;

/// Holdoff sample count, low 32 bits (in FPGA ticks).
pub const REG_TRIGGER_HOLDOFF_LO: u16 = 0x0080;

/// Holdoff sample count, high 32 bits.
pub const REG_TRIGGER_HOLDOFF_HI: u16 = 0x0084;

/// Timeout sample count, low 32 bits (in FPGA ticks).
pub const REG_TRIGGER_TIMEOUT_LO: u16 = 0x0088;

/// Timeout sample count, high 32 bits.
pub const REG_TRIGGER_TIMEOUT_HI: u16 = 0x008C;

/// Initial holdoff and threshold configuration.
pub const REG_TRIGGER_INITIAL: u16 = 0x0090;

/// Sample-to-tick conversion factor.
pub const REG_TRIGGER_TICK_SIZE: u16 = 0x0094;

/// Software trigger timestamp, low 32 bits.
pub const REG_SW_TRIGGER_LO: u16 = 0x0098;

/// Software trigger timestamp, high 32 bits.
pub const REG_SW_TRIGGER_HI: u16 = 0x009C;

// ---------------------------------------------------------------------------
// Test data registers
// ---------------------------------------------------------------------------

/// Test data pattern enable, size.
pub const REG_TEST_DATA_0: u16 = 0x0030;

/// Test data frequency.
pub const REG_TEST_DATA_1: u16 = 0x0034;

// ---------------------------------------------------------------------------
// Digital channel registers
// ---------------------------------------------------------------------------

/// Digital logic voltage threshold.
pub const REG_DIGITAL_THRESHOLD: u16 = 0x0104;

/// Base address for per-channel digital config.
///
/// Channel address = `DIGITAL_CH_CONFIG_BASE + channel * 4` (channels 0..3).
pub const REG_DIGITAL_CH_CONFIG_BASE: u16 = 0x0170;

// ---------------------------------------------------------------------------
// Control registers
// ---------------------------------------------------------------------------

/// Configuration commit / bank control.
pub const REG_COMMIT_CONTROL: u16 = 0x01C0;

/// Digital test pattern configuration.
pub const REG_DIGITAL_TEST_PATTERN: u16 = 0x01C4;

// ---------------------------------------------------------------------------
// Bit masks for REG_STATUS_DDR_JESD (0x0008)
// ---------------------------------------------------------------------------

/// DDR locked status bit (bit 4).
pub const DDR_LOCKED_BIT: u32 = 1 << 4;

/// DDR calibrated status bit (bit 3).
pub const DDR_CALIBRATED_BIT: u32 = 1 << 3;

/// Mask for DDR ready check: both locked and calibrated.
pub const DDR_READY_MASK: u32 = DDR_LOCKED_BIT | DDR_CALIBRATED_BIT;

// ---------------------------------------------------------------------------
// FPGA programming verification
// ---------------------------------------------------------------------------

/// Expected readback value after successful FPGA programming.
pub const FPGA_READBACK_OK: u8 = 0xAA;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Whether a register address requires the 16-bit raw path (> 0xFF).
///
/// Addresses 0x00..=0xFF can use the opcode-based 8-bit path (opcodes
/// 0x80/0x81). Addresses 0x100+ need direct EP1 packets.
pub const fn needs_wide_path(addr: u16) -> bool {
    addr > 0xFF
}

/// Compute per-channel digital config register address.
///
/// # Panics
///
/// Panics if `channel >= 4`.
pub const fn digital_ch_config_addr(channel: u8) -> u16 {
    assert!(channel < 4, "channel must be 0..3");
    REG_DIGITAL_CH_CONFIG_BASE + (channel as u16) * 4
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_addresses_are_aligned() {
        // All register addresses should be 4-byte aligned (32-bit registers).
        let addrs = [
            REG_SPI_DATA,
            REG_STATUS_DDR_JESD,
            REG_GPIF_ENABLE,
            REG_GPIF_FLUSH_TIMEOUT,
            REG_INIT_CONFIG,
            REG_JESD_BER_TEST,
            REG_CAPTURE_CONTROL,
            REG_SCOPE_CONFIG_ID_0,
            REG_TRIGGER_CONTROL,
            REG_DIGITAL_THRESHOLD,
            REG_DIGITAL_CH_CONFIG_BASE,
            REG_COMMIT_CONTROL,
            REG_DIGITAL_TEST_PATTERN,
        ];
        for addr in addrs {
            assert_eq!(
                addr % 4,
                0,
                "register {addr:#06x} is not 4-byte aligned"
            );
        }
    }

    #[test]
    fn digital_ch_config_addresses() {
        assert_eq!(digital_ch_config_addr(0), 0x0170);
        assert_eq!(digital_ch_config_addr(1), 0x0174);
        assert_eq!(digital_ch_config_addr(2), 0x0178);
        assert_eq!(digital_ch_config_addr(3), 0x017C);
    }

    #[test]
    #[should_panic(expected = "channel must be 0..3")]
    fn digital_ch_config_panics_on_invalid_channel() {
        let _ = digital_ch_config_addr(4);
    }

    #[test]
    fn needs_wide_path_boundary() {
        assert!(!needs_wide_path(0x00));
        assert!(!needs_wide_path(0xFF));
        assert!(needs_wide_path(0x100));
        assert!(needs_wide_path(0x01C0));
    }

    #[test]
    fn ddr_ready_mask_is_correct() {
        assert_eq!(DDR_READY_MASK, 0x18);
        // Both bits set -> ready
        assert_eq!(0x18 & DDR_READY_MASK, DDR_READY_MASK);
        // Only locked -> not ready
        assert_ne!(0x10 & DDR_READY_MASK, DDR_READY_MASK);
        // Only calibrated -> not ready
        assert_ne!(0x08 & DDR_READY_MASK, DDR_READY_MASK);
        // Neither -> not ready
        let neither: u32 = 0x00;
        assert_ne!(neither & DDR_READY_MASK, DDR_READY_MASK);
    }

    #[test]
    fn post_init_registers_classification() {
        // Post-init writes to these registers. Verify which need the wide path.
        assert!(needs_wide_path(REG_COMMIT_CONTROL)); // 0x01C0
        assert!(!needs_wide_path(REG_CAPTURE_CONTROL)); // 0x0040
        assert!(!needs_wide_path(REG_INIT_CONFIG)); // 0x00E0
        assert!(!needs_wide_path(REG_GPIF_FLUSH_TIMEOUT)); // 0x0024
        assert!(!needs_wide_path(REG_GPIF_ENABLE)); // 0x0020
        assert!(!needs_wide_path(REG_STATUS_DDR_JESD)); // 0x0008
    }
}
