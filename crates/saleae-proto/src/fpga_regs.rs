//! FPGA register address constants.
//!
//! These addresses are extracted from `libgraph_server_shared.so` v2.4.40
//! via Ghidra decompilation of 59 register functions. See
//! `doc/protocol/fpga-registers.md` and `re/notes/fpga-register-map.md`.
//!
//! Two distinct register interfaces exist:
//!
//! 1. **MsoFpga**: 16-bit addresses, 32-bit values. Direct 6-byte USB
//!    bulk transfers. Used by MSO/oscilloscope device path. 39 addresses.
//!
//! 2. **FpgaDeviceFeatures**: 8-bit addresses, 8-bit values. Wrapped in
//!    EP1 opcode commands (0x80 write, 0x81 read). Used by Logic 4/8 and
//!    Logic Pro 8/16. 21 addresses.
//!
//! Registers at addresses <= 0xFF can use the opcode-based 8-bit path
//! (opcodes 0x80/0x81). Registers at addresses > 0xFF require the raw
//! 16-bit path (direct EP1 packets without opcode framing).

// ---------------------------------------------------------------------------
// System registers
// ---------------------------------------------------------------------------

/// SPI bridge to ADC (MsoFpga path).
///
/// Bit layout (from decompiled WriteSpi, ELF VA 0x2d640c0):
///   bits [22:0]  = SPI address + data (packed per SpiAddressDataFormat)
///   bit  23      = R/W flag (0 = write, 1 = read)
///   bit  24      = SpiWireConfiguration (0 = 4-wire, 1 = 3-wire)
///   bit  26      = SpiClockEdge (0 = rising, 1 = falling)
///   bits [31:27] = must be zero
pub const REG_SPI_DATA: u16 = 0x0004;

/// DDR ready status / JESD PHY control.
///
/// Read bits:
///   bit  0       = JESD link_up
///   bit  3       = DDR calibrated
///   bit  4       = DDR locked
///   bit  5       = cpll_locked (Xilinx) / locked (Altera)
///   bits [15:8]  = sync_status
///   bits [23:16] = aligned / realigned
///
/// Write: value 2 triggers JESD PHY transceiver reset (used by
///   ResetJesdPhyTransceivers in a poll loop with 1 ms sleep,
///   2-second timeout).
///
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
///
/// Bit layout (from decompiled SetAnalogChannelConfiguration):
///   bits [3:0]   = preserved (masked with 0x8F on read-modify-write)
///   bit  4       = capture mode (from config byte 0)
///   bits [6:5]   = DownsampleMode (2 bits)
///   bits [31:7]  = other configuration
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
// Analog channel configuration registers (MsoFpga)
// ---------------------------------------------------------------------------
// These are NOT per-channel registers. They are per-parameter registers
// used by SetAnalogChannelConfiguration. The naming was corrected based
// on TASK-0068 decompilation of the config struct field offsets.

/// Sample rate divider (from config.mSamplesPerSecondDivider).
pub const REG_ANALOG_SAMPLE_DIVIDER: u16 = 0x0054;

/// Negated pre-trigger length (written as -config.mPreTriggerLength).
pub const REG_ANALOG_PRE_TRIGGER: u16 = 0x0058;

/// Post-trigger length (config.mPostTriggerLength).
pub const REG_ANALOG_POST_TRIGGER: u16 = 0x005C;

/// Computed sample rate: round(clock_rate / divider).
pub const REG_ANALOG_COMPUTED_RATE: u16 = 0x0060;

/// Upper trigger bits: (pre_hi & 0xFFFF) | (post_hi & 0xFFFF) << 16.
pub const REG_ANALOG_TRIGGER_HI: u16 = 0x0064;

/// Samples per chunk (config.mSamplesPerChunk).
///
/// FPGA variant constraints:
///   Xilinx: 0x100..0x1E00, MaxTriggerBuffer = 0x3FFFC000
///   Altera: 0x40..0x1780,  MaxTriggerBuffer = 0x2FFFC000
pub const REG_ANALOG_SAMPLES_PER_CHUNK: u16 = 0x0068;

// Keep backward-compatible aliases for existing code.
pub const REG_ANALOG_CH_CONFIG_0: u16 = REG_ANALOG_SAMPLE_DIVIDER;
pub const REG_ANALOG_CH_CONFIG_1: u16 = REG_ANALOG_PRE_TRIGGER;
pub const REG_ANALOG_CH_CONFIG_2: u16 = REG_ANALOG_POST_TRIGGER;
pub const REG_ANALOG_CH_CONFIG_3: u16 = REG_ANALOG_COMPUTED_RATE;
pub const REG_ANALOG_CH_CONFIG_4: u16 = REG_ANALOG_TRIGGER_HI;
pub const REG_ANALOG_CH_CONFIG_5: u16 = REG_ANALOG_SAMPLES_PER_CHUNK;

