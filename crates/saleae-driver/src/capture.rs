//! Digital capture: configure, start, stream, stop.
//!
//! This module implements the capture lifecycle for digital channels.
//! The sequence mirrors the original software's `StartSampling` flow:
//!
//! 1. Validate and build capture configuration.
//! 2. Write capture parameters to FPGA registers via EP1 WriteRegisters
//!    (opcode 0x80), then read back and verify via ReadRegisters (0x81).
//! 3. Send START_READ command (0x01) via EP1.
//! 4. Write r0x00=0x01 to trigger capture.
//! 5. Read data from EP2 IN (0x82) as `DataChunk`s.
//! 6. Write r0x00=0x00, then send STOP_STREAM (0x02) to end capture.
//!
//! # RE source
//!
//! Register mapping from decompiled `Logic2FpgaDevice::SetCaptureParameters`
//! at Ghidra 01d24ca0 (ELF 0x1c24ca0, 6598 bytes) and validated against
//! pcap Phase 7 from `re/notes/init-sequence-pcap.md`.
//!
//! Key finding: capture config uses WriteRegisters (opcode 0x80) with
//! 8-bit FpgaDeviceFeatures registers, NOT WriteConfigurationData (0x7F).
//! The 0x7F path is for FPGA bitstream upload only.

use saleae_proto::capture_data::{self, DataChunk};
use saleae_proto::device::{endpoint, FpgaType, UsbSpeed};
use saleae_proto::fpga_regs;
use tracing::{debug, info, warn};

use crate::ep1::Ep1CommandChannel;
use crate::error::{DriverError, DriverResult};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// EP2 IN endpoint for capture data.
pub const EP2_CAPTURE_IN: u8 = endpoint::CAPTURE_IN;

/// Default USB bulk transfer size for EP2 reads (16 x 1024 = 16 KiB).
///
/// This is a placeholder. The correct transfer size should be computed
/// using [`crate::transfer_queue::TransferConfig::from_capture`], which
/// implements the Saleae formula: bandwidth/240 rounded to block size,
/// capped at 1.6 MB. At high sample rates the computed size can be up
/// to 1,638,400 bytes (100 x 16 KiB blocks). Using 16 KiB here limits
/// throughput to ~3.75 MB/s (240 completions/s * 16 KiB).
pub const DEFAULT_TRANSFER_SIZE: usize = 16 * 1024;

/// Default samples per chunk. Must be a multiple of 256 (WithStats).
///
/// PLACEHOLDER: The actual value depends on sample rate, channel count,
/// and FPGA type. 256 is the minimum for Xilinx.
pub const DEFAULT_SAMPLES_PER_CHUNK: u32 = 256;

// ---------------------------------------------------------------------------
// CaptureConfig
// ---------------------------------------------------------------------------

/// Configuration for a digital capture session.
///
/// Device context fields (`total_device_channels`, `board_revision`,
/// `is_pro`) are needed to compute the correct FPGA register values.
/// These come from the device profile and EEPROM hardware revision.
#[derive(Debug, Clone)]
pub struct CaptureConfig {
    /// Sample rate in Hz.
    pub sample_rate: u64,
    /// Enabled digital channel indices (0-based).
    pub digital_channels: Vec<u8>,
    /// Enabled analog channel indices (0-based). Empty for digital-only.
    pub analog_channels: Vec<u8>,
    /// Number of samples to capture. 0 = capture until stop.
    pub duration_samples: u64,
    /// Voltage threshold for digital logic level (volts).
    pub voltage_threshold: f64,
    /// Whether to enable per-packet stats collection.
    pub stats_enabled: bool,
    /// Samples per chunk (must be multiple of 256 when stats enabled).
    pub samples_per_chunk: u32,

    // --- Device context (from DeviceProfile + HwRevision) ---
    /// Total channels on the device (8 for Pro 8, 16 for Pro 16).
    ///
    /// Determines whether high-byte channel enable registers (0x07, 0x09)
    /// are written. Only 16-channel devices use the high bytes.
    pub total_device_channels: u8,
    /// Board hardware revision (0 or 1 for Xilinx, 3+ for Lattice).
    ///
    /// Affects: FEAT_ANALOG_DEC_UPPER (0x1B) is only written for rev 1.
    /// Rev 0 asserts `analog_decimation_upper == 0`.
    pub board_revision: u8,
    /// Whether this is a Pro device (Logic Pro 8/16).
    ///
    /// Affects: ADC config register (0x1A) and capture control flags.
    /// Pro devices use JESD mode, non-Pro do not.
    pub is_pro: bool,
    /// Base digital clock rate in Hz (200 MHz for Logic 8, 500 MHz for Pro).
    ///
    /// Used to compute the digital sample rate divider.
    pub base_digital_rate: u64,
    /// Base analog clock rate in Hz (20 MHz for Logic 8, 100 MHz for Pro).
    ///
    /// Used to compute the analog decimation factor.
    pub base_analog_rate: u64,
    /// USB connection speed. Used for bandwidth validation.
    pub usb_speed: UsbSpeed,
    /// FPGA type. Determines samples-per-chunk limits.
    pub fpga_type: FpgaType,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            sample_rate: 1_000_000,
            digital_channels: vec![0, 1, 2, 3],
            analog_channels: vec![],
            duration_samples: 0,
            voltage_threshold: 1.65,
            stats_enabled: true,
            samples_per_chunk: DEFAULT_SAMPLES_PER_CHUNK,
            // Default: Logic Pro 8 context.
            total_device_channels: 8,
            board_revision: 1,
            is_pro: true,
            base_digital_rate: 500_000_000,
            base_analog_rate: 100_000_000,
            usb_speed: UsbSpeed::Super,
            fpga_type: FpgaType::Pro,
        }
    }
}

impl CaptureConfig {
    /// Validate the capture configuration.
    pub fn validate(&self) -> DriverResult<()> {
        if self.sample_rate == 0 {
            return Err(DriverError::InvalidCaptureConfig {
                reason: "sample rate must be > 0".into(),
            });
        }

        if self.digital_channels.is_empty() {
            return Err(DriverError::InvalidCaptureConfig {
                reason: "at least one digital channel must be enabled".into(),
            });
        }

        for &ch in &self.digital_channels {
            if ch >= 16 {
                return Err(DriverError::InvalidCaptureConfig {
                    reason: format!("channel {ch} out of range (max 15)"),
                });
            }
        }

        if self.stats_enabled
            && self.samples_per_chunk
                % (capture_data::SAMPLES_PER_PACKET_MULTIPLE_WITH_STATS as u32)
                != 0
        {
            return Err(DriverError::InvalidCaptureConfig {
                reason: format!(
                    "samples_per_chunk ({}) must be a multiple of {} when stats enabled",
                    self.samples_per_chunk,
                    capture_data::SAMPLES_PER_PACKET_MULTIPLE_WITH_STATS
                ),
            });
        }

        if self.samples_per_chunk < 64 {
            return Err(DriverError::InvalidCaptureConfig {
                reason: format!(
                    "samples_per_chunk ({}) must be >= 64",
                    self.samples_per_chunk
                ),
            });
        }

        Ok(())
    }