/// Compute analog parameter register address by index (0..5).
///
/// Registers 0x0054..0x0068 are per-parameter (not per-channel).
///
/// # Panics
///
/// Panics if `index >= 6`.
pub const fn analog_ch_config_addr(index: u8) -> u16 {
    assert!(index < 6, "analog config index must be 0..5");
    REG_ANALOG_SAMPLE_DIVIDER + (index as u16) * 4
}

// ---------------------------------------------------------------------------
// Trigger registers
// ---------------------------------------------------------------------------

/// Trigger control with packed bit fields.
///
/// Bit layout (from decompiled SetAnalogTriggerConfiguration):
///   bits [1:0]   = glitchFilterEnabled / enable
///   bits [3:2]   = TriggerMode (2 bits)
///   bits [7:4]   = TriggerType (4 bits, max 0xF)
///   bits [15:8]  = channelSource
///   bits [31:16] = TriggerThreshold (16 bits, signed)
pub const REG_TRIGGER_CONTROL: u16 = 0x0070;

/// Normal-mode holdoff in ticks: SamplesToTicks(samples) = (samples+7) >> 3.
pub const REG_TRIGGER_NORMAL_HOLDOFF: u16 = 0x0074;

/// Normal-mode timeout in ticks.
pub const REG_TRIGGER_NORMAL_TIMEOUT: u16 = 0x0078;

/// Trigger hysteresis: rising[15:0] | falling[31:16].
pub const REG_TRIGGER_HYSTERESIS: u16 = 0x007C;

/// Initial holdoff, low 32 bits: SamplesToTicks(initial_holdoff).
pub const REG_TRIGGER_INITIAL_HOLDOFF_LO: u16 = 0x0080;

/// Auto-mode holdoff in ticks.
pub const REG_TRIGGER_AUTO_HOLDOFF: u16 = 0x0084;

/// Auto-mode timeout in ticks.
pub const REG_TRIGGER_AUTO_TIMEOUT: u16 = 0x0088;

/// Packed high bits: timeout_hi[11:0] << 16 | holdoff_hi[11:0].
pub const REG_TRIGGER_HOLDOFF_TIMEOUT_HI: u16 = 0x008C;

/// Initial holdoff high: (holdoff >> 35) & 0x1FFF (13 bits).
pub const REG_TRIGGER_INITIAL_HOLDOFF_HI: u16 = 0x0090;

/// SamplesToTicks conversion factor.
pub const REG_TRIGGER_TICK_SIZE: u16 = 0x0094;

/// Software trigger timestamp, low 32 bits.
pub const REG_SW_TRIGGER_LO: u16 = 0x0098;

/// Software trigger timestamp, high 32 bits.
pub const REG_SW_TRIGGER_HI: u16 = 0x009C;

// Backward-compatible aliases for old trigger register names.
pub const REG_TRIGGER_AUTO_CONFIG: u16 = REG_TRIGGER_HYSTERESIS;
pub const REG_TRIGGER_HOLDOFF_LO: u16 = REG_TRIGGER_INITIAL_HOLDOFF_LO;
pub const REG_TRIGGER_HOLDOFF_HI: u16 = REG_TRIGGER_AUTO_HOLDOFF;
pub const REG_TRIGGER_TIMEOUT_LO: u16 = REG_TRIGGER_AUTO_TIMEOUT;
pub const REG_TRIGGER_TIMEOUT_HI: u16 = REG_TRIGGER_HOLDOFF_TIMEOUT_HI;
pub const REG_TRIGGER_INITIAL: u16 = REG_TRIGGER_INITIAL_HOLDOFF_HI;

// ---------------------------------------------------------------------------
// Test data registers
// ---------------------------------------------------------------------------

/// Test data pattern size. Written with config.mSize.
pub const REG_TEST_DATA_SIZE: u16 = 0x0030;

/// Test data frequency and control.
///
/// Bit layout (from decompiled SetTestDataConfiguration):
///   bits [19:0]  = frequency (max 0xFFFFF, 20 bits)
///   bit  20      = mDataObjectEnabled
///   bit  21      = mTestPatternEnabled
///   bits [31:22] = unused
///
/// Constraint: only one of TestPattern and DataObject may be enabled.
pub const REG_TEST_DATA_FREQ_CTL: u16 = 0x0034;