    /// Validate that the capture configuration fits within USB bandwidth
    /// and FPGA packet size limits.
    ///
    /// Must be called before sending configuration to the device. The USB
    /// bus has a hard throughput ceiling and the FPGA has min/max limits
    /// on samples_per_chunk.
    ///
    /// # Bandwidth formula
    ///
    /// ```text
    /// digital_bw   = sample_rate * digital_channels * 1 byte
    /// analog_bw    = sample_rate * analog_channels  * 2 bytes (12-bit -> 16-bit)
    /// total_bw     = digital_bw + analog_bw
    /// ```
    ///
    /// USB limits (practical sustained throughput, not raw signaling rate):
    /// - USB 2.0 High Speed: 37.5 MB/s (300 Mbit/s)
    /// - USB 3.0 SuperSpeed: 375 MB/s (3 Gbit/s)
    ///
    /// FPGA samples_per_chunk limits:
    /// - Xilinx (Pro/NonPro): 256..=7680
    /// - Altera: 64..=6016
    pub fn validate_bandwidth(&self) -> DriverResult<()> {
        // --- Bandwidth check ---

        let digital_count = self.digital_channels.len() as u64;
        let analog_count = self.analog_channels.len() as u64;

        // Digital: 1 byte per sample per channel.
        // Analog: 2 bytes per sample per channel (12-bit ADC packed to 16-bit).
        let bytes_per_sample_digital = 1u64;
        let bytes_per_sample_analog = 2u64;

        let total_bandwidth = self.sample_rate.saturating_mul(
            digital_count * bytes_per_sample_digital + analog_count * bytes_per_sample_analog,
        );

        let max_bandwidth: u64 = match self.usb_speed {
            UsbSpeed::Super | UsbSpeed::SuperPlus => 375_000_000, // 375 MB/s
            UsbSpeed::High => 37_500_000,                         // 37.5 MB/s
            _ => {
                return Err(DriverError::InvalidCaptureConfig {
                    reason: format!("USB speed {} is too slow for capture", self.usb_speed,),
                });
            }
        };

        if total_bandwidth > max_bandwidth {
            let digital_bw = self.sample_rate * digital_count * bytes_per_sample_digital;
            let analog_bw = self.sample_rate * analog_count * bytes_per_sample_analog;
            return Err(DriverError::InvalidCaptureConfig {
                reason: format!(
                    "capture bandwidth {total_bandwidth} B/s exceeds {} limit of {max_bandwidth} B/s \
                     (digital: {digital_count} ch x {} Hz = {digital_bw} B/s, \
                     analog: {analog_count} ch x {} Hz x 2 = {analog_bw} B/s)",
                    self.usb_speed, self.sample_rate, self.sample_rate,
                ),
            });
        }

        // --- FPGA samples_per_chunk limits ---

        let (min_chunk, max_chunk) = match self.fpga_type {
            FpgaType::NonPro | FpgaType::Pro => (256u32, 7680u32), // Xilinx
            FpgaType::Mso => (64u32, 6016u32),                     // Altera
        };

        if self.samples_per_chunk < min_chunk || self.samples_per_chunk > max_chunk {
            return Err(DriverError::InvalidCaptureConfig {
                reason: format!(
                    "samples_per_chunk {} is outside {} range [{min_chunk}, {max_chunk}]",
                    self.samples_per_chunk, self.fpga_type,
                ),
            });
        }

        Ok(())
    }

    /// Number of digital channel groups (2 or 4).
    ///
    /// The FPGA always captures in groups of 2 or 4 channels.
    fn channel_group_count(&self) -> usize {
        let max_ch = self.digital_channels.iter().max().copied().unwrap_or(0);
        if max_ch < 2 {
            2
        } else {
            4
        }
    }

    /// Bytes per sample row for this configuration (digital only).
    ///
    /// Each sample row has `channel_group_count / 8` bytes of digital
    /// data. For 2 or 4 channels captured together, one byte holds
    /// multiple sample periods.
    ///
    /// PLACEHOLDER: This is a simplification. The actual row width
    /// depends on the GPIF bus width and whether analog channels are
    /// also enabled.
    pub fn bytes_per_sample_row(&self) -> usize {
        // For digital-only: one byte holds 8/channel_count samples.
        // So bytes_per_row = 1 for both 2 and 4 channels.
        // The sample data in each chunk is: samples_per_chunk / (8/channel_count) bytes.
        1
    }

    /// Total sample data bytes per chunk.
    pub fn sample_data_bytes_per_chunk(&self) -> usize {
        // Each byte holds (8 / channel_group_count) samples.
        let samples_per_byte = 8 / self.channel_group_count();
        (self.samples_per_chunk as usize).div_ceil(samples_per_byte)
    }
}

// ---------------------------------------------------------------------------
// Analog capture configuration
// ---------------------------------------------------------------------------

/// Configuration for an analog capture session.
///
/// Separate from [`CaptureConfig`] because analog capture has different
/// parameters: per-channel AFE configuration, calibration-dependent
/// sample processing, and different data formats (int16 vs 1-bit).
#[derive(Debug, Clone)]
pub struct AnalogCaptureConfig {
    /// Sample rate in Hz.
    ///
    /// Valid rates for Logic Pro 8 analog: 50_000_000, 12_500_000,
    /// 6_250_000, 3_125_000, 1_562_500, 781_250 (same as calibration rates).
    pub sample_rate: u64,

    /// Enabled analog channel indices (0-based, 0-7 for Pro 8).
    pub analog_channels: Vec<u8>,

    /// Number of samples to capture per channel. 0 = capture until stop.
    pub duration_samples: u64,

    /// Samples per chunk (must be multiple of 256 when stats enabled).
    ///
    /// PLACEHOLDER: The actual value depends on the device and FPGA type.
    pub samples_per_chunk: u32,

    /// Whether to enable per-packet stats collection.
    pub stats_enabled: bool,
}

/// Known analog sample rates for Logic Pro 8/16.
///
/// These correspond to the sample rates present in calibration data.
pub const ANALOG_SAMPLE_RATES: &[u64] = &[
    50_000_000, 12_500_000, 6_250_000, 3_125_000, 1_562_500, 781_250,
];

/// Maximum analog channel index for Logic Pro 8.
pub const MAX_ANALOG_CHANNEL_PRO8: u8 = 7;

/// Maximum analog channel index for Logic Pro 16.
pub const MAX_ANALOG_CHANNEL_PRO16: u8 = 15;

/// Per-channel analog front-end (AFE) configuration.
///
/// The AFE settings control the analog signal path before the ADC:
/// attenuators, VGA gain, and DAC offset. These are written to FPGA
/// registers 0x0054-0x0068.
///
/// # PLACEHOLDER
///
/// The exact register bit layout is unknown. The field names are from
/// string analysis of `libgraph_server_shared.so`. The actual values
/// need USB capture from the Saleae app during analog capture setup.
#[derive(Debug, Clone, Copy)]
pub struct AfeChannelConfig {
    /// First attenuator stage (0 or 1). PLACEHOLDER.
    pub attn1: u8,
    /// Second attenuator stage (0 or 1). PLACEHOLDER.
    pub attn2: u8,
    /// Preamplifier enable (0 or 1). PLACEHOLDER.
    pub preamp: u8,
    /// Resistor ladder gain select. PLACEHOLDER.
    pub ladder: u8,
    /// Fine gain adjust. PLACEHOLDER.
    pub gain: u8,
    /// DAC offset correction code. PLACEHOLDER.
    pub dac_offset: u16,
}

impl Default for AfeChannelConfig {
    /// Default AFE configuration (no attenuation, no gain, zero offset).
    ///
    /// PLACEHOLDER: These defaults are guesses. The correct values
    /// depend on the voltage range and sample rate, and need USB
    /// capture confirmation.
    fn default() -> Self {
        Self {
            attn1: 0,
            attn2: 0,
            preamp: 0,
            ladder: 0,
            gain: 0,
            dac_offset: 0,
        }
    }
}

impl AfeChannelConfig {
    /// Pack the AFE config into a 32-bit register value.
    ///
    /// PLACEHOLDER: The bit layout is completely unknown. This packs
    /// the fields into a minimal format for now. Will need to be
    /// updated once the real format is reverse-engineered from USB
    /// captures.
    pub fn to_register_value(&self) -> u32 {
        // PLACEHOLDER packing:
        // [7:0]   = attn1 | attn2 | preamp | ladder | gain
        // [23:8]  = dac_offset
        // [31:24] = reserved
        let mut value: u32 = 0;
        value |= (self.attn1 as u32) & 0x01;
        value |= ((self.attn2 as u32) & 0x01) << 1;
        value |= ((self.preamp as u32) & 0x01) << 2;
        value |= ((self.ladder as u32) & 0x0F) << 3;
        value |= ((self.gain as u32) & 0x01) << 7;
        value |= (self.dac_offset as u32) << 8;
        value
    }
}

impl Default for AnalogCaptureConfig {
    fn default() -> Self {
        Self {
            sample_rate: 1_000_000,
            analog_channels: vec![0],
            duration_samples: 0,
            samples_per_chunk: DEFAULT_SAMPLES_PER_CHUNK,
            stats_enabled: true,
        }
    }
}

impl AnalogCaptureConfig {
    /// Validate the analog capture configuration.
    pub fn validate(&self) -> DriverResult<()> {
        if self.sample_rate == 0 {
            return Err(DriverError::InvalidCaptureConfig {
                reason: "sample rate must be > 0".into(),
            });
        }

        if self.analog_channels.is_empty() {
            return Err(DriverError::InvalidCaptureConfig {
                reason: "at least one analog channel must be enabled".into(),
            });
        }

        for &ch in &self.analog_channels {
            if ch > MAX_ANALOG_CHANNEL_PRO16 {
                return Err(DriverError::InvalidCaptureConfig {
                    reason: format!(
                        "analog channel {} out of range (max {})",
                        ch, MAX_ANALOG_CHANNEL_PRO16
                    ),
                });
            }
        }

        if self.stats_enabled
            && self.samples_per_chunk
                % (capture_data::SAMPLES_PER_PACKET_MULTIPLE_WITH_STATS as u32)
                != 0
        {
            return Err(DriverError::InvalidCaptureConfig {
                reason: format!(
                    "samples_per_chunk ({}) must be a multiple of {} when stats enabled",
                    self.samples_per_chunk,
                    capture_data::SAMPLES_PER_PACKET_MULTIPLE_WITH_STATS
                ),
            });
        }

        if self.samples_per_chunk < 64 {
            return Err(DriverError::InvalidCaptureConfig {
                reason: format!(
                    "samples_per_chunk ({}) must be >= 64",
                    self.samples_per_chunk
                ),
            });
        }

        Ok(())
    }

    /// Number of bytes of analog sample data per chunk.
    ///
    /// Each analog sample is 2 bytes (int16 LE). With N channels,
    /// each sample row is 2*N bytes.
    pub fn sample_data_bytes_per_chunk(&self) -> usize {
        let num_channels = self.analog_channels.len();
        self.samples_per_chunk as usize * 2 * num_channels
    }
}

// ---------------------------------------------------------------------------
// Capture sequence: configure FPGA
// ---------------------------------------------------------------------------

/// Configure the FPGA for a capture session.
///
/// Writes capture parameters as a batch of (addr, value) register pairs
/// via WriteRegisters (opcode 0x80), then reads them all back via
/// ReadRegisters (0x81) and verifies each value matches.
///
/// This is the Rust equivalent of `Logic2FpgaDevice::SetCaptureParameters`
/// from the original software (Ghidra 01d24ca0).
///
/// # Register sequence (from decompilation + pcap Phase 7)
///
/// The function builds a vector of up to 15 register writes:
///
/// | Reg  | Constant              | Description                          |
/// |------|-----------------------|--------------------------------------|
/// | 0x00 | FEAT_CAPTURE_INIT     | Always 0x00                          |
/// | 0x08 | FEAT_DIGITAL_CH_ENABLE_LO | Digital channel mask low byte    |
/// | 0x09 | FEAT_DIGITAL_CH_ENABLE_HI | Digital mask high (16ch only)    |
/// | 0x06 | FEAT_ANALOG_CH_ENABLE_LO  | Analog channel mask low byte    |
/// | 0x07 | FEAT_ANALOG_CH_ENABLE_HI  | Analog mask high (16ch only)    |
/// | 0x0A | FEAT_ANALOG_DECIMATION | Analog decimation factor low byte   |
/// | 0x0B | FEAT_DIGITAL_DIVIDER_LO | Digital divider low byte           |
/// | 0x0C | FEAT_DIGITAL_DIVIDER_HI | Digital divider high byte          |
/// | 0x1A | FEAT_ADC_CONFIG       | ADC config (Pro/JESD only)           |
/// | 0x1B | FEAT_ANALOG_DEC_UPPER | Upper decimation (board rev 1 only)  |
/// | 0x0D | FEAT_MIX_RATIO_A      | Mix ratio A                         |
/// | 0x0E | FEAT_MIX_RATIO_B      | Mix ratio B                         |
/// | 0x12 | FEAT_CAPTURE_CONFIG_2 | ADC selection                        |
/// | 0x13 | FEAT_CAPTURE_CONTROL  | Capture flags                        |
/// | 0x14 | FEAT_DECIMATION_FACTOR | Computed decimation (max 0xFF)      |
pub fn configure_capture(ep1: &mut Ep1CommandChannel, config: &CaptureConfig) -> DriverResult<()> {
    config.validate()?;
    config.validate_bandwidth()?;

    info!(
        sample_rate = config.sample_rate,
        digital_channels = ?config.digital_channels,
        analog_channels = ?config.analog_channels,
        samples_per_chunk = config.samples_per_chunk,
        "Configuring capture"
    );

    let reg_pairs = build_capture_register_pairs(config);

    debug!(
        num_regs = reg_pairs.len(),
        "Writing capture registers via WriteRegisters (0x80)"
    );
    for &(addr, val) in &reg_pairs {
        debug!(
            addr = format_args!("{addr:#04x}"),
            val = format_args!("{val:#04x}"),
            "reg"
        );
    }

    // Write all registers in one batch.
    ep1.write_registers(&reg_pairs)?;

    // Read back all registers and verify (matches binary behavior).
    let addrs: Vec<u8> = reg_pairs.iter().map(|&(a, _)| a).collect();
    let readback = ep1.read_registers(&addrs)?;

    for (i, &(addr, expected)) in reg_pairs.iter().enumerate() {
        let actual = readback.get(i).copied().unwrap_or(0xFF);
        if actual != expected {
            return Err(DriverError::CommandFailed {
                command: "SetCaptureParameters",
                detail: format!(
                    "register readback mismatch at r{addr:#04x}: wrote {expected:#04x}, read {actual:#04x}"
                ),
            });
        }
    }

    info!(
        "Capture configuration complete ({} registers verified)",
        reg_pairs.len()
    );
    Ok(())
}