// Backward-compatible aliases.
pub const REG_TEST_DATA_0: u16 = REG_TEST_DATA_SIZE;
pub const REG_TEST_DATA_1: u16 = REG_TEST_DATA_FREQ_CTL;

// ---------------------------------------------------------------------------
// Digital channel registers
// ---------------------------------------------------------------------------

/// Digital logic voltage threshold.
///
/// Referenced by SetDigitalDerivedVoltageThresholds, with an additional
/// secondary write via a different mechanism (possibly I2C DAC).
pub const REG_DIGITAL_THRESHOLD: u16 = 0x0104;

/// Base address for smart cable digital config (MSO only).
///
/// Address formula: 0x0120 + port * 16 + sub_channel * 4
/// Range: 0x0120..0x016F (5 ports x 4 channels = 20 registers).
/// Constraint: port * 16 + sub_channel * 4 < 0x50.
///
/// Each register has the same format as DIGITAL_CH_CONFIG:
///   bits [19:0] = PulseWidth, bit 20 = Enabled.
pub const REG_SMART_CABLE_BASE: u16 = 0x0120;

/// Base address for per-channel digital config (DigitalDerived).
///
/// Address formula: 0x0170 + channel_index * 4 (channels 0..3).
/// Confirmed from decompiled CalculateDigitalConfigurationAddress_DigitalDerived
/// (ELF VA 0x2d66870).
///
/// Per-channel bit layout (from InternalSetDigitalConfiguration):
///   bits [19:0]  = PulseWidth (glitch filter width)
///   bit  20      = Enabled (channel enable)
///   bits [31:21] = unused
///
/// Glitch filter constraints:
///   PulseWidth >= 5 (MinGlitchFilterLength)
///   PulseWidth <= ~250000 (MaxGlitchFilterLength)
///   PulseWidth == 5 || (PulseWidth % 10) == 0
pub const REG_DIGITAL_CH_CONFIG_BASE: u16 = 0x0170;

/// Scope channel configuration register.
pub const REG_SCOPE_CHANNEL_CONFIG: u16 = 0x0194;

// ---------------------------------------------------------------------------
// Control registers
// ---------------------------------------------------------------------------

/// Configuration commit and smart cable channel enable bits.
///
/// Bit layout (from decompiled SetEnabledSmartCableDigitalChannels):
///   bits [19:0]  = smart cable channel enable (5 ports x 4 ch = 20 bits)
///                  bit position = port * 4 + channel
///   bits [31:20] = other commit/bank configuration
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
// JESD204B link status bits in REG_STATUS_DDR_JESD (0x0008)
// ---------------------------------------------------------------------------
// Extracted from decompiled ResetJesdPhyTransceivers (ELF VA 0x2d62200)
// and WaitForDdrReady. The log format includes: "link_up: {}, sync_status:
// {:#x}, realigned: {:#x}, aligned: {:#x}, spll_locked: {}, cpll_locked: {}".

/// JESD link_up status bit (bit 0, confirmed from decompilation).
pub const JESD_LINK_UP_BIT: u32 = 1 << 0;

/// JESD CPLL (Channel PLL) locked bit (bit 5, confirmed for Xilinx).
///
/// For Altera this is a computed "locked" field from sync/align bits.
pub const JESD_CPLL_LOCKED_BIT: u32 = 1 << 5;

/// Mask for JESD link ready check.
///
/// The ResetJesdPhyTransceivers loop writes value 2 to register 0x0008,
/// then reads back and checks specific bits. The exact ready condition
/// varies by FPGA variant (Xilinx checks link_up + cpll_locked, Altera
/// checks sync + locked). We check the minimal set: link_up + cpll_locked.
pub const JESD_READY_MASK: u32 = JESD_LINK_UP_BIT | JESD_CPLL_LOCKED_BIT;

/// Value written to REG_STATUS_DDR_JESD to trigger JESD PHY reset.
///
/// Confirmed from decompilation: ResetJesdPhyTransceivers writes value 2
/// to register 0x0008 in a poll loop (1 ms sleep, 2-second timeout).
/// The channel mask is 0xFF for 4-channel devices, 0x5A for 2-channel.
pub const JESD_PHY_RESET_VALUE: u32 = 0x0000_0002;

// ---------------------------------------------------------------------------
// SPI bridge bit layout constants for REG_SPI_DATA (0x0004)
// ---------------------------------------------------------------------------
// From decompiled WriteSpi (ELF VA 0x2d640c0). The SPI bridge uses a
// single 32-bit register write to perform an SPI transaction to the ADC.

/// SPI R/W flag bit position (bit 23). 0 = write, 1 = read.
pub const SPI_RW_BIT: u32 = 1 << 23;