/// Build the (addr, value) register pairs for WriteRegisters.
///
/// The order matches the decompiled `SetCaptureParameters` function,
/// which builds a `vector<uint16_t>` of `(value << 8) | addr` entries.
///
/// Pcap Phase 7 example (Logic Pro 8, all channels, 12.5 MSa/s analog+digital):
/// ```text
/// r0x00=0x00  r0x08=0xff  r0x06=0xff  r0x0a=0x20
/// r0x0b=0x10  r0x0c=0x00  r0x1a=0x00  r0x0d=0x18
/// r0x0e=0x08  r0x12=0x02  r0x13=0x00  r0x14=0x10
/// ```
pub fn build_capture_register_pairs(config: &CaptureConfig) -> Vec<(u8, u8)> {
    let mut pairs = Vec::with_capacity(15);

    // Build channel masks.
    let mut digital_ch_mask: u16 = 0;
    for &ch in &config.digital_channels {
        digital_ch_mask |= 1 << ch;
    }
    let mut analog_ch_mask: u16 = 0;
    for &ch in &config.analog_channels {
        analog_ch_mask |= 1 << ch;
    }

    let analog_count = config.analog_channels.len() as u32;
    let digital_count = config.digital_channels.len() as u32;

    // Analog decimation: base_analog_rate / sample_rate.
    // For digital-only capture, analog decimation is 0.
    let analog_decimation: u16 = if analog_count > 0 && config.base_analog_rate > 0 {
        (config.base_analog_rate / config.sample_rate.max(1)) as u16
    } else {
        0
    };

    // Digital divider: base_digital_rate / sample_rate.
    // From decompilation:
    //   Non-JESD (non-Pro): divider = rate_ratio - 1
    //   JESD (Pro): divider = rate_ratio (pcap shows 0x10 for 500MHz/12.5MSa/s
    //     mixed capture, but the exact formula for Pro path with analog
    //     interleaving needs further RE work).
    //
    // TODO: The Pro divider formula is approximate. Pcap Phase 7 shows
    // digital_divider=0x10 (16) for 12.5 MSa/s mixed capture, but
    // rate_ratio=40. The relationship is not a simple /5 division.
    // Hardware testing will reveal the correct formula.
    let rate_ratio = if config.sample_rate > 0 {
        (config.base_digital_rate / config.sample_rate) as u16
    } else {
        0
    };
    let digital_divider: u16 = if config.is_pro {
        // Pro path: approximate formula. The pcap value (0x10) doesn't
        // match a simple rate_ratio/N for the known case. Using rate_ratio
        // directly until we have more pcap data points.
        if analog_decimation <= 1 && analog_count > 0 {
            0
        } else {
            rate_ratio
        }
    } else {
        rate_ratio.saturating_sub(1)
    };

    // Mix ratios: control GPIF analog/digital bandwidth allocation.
    // For digital-only, both are 0.
    let (mix_ratio_a, mix_ratio_b): (u8, u8) = if analog_count > 0 && digital_count > 0 {
        compute_mix_ratios(
            analog_count,
            digital_count,
            analog_decimation,
            config.is_pro,
        )
    } else if analog_count > 0 {
        (0x01, 0x00)
    } else {
        (0x00, 0x00)
    };

    // ADC config (0x1A): only for Pro/JESD devices.
    let adc_config: u8 = if config.is_pro {
        match analog_decimation {
            0 | 1 => 0, // 4x interleaved or no analog
            2 => 1,     // standard
            4 => 0,     // 4x interleaved
            _ => 0,     // safe default
        }
    } else {
        0
    };

    // Upper analog decimation (board rev 1 only).
    let analog_dec_upper: u8 = if config.board_revision == 1 {
        ((analog_decimation >> 8) & 0xFF).min(1) as u8
    } else {
        0
    };

    // Capture control flags.
    // bit 0 = capture enabled (rev 2+ = always 1, rev 0/1 = 0)
    // bit 1 = ignore FPGA errors
    let capture_control: u8 = if config.board_revision >= 2 {
        0x01
    } else {
        0x00
    };

    // Decimation factor: min(0xFF, (total_rate + padding) / total_rate).
    let gpif_width_factor: u64 = if config.is_pro { 0x20 } else { 0x10 };
    let total_rate = config.sample_rate / gpif_width_factor.max(1);
    let padding: u64 = if config.is_pro {
        99_999_999
    } else {
        39_999_999
    };
    let decimation_factor: u8 = if total_rate > 0 {
        let raw = (total_rate + padding) / total_rate;
        raw.min(0xFF) as u8
    } else {
        0xFF
    };

    // Capture config 2 (0x12): ADC type selection.
    // Xilinx Spartan-6 with AD9637: index=1 -> value=0x02
    let capture_config_2: u8 = if config.board_revision <= 1 {
        0x02 // AD9637 = index 1, value = 1 << 1
    } else {
        0x01 // default for other revisions
    };

    // --- Build register pairs in decompilation order ---

    // 1. FEAT_CAPTURE_INIT (0x00) = 0x00
    pairs.push((fpga_regs::FEAT_CAPTURE_INIT, 0x00));

    // 2. FEAT_DIGITAL_CH_ENABLE_LO (0x08)
    pairs.push((
        fpga_regs::FEAT_DIGITAL_CH_ENABLE_LO,
        (digital_ch_mask & 0xFF) as u8,
    ));

    // 3. FEAT_DIGITAL_CH_ENABLE_HI (0x09) -- 16ch devices only
    if config.total_device_channels == 16 {
        pairs.push((
            fpga_regs::FEAT_DIGITAL_CH_ENABLE_HI,
            ((digital_ch_mask >> 8) & 0xFF) as u8,
        ));
    }

    // 4. FEAT_ANALOG_CH_ENABLE_LO (0x06)
    pairs.push((
        fpga_regs::FEAT_ANALOG_CH_ENABLE_LO,
        (analog_ch_mask & 0xFF) as u8,
    ));

    // 5. FEAT_ANALOG_CH_ENABLE_HI (0x07) -- 16ch devices only
    if config.total_device_channels == 16 {
        pairs.push((
            fpga_regs::FEAT_ANALOG_CH_ENABLE_HI,
            ((analog_ch_mask >> 8) & 0xFF) as u8,
        ));
    }

    // 6. FEAT_ANALOG_DECIMATION (0x0A)
    pairs.push((
        fpga_regs::FEAT_ANALOG_DECIMATION,
        (analog_decimation & 0xFF) as u8,
    ));

    // 7. FEAT_DIGITAL_DIVIDER_LO (0x0B)
    pairs.push((
        fpga_regs::FEAT_DIGITAL_DIVIDER_LO,
        (digital_divider & 0xFF) as u8,
    ));

    // 8. FEAT_DIGITAL_DIVIDER_HI (0x0C)
    pairs.push((
        fpga_regs::FEAT_DIGITAL_DIVIDER_HI,
        ((digital_divider >> 8) & 0xFF) as u8,
    ));

    // 9. FEAT_ADC_CONFIG (0x1A) -- Pro/JESD only
    if config.is_pro {
        pairs.push((fpga_regs::FEAT_ADC_CONFIG, adc_config));
    }

    // 10. FEAT_ANALOG_DEC_UPPER (0x1B) -- board rev 1 only
    if config.board_revision == 1 {
        pairs.push((fpga_regs::FEAT_ANALOG_DEC_UPPER, analog_dec_upper));
    }

    // 11. FEAT_MIX_RATIO_A (0x0D)
    pairs.push((fpga_regs::FEAT_MIX_RATIO_A, mix_ratio_a));

    // 12. FEAT_MIX_RATIO_B (0x0E)
    pairs.push((fpga_regs::FEAT_MIX_RATIO_B, mix_ratio_b));

    // 13. FEAT_CAPTURE_CONFIG_2 (0x12)
    pairs.push((fpga_regs::FEAT_CAPTURE_CONFIG_2, capture_config_2));

    // 14. FEAT_CAPTURE_CONTROL (0x13)
    pairs.push((fpga_regs::FEAT_CAPTURE_CONTROL, capture_control));

    // 15. FEAT_DECIMATION_FACTOR (0x14)
    pairs.push((fpga_regs::FEAT_DECIMATION_FACTOR, decimation_factor));

    pairs
}

/// Compute analog/digital mix ratios for GPIF interleaving.
///
/// Approximation of `AnalogGpifWidthValidation` (FUN_01d19fa0).
/// The exact algorithm is complex; this provides reasonable values
/// based on pcap observations.
///
/// Known pcap values for Logic Pro 8 (all channels, 12.5 MSa/s):
///   mix_ratio_a = 0x18 (24), mix_ratio_b = 0x08 (8)
fn compute_mix_ratios(
    analog_count: u32,
    digital_count: u32,
    analog_decimation: u16,
    is_pro: bool,
) -> (u8, u8) {
    if !is_pro {
        // Non-Pro (16-bit GPIF): simpler ratio
        let total = (analog_count * 2 + digital_count).max(1);
        let a = ((analog_count * 2 * 0x20) / total).min(0xFF) as u8;
        let b = ((digital_count * 0x20) / total).min(0xFF) as u8;
        return (a, b);
    }

    // Pro (32-bit GPIF): compute based on byte ratios.
    let analog_bytes = analog_count * 2; // 12-bit ADC, packed 16-bit
    let digital_bytes = digital_count.div_ceil(8); // 1 bit per channel

    if analog_decimation <= 1 {
        let a = (analog_bytes * 8).min(0xFF) as u8;
        let b = (digital_bytes * 8).min(0xFF) as u8;
        return (a, b);
    }

    // Decimated: scale by decimation factor
    let eff_analog = analog_bytes * 8 / analog_decimation as u32;
    let eff_digital = digital_bytes * 8;
    let a = eff_analog.clamp(1, 0xFF) as u8;
    let b = eff_digital.clamp(1, 0xFF) as u8;
    (a, b)
}

// ---------------------------------------------------------------------------
// Capture stream
// ---------------------------------------------------------------------------