/// SPI wire configuration bit position (bit 24). 0 = 4-wire, 1 = 3-wire.
pub const SPI_WIRE_CONFIG_BIT: u32 = 1 << 24;

/// SPI clock edge bit position (bit 26). 0 = rising, 1 = falling.
pub const SPI_CLOCK_EDGE_BIT: u32 = 1 << 26;

/// Mask for SPI address+data field (bits [22:0]).
pub const SPI_ADDR_DATA_MASK: u32 = 0x007F_FFFF;

// ---------------------------------------------------------------------------
// ADC SPI register addresses (via REG_SPI_DATA at 0x0004)
// ---------------------------------------------------------------------------
// The ADS528x ADC (Lattice-based devices) is configured via SPI writes
// tunneled through the FPGA. The FpgaDeviceFeatures path (Logic Pro 8/16
// with Xilinx) uses a different ADC (AD9637) with a multi-register
// protocol through regs 0x03-0x05, 0x16.
//
// The SPI values below are for the MsoFpga path. The exact ADS528x
// register addresses and data values need USB capture confirmation --
// the addresses below are reasonable defaults from the ADS528x datasheet.

/// SPI word to set ADS528x drive strength (awaiting USB capture confirmation).
///
/// ADS528x output driver configuration register. Controls LVDS output
/// swing for JESD204B data lanes.
pub const ADC_SPI_DRIVE_STRENGTH_ADDR: u32 = 0x0000_0042;

/// ADC drive strength value for normal operation (awaiting confirmation).
pub const ADC_SPI_DRIVE_STRENGTH_VALUE: u32 = 0x0000_0001;

/// SPI word to enable the ADS528x ADC (awaiting USB capture confirmation).
pub const ADC_SPI_ENABLE_ADDR: u32 = 0x0000_0000;

/// ADC enable value (awaiting confirmation).
pub const ADC_SPI_ENABLE_VALUE: u32 = 0x0000_0001;

// ---------------------------------------------------------------------------
// JESD PHY reset constants for REG_STATUS_DDR_JESD (0x0008)
// ---------------------------------------------------------------------------
// From decompiled ResetJesdPhyTransceivers: the function asserts
// channel_count == 2 || channel_count == 4, then writes value 2 to
// register 0x0008 in a poll loop. The channel mask selects which
// SERDES lanes to reset: 0xFF for 4 channels, 0x5A for 2 channels.

/// Value to assert JESD PHY transceiver reset.
///
/// Confirmed from decompilation: the reset loop writes exactly 2 to
/// register 0x0008. This is NOT an assert/deassert toggle -- it is
/// a write-and-poll pattern.
pub const JESD_PHY_RESET_ASSERT: u32 = JESD_PHY_RESET_VALUE;

/// Value written after reset is complete (deassert).
///
/// The decompiled code does not explicitly deassert. After the poll
/// loop succeeds (JESD locks), the register is not cleared. Writing
/// 0 here is a conservative default.
pub const JESD_PHY_RESET_DEASSERT: u32 = 0x0000_0000;

/// JESD lane mask for 4-channel devices (Logic Pro 16, etc.).
pub const JESD_LANE_MASK_4CH: u8 = 0xFF;

/// JESD lane mask for 2-channel devices (Logic Pro 8, etc.).
pub const JESD_LANE_MASK_2CH: u8 = 0x5A;

// ---------------------------------------------------------------------------
// SPI bridge configuration for REG_SPI_DATA (0x0004)
// ---------------------------------------------------------------------------

/// Initial SPI bridge configuration for ADC access.
///
/// The actual configuration depends on SpiWireConfiguration and
/// SpiClockEdge settings for the specific ADC type. For ADS528x,
/// the default is 4-wire SPI with rising clock edge (value 0).
pub const SPI_BRIDGE_CONFIG_FOR_ADC: u32 = 0x0000_0000;

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

/// Compute per-channel digital config register address (DigitalDerived).
///
/// Formula: `0x0170 + channel_index * 4` (confirmed from decompilation).
///
/// # Panics
///
/// Panics if `channel >= 4`.
pub const fn digital_ch_config_addr(channel: u8) -> u16 {
    assert!(channel < 4, "channel must be 0..3");
    REG_DIGITAL_CH_CONFIG_BASE + (channel as u16) * 4
}

/// Compute smart cable digital config register address.
///
/// Formula: `0x0120 + port * 16 + sub_channel * 4` (confirmed from
/// decompiled CalculateDigitalConfigurationAddress_SmartCable,
/// ELF VA 0x2d66380).
///
/// Constraint: `port * 16 + sub_channel * 4 < 0x50` (80 bytes max).
///
/// # Panics
///
/// Panics if the computed offset exceeds the valid range.
pub const fn smart_cable_config_addr(port: u8, sub_channel: u8) -> u16 {
    let offset = (port as u16) * 16 + (sub_channel as u16) * 4;
    assert!(offset < 0x50, "smart cable address out of range");
    REG_SMART_CABLE_BASE + offset
}

// ===========================================================================
// FpgaDeviceFeatures registers (8-bit address, 8-bit value)
// ===========================================================================
// Used by Logic 4/8 and Logic Pro 8/16 via EP1 opcode 0x80 (write) / 0x81
// (read). Max 31 register pairs per batch write, 63 per batch read.
// Source: Logic2FpgaDevice::SetCaptureParameters decompilation.

/// Capture init register (written as 0 at start of SetCaptureParameters).
pub const FEAT_CAPTURE_INIT: u8 = 0x00;

/// ADC SPI address high byte (for WriteAd9637Register protocol).
pub const FEAT_ADC_SPI_ADDR: u8 = 0x03;

/// ADC SPI data low byte.
pub const FEAT_ADC_SPI_DATA_LO: u8 = 0x04;

/// ADC SPI data high byte.
pub const FEAT_ADC_SPI_DATA_HI: u8 = 0x05;

/// Analog channel enable mask, low byte.
pub const FEAT_ANALOG_CH_ENABLE_LO: u8 = 0x06;

/// Analog channel enable mask, high byte (16-channel devices).
pub const FEAT_ANALOG_CH_ENABLE_HI: u8 = 0x07;

/// Digital channel enable mask, low byte.
pub const FEAT_DIGITAL_CH_ENABLE_LO: u8 = 0x08;

/// Digital channel enable mask, high byte (16-channel devices).
pub const FEAT_DIGITAL_CH_ENABLE_HI: u8 = 0x09;

/// Analog decimation factor, low byte.
pub const FEAT_ANALOG_DECIMATION: u8 = 0x0A;

/// Digital sample rate divider, low byte.
pub const FEAT_DIGITAL_DIVIDER_LO: u8 = 0x0B;

/// Digital sample rate divider, high byte.
pub const FEAT_DIGITAL_DIVIDER_HI: u8 = 0x0C;

/// Analog/digital mix ratio A.
pub const FEAT_MIX_RATIO_A: u8 = 0x0D;

/// Analog/digital mix ratio B.
pub const FEAT_MIX_RATIO_B: u8 = 0x0E;

/// Capture control flags.
///
/// Bit layout:
///   bit 0 = capture enabled (board rev 2 always 1)
///   bit 1 = ignore FPGA errors flag
pub const FEAT_CAPTURE_CONTROL: u8 = 0x13;

/// Computed decimation factor (max 0xFF).
pub const FEAT_DECIMATION_FACTOR: u8 = 0x14;

/// Extended ADC address bits (for WriteAd9637Register protocol).
pub const FEAT_ADC_SPI_ADDR_EXT: u8 = 0x16;

/// ADC configuration (Pro devices, JESD mode only).
///
/// Value encoding: 1 = standard, 0 = 4x interleaved, 2*N = other.
pub const FEAT_ADC_CONFIG: u8 = 0x1A;

/// Upper analog decimation (board revision 1 only, max value 1).
pub const FEAT_ANALOG_DEC_UPPER: u8 = 0x1B;

/// PLL/DCM lock status register (read-only).
///
/// Bit layout (from SetAd9637Enabled decompilation):
///   bit 5 = Main DCM locked
///   bit 6 = Slow DCM locked
pub const FEAT_PLL_STATUS: u8 = 0x40;

/// Secondary PLL status (read-only, read alongside FEAT_PLL_STATUS).
pub const FEAT_PLL_STATUS_2: u8 = 0x4A;

// PLL status bit masks.
pub const PLL_MAIN_DCM_LOCKED: u8 = 1 << 5;
pub const PLL_SLOW_DCM_LOCKED: u8 = 1 << 6;

// ===========================================================================
// JESD204B power sequence values (pcap-validated, register 0x00)
// ===========================================================================
// From pcap Phase 3c (init-sequence-pcap.md): the JESD link is brought up
// by a staged power sequence writing to register 0x00:
//   0x1e -> 0x1c -> 0x18 -> 0x08 -> 0x00
// Each step releases one more subsystem from reset. The same sequence is
// used for power-up (Phases 3c, 3f, 7) and power-down (Phase 8).

/// JESD power step 0: all subsystems held in reset.
pub const JESD_POWER_ALL_RESET: u8 = 0x1E;