/// Handle for an active capture stream.
///
/// Created by [`start_digital_capture`]. Reads data chunks from EP2 IN
/// and provides parsed `DataChunk`s. Must be [`stop`](CaptureStream::stop)ped
/// when done.
///
/// # Limitations (known gaps vs original Saleae implementation)
///
/// The original uses a two-thread model with 128 outstanding async USB
/// transfers via libusb, managed by `ReadStream_QueueBuffersThread` and
/// `BulkReadStream_Read`. See `re/notes/streaming-thread-decompilation.md`.
///
/// This implementation uses synchronous single-buffered reads. At high
/// sample rates, this will cause FPGA FIFO overflow because no USB
/// transfer is pending while we parse data. To fix this, the capture
/// stream should use `nusb::transfer::Queue` with `QUEUE_DEPTH` (128)
/// outstanding transfers of `TransferConfig::transfer_size` bytes each.
///
/// Missing edge cases from the original:
/// - No transfer queue (single-buffered reads -> FIFO overflow risk)
/// - No transfer size computation (hardcoded 16 KiB vs computed size)
/// - No stall recovery (original clears halt and retries)
/// - No timeout classification (timeout vs stale-data-timeout)
/// - No short transfer detection (original checks actual <= expected)
/// - No graceful queue depth reduction on submit failure
/// - No buffer reuse (allocates new Vec per read)
pub struct CaptureStream {
    /// Read buffer for EP2 IN bulk transfers.
    read_buf: Vec<u8>,
    /// Whether a start packet has been received for each channel.
    channel_started: [bool; capture_data::MAX_DIGITAL_CHANNELS],
    /// Total samples read so far.
    total_samples: u64,
    /// Target sample count (0 = unlimited).
    target_samples: u64,
    /// Samples per chunk.
    samples_per_chunk: u32,
    /// Sample data bytes per chunk.
    sample_data_bytes: usize,
    /// Whether stats are enabled.
    stats_enabled: bool,
    /// Whether the stream has been stopped.
    stopped: bool,
}

impl CaptureStream {
    fn new(config: &CaptureConfig) -> Self {
        Self {
            read_buf: vec![0u8; DEFAULT_TRANSFER_SIZE],
            channel_started: [false; capture_data::MAX_DIGITAL_CHANNELS],
            total_samples: 0,
            target_samples: config.duration_samples,
            samples_per_chunk: config.samples_per_chunk,
            sample_data_bytes: config.sample_data_bytes_per_chunk(),
            stats_enabled: config.stats_enabled,
            stopped: false,
        }
    }

    /// Read the next data chunk from EP2 IN.
    ///
    /// Returns `None` when the target sample count is reached or the
    /// stream has been stopped.
    ///
    /// # Arguments
    ///
    /// - `transport`: USB transport for EP2 IN reads.
    pub fn read_chunk(
        &mut self,
        transport: &dyn saleae_usb::transport::UsbTransport,
    ) -> DriverResult<Option<DataChunk>> {
        if self.stopped {
            return Ok(None);
        }

        if self.target_samples > 0 && self.total_samples >= self.target_samples {
            return Ok(None);
        }

        // Read from EP2 IN.
        let n = transport
            .bulk_transfer_in(EP2_CAPTURE_IN, &mut self.read_buf)
            .map_err(DriverError::Usb)?;

        if n == 0 {
            debug!("EP2 IN: zero-length read");
            return Ok(None);
        }

        let data = &self.read_buf[..n];

        // Parse chunk.
        let (chunk, _consumed) =
            capture_data::parse_data_chunk(data, self.sample_data_bytes, self.stats_enabled)
                .map_err(|e| DriverError::CommandFailed {
                    command: "ParseDataChunk",
                    detail: e.to_string(),
                })?;

        // Track start packets.
        let ch = chunk.header.channel as usize;
        if chunk.header.is_start {
            if ch < self.channel_started.len() {
                self.channel_started[ch] = true;
                debug!(
                    channel = ch,
                    config_id = chunk.header.config_id,
                    "Got start packet"
                );
            }
        } else if ch < self.channel_started.len() && !self.channel_started[ch] {
            warn!(channel = ch, "Received digital data without start packet");
        }

        // Count samples.
        if !chunk.header.is_start {
            let sample_count = chunk.sample_data.len() as u64 * 8
                / (self.samples_per_chunk as u64 / self.sample_data_bytes as u64).max(1);
            self.total_samples += sample_count;
        }

        Ok(Some(chunk))
    }

    /// Stop the capture stream.
    ///
    /// Sends StopDataPath (0x86) and STOP_STREAM (0x02) commands,
    /// then drains any remaining EP2 data.
    pub fn stop(&mut self, ep1: &mut Ep1CommandChannel) -> DriverResult<()> {
        if self.stopped {
            return Ok(());
        }

        info!(
            total_samples = self.total_samples,
            "Stopping capture stream"
        );

        // Step 1: Stop data path (opcode 0x86).
        debug!("Sending StopDataPath (0x86)");
        ep1.stop_data_path()?;

        // Step 2: Stop stream (opcode 0x02).
        debug!("Sending StopStream (0x02)");
        ep1.stop_stream()?;

        self.stopped = true;
        info!("Capture stream stopped");
        Ok(())
    }

    /// Whether the stream has been stopped.
    pub fn is_stopped(&self) -> bool {
        self.stopped
    }

    /// Total samples read so far.
    pub fn total_samples(&self) -> u64 {
        self.total_samples
    }
}

// ---------------------------------------------------------------------------
// Start capture
// ---------------------------------------------------------------------------

/// Start a digital capture session.
///
/// Configures the FPGA, sends the start command, and returns a
/// [`CaptureStream`] for reading data.
///
/// # Arguments
///
/// - `ep1`: Initialized EP1 command channel.
/// - `config`: Capture configuration.
///
/// # Errors
///
/// Returns an error if configuration validation fails, FPGA register
/// writes fail, or the start command fails.
pub fn start_digital_capture(
    ep1: &mut Ep1CommandChannel,
    config: &CaptureConfig,
) -> DriverResult<CaptureStream> {
    config.validate()?;

    info!(
        sample_rate = config.sample_rate,
        channels = ?config.digital_channels,
        duration_samples = config.duration_samples,
        "Starting digital capture"
    );

    // Step 1: Configure FPGA.
    configure_capture(ep1, config)?;

    // Step 2: Send START_READ command (opcode 0x01).
    debug!("Sending StartSampling (0x01)");
    ep1.start_sampling()?;

    info!("Digital capture started");
    Ok(CaptureStream::new(config))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use saleae_proto::capture_data::{ChunkHeader, StartPacketFields};
    use saleae_proto::device::endpoint as ep;
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

    /// Create a mock channel with seed=0 (identity XOR).
    fn mock_channel(mock: &std::sync::Arc<MockTransport>) -> Ep1CommandChannel {
        let transport = Box::new(MockTransportWrapper(mock.clone()));
        let mut channel = Ep1CommandChannel::new(transport, EncryptionMode::XorAll);
        channel.encryption.reseed(0);
        channel.initialized = true;
        channel
    }

    /// Queue mock responses for configure_capture.
    ///
    /// The new configure_capture does:
    /// 1. WriteRegisters (opcode 0x80): 1 OUT + 1 IN (1-byte ACK)
    /// 2. ReadRegisters (opcode 0x81): 1 OUT + 1 IN (N-byte readback)
    ///
    /// The readback values must match the written values for verification
    /// to pass. We build the register pairs from the config and return
    /// the expected values as the readback response.
    fn queue_configure_acks(mock: &MockTransport, config: &CaptureConfig) {
        // WriteRegisters ACK (1 byte)
        mock.push_bulk_in_response(ep::BULK_IN, vec![0x00]);
        // ReadRegisters readback: return the values that were "written"
        let pairs = build_capture_register_pairs(config);
        let readback: Vec<u8> = pairs.iter().map(|&(_, v)| v).collect();
        mock.push_bulk_in_response(ep::BULK_IN, readback);
    }

    // -------------------------------------------------------------------
    // CaptureConfig validation tests
    // -------------------------------------------------------------------

    #[test]
    fn config_default_is_valid() {
        CaptureConfig::default().validate().unwrap();
    }

    #[test]
    fn config_zero_sample_rate_invalid() {
        let config = CaptureConfig {
            sample_rate: 0,
            ..CaptureConfig::default()
        };
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("sample rate"));
    }

    #[test]
    fn config_no_channels_invalid() {
        let config = CaptureConfig {
            digital_channels: vec![],
            ..CaptureConfig::default()
        };
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("channel"));
    }

    #[test]
    fn config_channel_out_of_range() {
        let config = CaptureConfig {
            digital_channels: vec![0, 16],
            ..CaptureConfig::default()
        };
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("16"));
    }

    #[test]
    fn config_misaligned_samples_per_chunk() {
        let config = CaptureConfig {
            samples_per_chunk: 100,
            ..CaptureConfig::default()
        };
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("multiple of 256"));
    }

    #[test]
    fn config_small_samples_per_chunk() {
        let config = CaptureConfig {
            samples_per_chunk: 32,
            stats_enabled: false,
            ..CaptureConfig::default()
        };
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains(">= 64"));
    }

    #[test]
    fn config_valid_custom() {
        let config = CaptureConfig {
            sample_rate: 10_000_000,
            digital_channels: vec![0, 1],
            duration_samples: 1_000_000,
            voltage_threshold: 2.5,
            stats_enabled: true,
            samples_per_chunk: 512,
            ..CaptureConfig::default()
        };
        config.validate().unwrap();
    }

    // -------------------------------------------------------------------
    // configure_capture tests
    // -------------------------------------------------------------------

    #[test]
    fn configure_capture_sends_write_and_readback() {
        let mock = std::sync::Arc::new(MockTransport::new());
        let config = CaptureConfig {
            sample_rate: 1_000_000,
            digital_channels: vec![0, 1],
            ..CaptureConfig::default()
        };

        queue_configure_acks(&mock, &config);

        let mut channel = mock_channel(&mock);
        configure_capture(&mut channel, &config).unwrap();

        let ops = mock.operations();
        let out_count = ops
            .iter()
            .filter(|op| matches!(op, UsbOperation::BulkOut { .. }))
            .count();
        // 1 WriteRegisters OUT + 1 ReadRegisters OUT = 2
        assert_eq!(out_count, 2, "expected 2 OUT transfers (write + readback)");
    }

    #[test]
    fn configure_capture_all_channels() {
        let mock = std::sync::Arc::new(MockTransport::new());
        let config = CaptureConfig {
            sample_rate: 1_000_000,
            digital_channels: vec![0, 1, 2, 3, 4, 5, 6, 7],
            ..CaptureConfig::default()
        };

        queue_configure_acks(&mock, &config);

        let mut channel = mock_channel(&mock);
        configure_capture(&mut channel, &config).unwrap();

        let ops = mock.operations();
        let out_count = ops
            .iter()
            .filter(|op| matches!(op, UsbOperation::BulkOut { .. }))
            .count();
        assert_eq!(out_count, 2, "expected 2 OUT transfers for all channels");
    }

    #[test]
    fn configure_capture_rejects_invalid_config() {
        let mock = std::sync::Arc::new(MockTransport::new());
        let config = CaptureConfig {
            sample_rate: 0, // invalid
            ..CaptureConfig::default()
        };

        let mut channel = mock_channel(&mock);
        let err = configure_capture(&mut channel, &config).unwrap_err();
        assert!(err.to_string().contains("sample rate"));
    }

    // -------------------------------------------------------------------
    // start_digital_capture tests
    // -------------------------------------------------------------------

    #[test]
    fn start_capture_sends_configure_then_start() {
        let mock = std::sync::Arc::new(MockTransport::new());
        let config = CaptureConfig {
            digital_channels: vec![0, 1],
            ..CaptureConfig::default()
        };

        // Queue configure responses + StartSampling response
        queue_configure_acks(&mock, &config);
        mock.push_bulk_in_response(ep::BULK_IN, vec![0x00, 0x00]);

        let mut channel = mock_channel(&mock);
        let stream = start_digital_capture(&mut channel, &config).unwrap();
        assert!(!stream.is_stopped());
        assert_eq!(stream.total_samples(), 0);
    }

    #[test]
    fn start_capture_returns_stream() {
        let mock = std::sync::Arc::new(MockTransport::new());
        let config = CaptureConfig::default();

        // Queue configure responses + start response
        queue_configure_acks(&mock, &config);
        mock.push_bulk_in_response(ep::BULK_IN, vec![0x00, 0x00]);

        let mut channel = mock_channel(&mock);
        let stream = start_digital_capture(&mut channel, &config).unwrap();
        assert!(!stream.is_stopped());
    }

    // -------------------------------------------------------------------
    // CaptureStream tests
    // -------------------------------------------------------------------

    #[test]
    fn stream_read_chunk_start_packet() {
        let mock = std::sync::Arc::new(MockTransport::new());

        let config = CaptureConfig::default();
        let mut stream = CaptureStream::new(&config);

        // Build a start packet for EP2 IN.
        let header = ChunkHeader {
            channel: 0,
            software: false,
            is_start: true,
            config_id: 1,
        };
        let start = StartPacketFields {
            timestamp: 100,
            time: 200,
        };
        let mut packet = Vec::new();
        packet.extend_from_slice(&header.to_bytes());
        packet.extend_from_slice(&start.to_bytes());

        mock.push_bulk_in_response(EP2_CAPTURE_IN, packet);

        let chunk = stream.read_chunk(mock.as_ref()).unwrap().unwrap();
        assert!(chunk.header.is_start);
        assert_eq!(chunk.header.channel, 0);
        assert_eq!(chunk.start.unwrap().timestamp, 100);
    }

    #[test]
    fn stream_read_chunk_data() {
        let mock = std::sync::Arc::new(MockTransport::new());

        let config = CaptureConfig {
            stats_enabled: false,
            ..CaptureConfig::default()
        };
        let mut stream = CaptureStream::new(&config);

        // Build a data packet for EP2 IN.
        let header = ChunkHeader {
            channel: 0,
            software: false,
            is_start: false,
            config_id: 1,
        };
        let sample_data = vec![0xFF; 32]; // 32 bytes of sample data
        let mut packet = Vec::new();
        packet.extend_from_slice(&header.to_bytes());
        packet.extend_from_slice(&sample_data);

        mock.push_bulk_in_response(EP2_CAPTURE_IN, packet);

        // Mark channel as started to avoid warning.
        stream.channel_started[0] = true;

        let chunk = stream.read_chunk(mock.as_ref()).unwrap().unwrap();
        assert!(!chunk.header.is_start);
        assert!(!chunk.sample_data.is_empty());
    }

    #[test]
    fn stream_stop_sends_commands() {
        let mock = std::sync::Arc::new(MockTransport::new());

        let config = CaptureConfig::default();
        let mut stream = CaptureStream::new(&config);

        // Queue responses for stop sequence:
        // StopDataPath (0x86) may or may not get a response.
        // We need to set up the mock channel to handle this.

        // For stop, we need the EP1 command channel.
        // StopDataPath sends 0x86 and tries to read response.
        mock.push_bulk_in_response(ep::BULK_IN, vec![0x00]); // StopDataPath response
                                                             // StopStream sends 0x02 and reads response.
        mock.push_bulk_in_response(ep::BULK_IN, vec![0x00]); // StopStream response

        let mut channel = mock_channel(&mock);
        stream.stop(&mut channel).unwrap();
        assert!(stream.is_stopped());

        // Verify commands were sent.
        let ops = mock.operations();
        let out_ops: Vec<_> = ops
            .iter()
            .filter_map(|op| match op {
                UsbOperation::BulkOut { data, .. } => Some(data.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(
            out_ops.len(),
            2,
            "should send 2 commands (stop_data_path + stop_stream)"
        );

        // First command: StopDataPath (0x86)
        assert_eq!(out_ops[0][1], 0x86, "first stop command should be 0x86");
        // Second command: StopStream (0x02)
        assert_eq!(out_ops[1][1], 0x02, "second stop command should be 0x02");
    }

    #[test]
    fn stream_stop_idempotent() {
        let mock = std::sync::Arc::new(MockTransport::new());

        let config = CaptureConfig::default();
        let mut stream = CaptureStream::new(&config);

        // Queue stop responses.
        mock.push_bulk_in_response(ep::BULK_IN, vec![0x00]);
        mock.push_bulk_in_response(ep::BULK_IN, vec![0x00]);

        let mut channel = mock_channel(&mock);
        stream.stop(&mut channel).unwrap();
        // Calling stop again should be a no-op.
        stream.stop(&mut channel).unwrap();
        assert!(stream.is_stopped());
    }

    #[test]
    fn stream_read_after_stop_returns_none() {
        let mock = std::sync::Arc::new(MockTransport::new());

        let config = CaptureConfig::default();
        let mut stream = CaptureStream::new(&config);
        stream.stopped = true;

        let result = stream.read_chunk(mock.as_ref()).unwrap();
        assert!(result.is_none());
    }

    // -------------------------------------------------------------------
    // build_capture_register_pairs tests
    // -------------------------------------------------------------------

    #[test]
    fn register_pairs_contain_expected_registers() {
        let config = CaptureConfig::default();
        let pairs = build_capture_register_pairs(&config);

        // Pro 8 (rev 1, 8ch) should produce 13 registers:
        // INIT, DIG_LO, ANA_LO, ANA_DEC, DIG_DIV_LO, DIG_DIV_HI,
        // ADC_CONFIG (pro), ANA_DEC_UPPER (rev1), MIX_A, MIX_B,
        // CFG2, CTRL, DEC_FACTOR
        assert_eq!(pairs.len(), 13, "expected 13 registers for Pro 8 rev 1");

        // First register is always FEAT_CAPTURE_INIT = 0x00
        assert_eq!(pairs[0], (fpga_regs::FEAT_CAPTURE_INIT, 0x00));
    }

    #[test]
    fn register_pairs_16ch_includes_high_bytes() {
        let config = CaptureConfig {
            total_device_channels: 16,
            ..CaptureConfig::default()
        };
        let pairs = build_capture_register_pairs(&config);

        // 16ch adds DIGITAL_CH_ENABLE_HI and ANALOG_CH_ENABLE_HI = +2
        assert_eq!(pairs.len(), 15, "expected 15 registers for 16ch Pro");

        let addrs: Vec<u8> = pairs.iter().map(|&(a, _)| a).collect();
        assert!(addrs.contains(&fpga_regs::FEAT_DIGITAL_CH_ENABLE_HI));
        assert!(addrs.contains(&fpga_regs::FEAT_ANALOG_CH_ENABLE_HI));
    }

    #[test]
    fn register_pairs_digital_mask() {
        let config = CaptureConfig {
            digital_channels: vec![0, 2, 5],
            ..CaptureConfig::default()
        };
        let pairs = build_capture_register_pairs(&config);

        // Find FEAT_DIGITAL_CH_ENABLE_LO
        let dig_lo = pairs
            .iter()
            .find(|&&(a, _)| a == fpga_regs::FEAT_DIGITAL_CH_ENABLE_LO);
        assert_eq!(dig_lo.unwrap().1, 0b0010_0101); // bits 0, 2, 5
    }

    #[test]
    fn register_pairs_pcap_validation() {
        // Validate against pcap Phase 7: Logic Pro 8, all 8 channels,
        // 12.5 MSa/s mixed. Expected register values from pcap.
        let config = CaptureConfig {
            sample_rate: 12_500_000,
            digital_channels: vec![0, 1, 2, 3, 4, 5, 6, 7],
            analog_channels: vec![0, 1, 2, 3, 4, 5, 6, 7],
            total_device_channels: 8,
            board_revision: 1,
            is_pro: true,
            base_digital_rate: 500_000_000,
            base_analog_rate: 100_000_000,
            ..CaptureConfig::default()
        };
        let pairs = build_capture_register_pairs(&config);

        // Check key register values from pcap
        let find = |addr: u8| pairs.iter().find(|&&(a, _)| a == addr).map(|&(_, v)| v);
        assert_eq!(find(0x00), Some(0x00), "CAPTURE_INIT");
        assert_eq!(find(0x08), Some(0xff), "DIGITAL_CH_ENABLE_LO");
        assert_eq!(find(0x06), Some(0xff), "ANALOG_CH_ENABLE_LO");
        assert_eq!(find(0x12), Some(0x02), "CAPTURE_CONFIG_2 (AD9637)");
    }

    // -------------------------------------------------------------------
    // Channel group count tests
    // -------------------------------------------------------------------

    #[test]
    fn channel_group_count_2() {
        let config = CaptureConfig {
            digital_channels: vec![0, 1],
            ..CaptureConfig::default()
        };
        assert_eq!(config.channel_group_count(), 2);
    }

    #[test]
    fn channel_group_count_4() {
        let config = CaptureConfig {
            digital_channels: vec![0, 1, 2, 3],
            ..CaptureConfig::default()
        };
        assert_eq!(config.channel_group_count(), 4);
    }

    #[test]
    fn channel_group_count_with_ch2() {
        // If any channel >= 2, group count is 4.
        let config = CaptureConfig {
            digital_channels: vec![0, 2],
            ..CaptureConfig::default()
        };
        assert_eq!(config.channel_group_count(), 4);
    }

    // -------------------------------------------------------------------
    // AnalogCaptureConfig tests
    // -------------------------------------------------------------------

    #[test]
    fn analog_config_default_is_valid() {
        AnalogCaptureConfig::default().validate().unwrap();
    }

    #[test]
    fn analog_config_zero_sample_rate_invalid() {
        let config = AnalogCaptureConfig {
            sample_rate: 0,
            ..AnalogCaptureConfig::default()
        };
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("sample rate"));
    }

    #[test]
    fn analog_config_no_channels_invalid() {
        let config = AnalogCaptureConfig {
            analog_channels: vec![],
            ..AnalogCaptureConfig::default()
        };
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("channel"));
    }

    #[test]
    fn analog_config_channel_out_of_range() {
        let config = AnalogCaptureConfig {
            analog_channels: vec![0, 16],
            ..AnalogCaptureConfig::default()
        };
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("16"));
    }

    #[test]
    fn analog_config_misaligned_samples_per_chunk() {
        let config = AnalogCaptureConfig {
            samples_per_chunk: 100,
            ..AnalogCaptureConfig::default()
        };
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("multiple of 256"));
    }

    #[test]
    fn analog_config_valid_custom() {
        let config = AnalogCaptureConfig {
            sample_rate: 50_000_000,
            analog_channels: vec![0, 1, 2, 3],
            duration_samples: 1_000_000,
            samples_per_chunk: 512,
            stats_enabled: true,
        };
        config.validate().unwrap();
    }

    #[test]
    fn analog_sample_data_bytes_per_chunk() {
        let config = AnalogCaptureConfig {
            analog_channels: vec![0, 1],
            samples_per_chunk: 256,
            ..AnalogCaptureConfig::default()
        };
        // 256 samples * 2 bytes/sample * 2 channels = 1024
        assert_eq!(config.sample_data_bytes_per_chunk(), 1024);
    }

    #[test]
    fn analog_sample_data_bytes_single_channel() {
        let config = AnalogCaptureConfig {
            analog_channels: vec![0],
            samples_per_chunk: 256,
            ..AnalogCaptureConfig::default()
        };
        // 256 samples * 2 bytes/sample * 1 channel = 512
        assert_eq!(config.sample_data_bytes_per_chunk(), 512);
    }

    #[test]
    fn analog_sample_data_bytes_eight_channels() {
        let config = AnalogCaptureConfig {
            analog_channels: vec![0, 1, 2, 3, 4, 5, 6, 7],
            samples_per_chunk: 256,
            ..AnalogCaptureConfig::default()
        };
        // 256 samples * 2 bytes/sample * 8 channels = 4096
        assert_eq!(config.sample_data_bytes_per_chunk(), 4096);
    }

    // -------------------------------------------------------------------
    // AfeChannelConfig tests
    // -------------------------------------------------------------------

    #[test]
    fn afe_default_register_is_zero() {
        let afe = AfeChannelConfig::default();
        assert_eq!(afe.to_register_value(), 0);
    }

    #[test]
    fn afe_register_packing() {
        let afe = AfeChannelConfig {
            attn1: 1,
            attn2: 1,
            preamp: 1,
            ladder: 0x0A,
            gain: 1,
            dac_offset: 0x1234,
        };
        let val = afe.to_register_value();
        // bit 0 = attn1 = 1
        assert_eq!(val & 0x01, 1);
        // bit 1 = attn2 = 1
        assert_eq!((val >> 1) & 0x01, 1);
        // bit 2 = preamp = 1
        assert_eq!((val >> 2) & 0x01, 1);
        // bits 6:3 = ladder = 0x0A
        assert_eq!((val >> 3) & 0x0F, 0x0A);
        // bit 7 = gain = 1
        assert_eq!((val >> 7) & 0x01, 1);
        // bits 23:8 = dac_offset = 0x1234
        assert_eq!((val >> 8) & 0xFFFF, 0x1234);
    }

    // -------------------------------------------------------------------
    // Bandwidth validation tests
    // -------------------------------------------------------------------

    #[test]
    fn bandwidth_valid_low_rate_usb3() {
        let config = CaptureConfig {
            sample_rate: 1_000_000,
            digital_channels: vec![0, 1, 2, 3],
            analog_channels: vec![],
            samples_per_chunk: 256,
            usb_speed: UsbSpeed::Super,
            fpga_type: FpgaType::Pro,
            ..CaptureConfig::default()
        };
        config.validate_bandwidth().unwrap();
    }

    #[test]
    fn bandwidth_valid_max_digital_usb3() {
        // 8 digital channels at 25 MHz = 200 MB/s < 375 MB/s
        let config = CaptureConfig {
            sample_rate: 25_000_000,
            digital_channels: vec![0, 1, 2, 3, 4, 5, 6, 7],
            analog_channels: vec![],
            samples_per_chunk: 256,
            usb_speed: UsbSpeed::Super,
            fpga_type: FpgaType::Pro,
            ..CaptureConfig::default()
        };
        config.validate_bandwidth().unwrap();
    }

    #[test]
    fn bandwidth_exceeds_usb3() {
        // 8 digital + 8 analog at 50 MHz:
        // digital: 50M * 8 * 1 = 400 MB/s
        // analog:  50M * 8 * 2 = 800 MB/s
        // total:   1200 MB/s > 375 MB/s
        let config = CaptureConfig {
            sample_rate: 50_000_000,
            digital_channels: vec![0, 1, 2, 3, 4, 5, 6, 7],
            analog_channels: vec![0, 1, 2, 3, 4, 5, 6, 7],
            samples_per_chunk: 256,
            usb_speed: UsbSpeed::Super,
            fpga_type: FpgaType::Pro,
            ..CaptureConfig::default()
        };
        let err = config.validate_bandwidth().unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("bandwidth"), "got: {msg}");
        assert!(msg.contains("375000000"), "got: {msg}");
    }

    #[test]
    fn bandwidth_exceeds_usb2() {
        // 4 digital at 10 MHz = 40 MB/s > 37.5 MB/s
        let config = CaptureConfig {
            sample_rate: 10_000_000,
            digital_channels: vec![0, 1, 2, 3],
            analog_channels: vec![],
            samples_per_chunk: 256,
            usb_speed: UsbSpeed::High,
            fpga_type: FpgaType::Pro,
            ..CaptureConfig::default()
        };
        let err = config.validate_bandwidth().unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("37500000"), "got: {msg}");
    }

    #[test]
    fn bandwidth_valid_usb2() {
        // 2 digital at 10 MHz = 20 MB/s < 37.5 MB/s
        let config = CaptureConfig {
            sample_rate: 10_000_000,
            digital_channels: vec![0, 1],
            analog_channels: vec![],
            samples_per_chunk: 256,
            usb_speed: UsbSpeed::High,
            fpga_type: FpgaType::Pro,
            ..CaptureConfig::default()
        };
        config.validate_bandwidth().unwrap();
    }

    #[test]
    fn bandwidth_rejects_low_usb_speed() {
        let config = CaptureConfig {
            sample_rate: 1_000_000,
            digital_channels: vec![0],
            analog_channels: vec![],
            samples_per_chunk: 256,
            usb_speed: UsbSpeed::Full,
            fpga_type: FpgaType::Pro,
            ..CaptureConfig::default()
        };
        let err = config.validate_bandwidth().unwrap_err();
        assert!(err.to_string().contains("too slow"), "{}", err);
    }

    #[test]
    fn bandwidth_analog_only() {
        // 4 analog at 10 MHz = 80 MB/s < 375 MB/s
        let config = CaptureConfig {
            sample_rate: 10_000_000,
            digital_channels: vec![0], // need at least one for validate()
            analog_channels: vec![0, 1, 2, 3],
            samples_per_chunk: 256,
            usb_speed: UsbSpeed::Super,
            fpga_type: FpgaType::Pro,
            ..CaptureConfig::default()
        };
        config.validate_bandwidth().unwrap();
    }

    #[test]
    fn bandwidth_samples_per_chunk_below_xilinx_min() {
        let config = CaptureConfig {
            sample_rate: 1_000_000,
            digital_channels: vec![0],
            analog_channels: vec![],
            samples_per_chunk: 128, // below Xilinx min of 256
            usb_speed: UsbSpeed::Super,
            fpga_type: FpgaType::Pro,
            ..CaptureConfig::default()
        };
        let err = config.validate_bandwidth().unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("256"), "got: {msg}");
        assert!(msg.contains("7680"), "got: {msg}");
    }

    #[test]
    fn bandwidth_samples_per_chunk_above_xilinx_max() {
        let config = CaptureConfig {
            sample_rate: 1_000_000,
            digital_channels: vec![0],
            analog_channels: vec![],
            samples_per_chunk: 8192, // above Xilinx max of 7680
            usb_speed: UsbSpeed::Super,
            fpga_type: FpgaType::Pro,
            ..CaptureConfig::default()
        };
        let err = config.validate_bandwidth().unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("7680"), "got: {msg}");
    }

    #[test]
    fn bandwidth_samples_per_chunk_altera_range() {
        // Altera range: 64..=6016
        let config_ok = CaptureConfig {
            sample_rate: 1_000_000,
            digital_channels: vec![0],
            analog_channels: vec![],
            samples_per_chunk: 64,
            usb_speed: UsbSpeed::Super,
            fpga_type: FpgaType::Mso,
            ..CaptureConfig::default()
        };
        config_ok.validate_bandwidth().unwrap();

        let config_ok_max = CaptureConfig {
            samples_per_chunk: 6016,
            fpga_type: FpgaType::Mso,
            ..config_ok.clone()
        };
        config_ok_max.validate_bandwidth().unwrap();

        let config_too_big = CaptureConfig {
            samples_per_chunk: 6017,
            fpga_type: FpgaType::Mso,
            ..config_ok.clone()
        };
        let err = config_too_big.validate_bandwidth().unwrap_err();
        assert!(err.to_string().contains("6016"), "{}", err);
    }

    #[test]
    fn bandwidth_mixed_digital_analog() {
        // 4 digital + 2 analog at 25 MHz:
        // digital: 25M * 4 * 1 = 100 MB/s
        // analog:  25M * 2 * 2 = 100 MB/s
        // total:   200 MB/s < 375 MB/s
        let config = CaptureConfig {
            sample_rate: 25_000_000,
            digital_channels: vec![0, 1, 2, 3],
            analog_channels: vec![0, 1],
            samples_per_chunk: 256,
            usb_speed: UsbSpeed::Super,
            fpga_type: FpgaType::Pro,
            ..CaptureConfig::default()
        };
        config.validate_bandwidth().unwrap();
    }

    #[test]
    fn bandwidth_at_exact_usb3_limit() {
        // Exactly at 375 MB/s should pass.
        // 375 digital channels at 1 MHz = 375 MB/s... not realistic.
        // Instead: 1 channel at 375 MHz = 375 MB/s.
        // We can't do 375 MHz but let's do a calculation that lands exactly.
        // 3 digital channels at 125 MHz = 375 MB/s
        let config = CaptureConfig {
            sample_rate: 125_000_000,
            digital_channels: vec![0, 1, 2],
            analog_channels: vec![],
            samples_per_chunk: 256,
            usb_speed: UsbSpeed::Super,
            fpga_type: FpgaType::Pro,
            ..CaptureConfig::default()
        };
        config.validate_bandwidth().unwrap();
    }

    #[test]
    fn bandwidth_just_over_usb3_limit() {
        // 3 digital + 1 analog at 125 MHz:
        // digital: 125M * 3 * 1 = 375 MB/s
        // analog:  125M * 1 * 2 = 250 MB/s
        // total:   625 MB/s > 375 MB/s
        let config = CaptureConfig {
            sample_rate: 125_000_000,
            digital_channels: vec![0, 1, 2],
            analog_channels: vec![0],
            samples_per_chunk: 256,
            usb_speed: UsbSpeed::Super,
            fpga_type: FpgaType::Pro,
            ..CaptureConfig::default()
        };
        let err = config.validate_bandwidth().unwrap_err();
        assert!(err.to_string().contains("bandwidth"), "{}", err);
    }
}