/// JESD power step 1: first subsystem released (bit 1 cleared).
pub const JESD_POWER_STEP1: u8 = 0x1C;

/// JESD power step 2: second subsystem released (bit 2 cleared).
pub const JESD_POWER_STEP2: u8 = 0x18;

/// JESD power step 3: third subsystem released (bit 4 cleared).
pub const JESD_POWER_STEP3: u8 = 0x08;

/// JESD power step 4: all subsystems released, JESD link active.
pub const JESD_POWER_ALL_ACTIVE: u8 = 0x00;

/// The complete JESD power sequence in order.
pub const JESD_POWER_SEQUENCE: [u8; 5] = [
    JESD_POWER_ALL_RESET,
    JESD_POWER_STEP1,
    JESD_POWER_STEP2,
    JESD_POWER_STEP3,
    JESD_POWER_ALL_ACTIVE,
];

// ===========================================================================
// ADC indirect register access (pcap-validated, registers 0x03-0x05, 0x16)
// ===========================================================================
// The FPGA provides indirect access to ADC registers through a 4-register
// protocol observed in pcap Phase 3b:
//   r0x03 = bank/register address
//   r0x16 = address high byte (always 0x00 in pcap)
//   r0x04 = register data value
//   r0x05 = data high byte (always 0x00 in pcap)
//
// These overlap with the existing FEAT_ADC_SPI_* constants because the same
// physical registers serve dual purpose: SPI bridge on Xilinx paths, indirect
// ADC bank access on Lattice/FpgaDeviceFeatures paths.

/// ADC bank select register (indirect access via FPGA proxy).
pub const ADC_BANK_SELECT: u8 = 0x03;

/// ADC register address (indirect access -- data low byte).
pub const ADC_REG_ADDR: u8 = 0x04;

/// ADC register data (indirect access -- data high byte).
pub const ADC_REG_DATA: u8 = 0x05;

/// ADC register trigger / address high (indirect access).
///
/// Always written as 0x00 in pcap. Must be written before ADC_REG_ADDR
/// and ADC_REG_DATA to complete the indirect access transaction.
pub const ADC_REG_TRIGGER: u8 = 0x16;

// ===========================================================================
// Calibration coefficient register (pcap-validated)
// ===========================================================================

/// Calibration coefficient storage register.
///
/// From pcap Phase 3e: 16 sequential writes to register 0x17 load
/// calibration coefficients into the FPGA. Values are device-specific
/// and come from the server-hosted calibration file.
pub const CAL_COEFF_REG: u8 = 0x17;

/// Maximum number of calibration coefficients.
pub const CAL_COEFF_MAX_COUNT: usize = 16;

// ===========================================================================
// JESD link status registers (pcap-validated, read-only)
// ===========================================================================

/// JESD link status register (read-only, FpgaDeviceFeatures path).
///
/// From pcap: 0xE0 = link established, 0x00 = link down.
/// Read after JESD power sequence to verify link establishment.
pub const FEAT_JESD_LINK_STATUS: u8 = 0x40;

/// JESD link established value (all link indicator bits set).
pub const FEAT_JESD_LINK_ESTABLISHED: u8 = 0xE0;

/// JESD status detail register (read-only, FpgaDeviceFeatures path).
pub const FEAT_JESD_STATUS_DETAIL: u8 = 0x4A;

/// General status register (read-only, FpgaDeviceFeatures path).
pub const FEAT_STATUS_GENERAL: u8 = 0x19;

/// Bank power control register.
///
/// From pcap: 0x00 = power off, 0x03 = power on.
pub const FEAT_BANK_POWER: u8 = 0x15;

/// Capture config register (observed in Phase 3f as r0x12).
pub const FEAT_CAPTURE_CONFIG_2: u8 = 0x12;

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
            assert_eq!(addr % 4, 0, "register {addr:#06x} is not 4-byte aligned");
        }
    }

    #[test]
    fn analog_ch_config_addresses() {
        assert_eq!(analog_ch_config_addr(0), 0x0054);
        assert_eq!(analog_ch_config_addr(1), 0x0058);
        assert_eq!(analog_ch_config_addr(2), 0x005C);
        assert_eq!(analog_ch_config_addr(3), 0x0060);
        assert_eq!(analog_ch_config_addr(4), 0x0064);
        assert_eq!(analog_ch_config_addr(5), 0x0068);
    }

    #[test]
    #[should_panic(expected = "analog config index must be 0..5")]
    fn analog_ch_config_panics_on_invalid_channel() {
        let _ = analog_ch_config_addr(6);
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
    fn smart_cable_config_addresses() {
        // Port 0, channels 0-3.
        assert_eq!(smart_cable_config_addr(0, 0), 0x0120);
        assert_eq!(smart_cable_config_addr(0, 1), 0x0124);
        assert_eq!(smart_cable_config_addr(0, 2), 0x0128);
        assert_eq!(smart_cable_config_addr(0, 3), 0x012C);
        // Port 1, channel 0.
        assert_eq!(smart_cable_config_addr(1, 0), 0x0130);
        // Port 4, channel 3 (last valid).
        assert_eq!(smart_cable_config_addr(4, 3), 0x016C);
    }

    #[test]
    #[should_panic(expected = "smart cable address out of range")]
    fn smart_cable_config_panics_out_of_range() {
        // port=5, sub_channel=0 -> offset = 80 = 0x50, which is out of range.
        let _ = smart_cable_config_addr(5, 0);
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
        // Both bits set -> ready.
        assert_eq!(0x18 & DDR_READY_MASK, DDR_READY_MASK);
        // Only locked -> not ready.
        assert_ne!(0x10 & DDR_READY_MASK, DDR_READY_MASK);
        // Only calibrated -> not ready.
        assert_ne!(0x08 & DDR_READY_MASK, DDR_READY_MASK);
        // Neither -> not ready.
        let neither: u32 = 0x00;
        assert_ne!(neither & DDR_READY_MASK, DDR_READY_MASK);
    }

    #[test]
    fn jesd_ready_mask_verified() {
        // JESD_READY_MASK = link_up (bit 0) | cpll_locked (bit 5) = 0x21.
        assert_eq!(JESD_READY_MASK, 0x21);
        // Both set -> ready.
        assert_eq!(0x21 & JESD_READY_MASK, JESD_READY_MASK);
        // Only link_up -> not ready.
        assert_ne!(0x01 & JESD_READY_MASK, JESD_READY_MASK);
        // Only cpll_locked -> not ready.
        assert_ne!(0x20 & JESD_READY_MASK, JESD_READY_MASK);
    }

    #[test]
    fn jesd_phy_reset_value_is_2() {
        assert_eq!(JESD_PHY_RESET_VALUE, 2);
        assert_eq!(JESD_PHY_RESET_ASSERT, 2);
    }

    #[test]
    fn spi_bit_positions() {
        assert_eq!(SPI_RW_BIT, 0x0080_0000);
        assert_eq!(SPI_WIRE_CONFIG_BIT, 0x0100_0000);
        assert_eq!(SPI_CLOCK_EDGE_BIT, 0x0400_0000);
        assert_eq!(SPI_ADDR_DATA_MASK, 0x007F_FFFF);
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

    #[test]
    fn new_register_addresses_are_aligned() {
        let addrs = [
            REG_SMART_CABLE_BASE,
            REG_SCOPE_CHANNEL_CONFIG,
            REG_ANALOG_SAMPLE_DIVIDER,
            REG_ANALOG_PRE_TRIGGER,
            REG_ANALOG_POST_TRIGGER,
            REG_ANALOG_COMPUTED_RATE,
            REG_ANALOG_TRIGGER_HI,
            REG_ANALOG_SAMPLES_PER_CHUNK,
            REG_TRIGGER_HYSTERESIS,
            REG_TRIGGER_INITIAL_HOLDOFF_LO,
            REG_TRIGGER_AUTO_HOLDOFF,
            REG_TRIGGER_AUTO_TIMEOUT,
            REG_TRIGGER_HOLDOFF_TIMEOUT_HI,
            REG_TRIGGER_INITIAL_HOLDOFF_HI,
            REG_TEST_DATA_SIZE,
            REG_TEST_DATA_FREQ_CTL,
        ];
        for addr in addrs {
            assert_eq!(addr % 4, 0, "register {addr:#06x} is not 4-byte aligned");
        }
    }

    #[test]
    fn backward_compat_aliases() {
        // Verify backward-compatible aliases match the new names.
        assert_eq!(REG_ANALOG_CH_CONFIG_0, REG_ANALOG_SAMPLE_DIVIDER);
        assert_eq!(REG_ANALOG_CH_CONFIG_1, REG_ANALOG_PRE_TRIGGER);
        assert_eq!(REG_ANALOG_CH_CONFIG_2, REG_ANALOG_POST_TRIGGER);
        assert_eq!(REG_ANALOG_CH_CONFIG_3, REG_ANALOG_COMPUTED_RATE);
        assert_eq!(REG_ANALOG_CH_CONFIG_4, REG_ANALOG_TRIGGER_HI);
        assert_eq!(REG_ANALOG_CH_CONFIG_5, REG_ANALOG_SAMPLES_PER_CHUNK);

        assert_eq!(REG_TRIGGER_AUTO_CONFIG, REG_TRIGGER_HYSTERESIS);
        assert_eq!(REG_TRIGGER_HOLDOFF_LO, REG_TRIGGER_INITIAL_HOLDOFF_LO);
        assert_eq!(REG_TRIGGER_HOLDOFF_HI, REG_TRIGGER_AUTO_HOLDOFF);
        assert_eq!(REG_TRIGGER_TIMEOUT_LO, REG_TRIGGER_AUTO_TIMEOUT);
        assert_eq!(REG_TRIGGER_TIMEOUT_HI, REG_TRIGGER_HOLDOFF_TIMEOUT_HI);
        assert_eq!(REG_TRIGGER_INITIAL, REG_TRIGGER_INITIAL_HOLDOFF_HI);

        assert_eq!(REG_TEST_DATA_0, REG_TEST_DATA_SIZE);
        assert_eq!(REG_TEST_DATA_1, REG_TEST_DATA_FREQ_CTL);
    }

    #[test]
    fn pll_status_bits() {
        assert_eq!(PLL_MAIN_DCM_LOCKED, 0x20);
        assert_eq!(PLL_SLOW_DCM_LOCKED, 0x40);
    }

    #[test]
    fn jesd_power_sequence_values() {
        // Verify the exact pcap-observed values.
        assert_eq!(JESD_POWER_ALL_RESET, 0x1E);
        assert_eq!(JESD_POWER_STEP1, 0x1C);
        assert_eq!(JESD_POWER_STEP2, 0x18);
        assert_eq!(JESD_POWER_STEP3, 0x08);
        assert_eq!(JESD_POWER_ALL_ACTIVE, 0x00);

        // The array must match the exact ordering.
        assert_eq!(
            JESD_POWER_SEQUENCE,
            [0x1E, 0x1C, 0x18, 0x08, 0x00]
        );
    }

    #[test]
    fn jesd_power_sequence_bit_clearing_pattern() {
        // Each step clears one more bit from the starting value 0x1E.
        // 0x1E = 0b0001_1110 (bits 1,2,3,4 set)
        // Step 0: 0x1E = 0b0001_1110 (all reset bits set)
        // Step 1: 0x1C = 0b0001_1100 (bit 1 cleared)
        // Step 2: 0x18 = 0b0001_1000 (bit 2 cleared)
        // Step 3: 0x08 = 0b0000_1000 (bit 4 cleared)
        // Step 4: 0x00 = 0b0000_0000 (bit 3 cleared)
        assert_eq!(JESD_POWER_ALL_RESET & !0x02, JESD_POWER_STEP1);
        assert_eq!(JESD_POWER_STEP1 & !0x04, JESD_POWER_STEP2);
        assert_eq!(JESD_POWER_STEP2 & !0x10, JESD_POWER_STEP3);
        assert_eq!(JESD_POWER_STEP3 & !0x08, JESD_POWER_ALL_ACTIVE);
    }

    #[test]
    fn adc_indirect_register_addresses() {
        // These must match existing FEAT_ADC_SPI_* constants (same registers).
        assert_eq!(ADC_BANK_SELECT, FEAT_ADC_SPI_ADDR);
        assert_eq!(ADC_REG_ADDR, FEAT_ADC_SPI_DATA_LO);
        assert_eq!(ADC_REG_DATA, FEAT_ADC_SPI_DATA_HI);
        assert_eq!(ADC_REG_TRIGGER, FEAT_ADC_SPI_ADDR_EXT);
    }

    #[test]
    fn cal_coeff_register() {
        assert_eq!(CAL_COEFF_REG, 0x17);
        assert_eq!(CAL_COEFF_MAX_COUNT, 16);
    }

    #[test]
    fn jesd_link_status_constants() {
        assert_eq!(FEAT_JESD_LINK_STATUS, 0x40);
        assert_eq!(FEAT_JESD_LINK_ESTABLISHED, 0xE0);
        assert_eq!(FEAT_JESD_STATUS_DETAIL, 0x4A);
        assert_eq!(FEAT_STATUS_GENERAL, 0x19);
        assert_eq!(FEAT_BANK_POWER, 0x15);
        assert_eq!(FEAT_CAPTURE_CONFIG_2, 0x12);

        // FEAT_JESD_LINK_STATUS is the same register as FEAT_PLL_STATUS
        // (both are 0x40, read-only, different interpretations per device path).
        assert_eq!(FEAT_JESD_LINK_STATUS, FEAT_PLL_STATUS);
    }
}
