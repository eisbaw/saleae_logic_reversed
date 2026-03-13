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
use saleae_proto::device::{endpoint, DeviceFeatureClass, UsbSpeed};
use saleae_proto::fpga_regs;
use tracing::{debug, info, warn};

use crate::ep1::Ep1CommandChannel;
use crate::error::{DriverError, DriverResult};
use crate::transfer_queue::TransferConfig;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// EP2 IN endpoint for capture data.
pub const EP2_CAPTURE_IN: u8 = endpoint::CAPTURE_IN;

/// Default samples per chunk (256 = minimum for Xilinx with stats enabled).
///
/// The Saleae firmware requires `samples_per_chunk` to be a multiple of 256
/// when per-packet stats collection is enabled on Xilinx Spartan-6 FPGAs.
/// Higher values reduce per-chunk overhead but increase latency; 256 is the
/// safe default for all supported devices and sample rates.
pub const DEFAULT_SAMPLES_PER_CHUNK: u32 = 256;

/// Analog ADC bytes per sample used for bandwidth calculation.
///
/// The Logic Pro's AD9637 ADC produces 12-bit samples packed into 16-bit
/// (2-byte) words on the USB bus.
const ADC_BYTES_PER_SAMPLE: u8 = 2;

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
    /// Device feature class. Determines samples-per-chunk limits.
    pub feature_class: DeviceFeatureClass,
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
            feature_class: DeviceFeatureClass::Pro,
        }
    }
}

impl CaptureConfig {
    /// Create a `CaptureConfig` for Logic Pro 8 with the given capture parameters.
    ///
    /// Fills in the Pro 8 device constants:
    /// - `total_device_channels`: 8
    /// - `board_revision`: 1 (Xilinx Spartan-6)
    /// - `is_pro`: true
    /// - `base_digital_rate`: 500 MHz
    /// - `base_analog_rate`: 100 MHz
    /// - `usb_speed`: SuperSpeed
    /// - `feature_class`: Pro
    ///
    /// Other fields (`duration_samples`, `voltage_threshold`, `stats_enabled`,
    /// `samples_per_chunk`) use their `Default` values. Override them after
    /// construction if needed.
    pub fn pro8(sample_rate: u64, digital_channels: Vec<u8>, analog_channels: Vec<u8>) -> Self {
        Self {
            sample_rate,
            digital_channels,
            analog_channels,
            ..Self::default()
        }
    }

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

        let (min_chunk, max_chunk) = match self.feature_class {
            DeviceFeatureClass::NonPro | DeviceFeatureClass::Pro => (256u32, 7680u32), // Xilinx
            DeviceFeatureClass::Mso => (64u32, 6016u32),                     // Altera
        };

        if self.samples_per_chunk < min_chunk || self.samples_per_chunk > max_chunk {
            return Err(DriverError::InvalidCaptureConfig {
                reason: format!(
                    "samples_per_chunk {} is outside {} range [{min_chunk}, {max_chunk}]",
                    self.samples_per_chunk, self.feature_class,
                ),
            });
        }

        Ok(())
    }

    /// Compute the USB transfer configuration for this capture.
    ///
    /// Delegates to [`TransferConfig::from_capture`] using this config's
    /// sample rate, channel counts, and USB speed. The returned transfer
    /// size is bandwidth-appropriate and block-aligned, matching the
    /// Saleae formula: `bandwidth / 240` rounded to USB block size.
    pub fn transfer_config(&self) -> TransferConfig {
        TransferConfig::from_capture(
            self.sample_rate,
            self.digital_channels.len() as u8,
            self.analog_channels.len() as u8,
            ADC_BYTES_PER_SAMPLE,
            self.usb_speed,
        )
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

    /// Compute [`CaptureRegisters`] for this configuration.
    ///
    /// Shorthand for [`compute_capture_registers(self)`].
    pub fn capture_registers(&self) -> CaptureRegisters {
        compute_capture_registers(self)
    }

    /// Create a proto [`Disaggregator`](saleae_proto::Disaggregator) matching
    /// this capture configuration.
    ///
    /// The disaggregator settings must match the FPGA capture registers
    /// so that the EP2 data stream is correctly split into per-channel
    /// samples.
    ///
    /// # Errors
    ///
    /// Returns error if the GPIF/ADC combination is invalid.
    pub fn disaggregator(&self) -> crate::DriverResult<saleae_proto::Disaggregator> {
        let regs = self.capture_registers();
        let gpif_width = regs.gpif_packet_size * 8;
        let adc_bits = 12u32; // All supported FX3 devices use 12-bit ADCs

        saleae_proto::Disaggregator::from_register_settings(
            self.feature_class,
            gpif_width,
            adc_bits,
            self.digital_channels.len(),
            self.analog_channels.len(),
            regs.digital_ratio,
            regs.low_speed_divider,
        )
        .map_err(|e| DriverError::InvalidCaptureConfig {
            reason: e.to_string(),
        })
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
/// Maps to the `AfeConfig_RawHardware` struct from Ghidra decompilation of
/// `libgraph_server_shared.so`. The VGA fields (preamp, power, filter, ladder)
/// are encoded into an LMH6518 SPI control word via `encode_vga_config()`.
///
/// The attenuators (attn1, attn2) are separate GPIO controls, not part of
/// the VGA SPI word. The DAC offset voltage is computed dynamically per-channel
/// and is not part of the static 66-entry config table.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AfeChannelConfig {
    /// First attenuator stage: false=bypass, true=10x attenuation.
    pub attn1: bool,
    /// Second attenuator stage: false=bypass, true=100x attenuation.
    pub attn2: bool,
    /// LMH6518 preamp mode: false=Low Gain (-6.5 dB), true=High Gain (+13.5 dB).
    pub preamp: bool,
    /// LMH6518 power mode: false=low power, true=full power.
    /// Always true (1) in the static config table.
    pub power: bool,
    /// LMH6518 filter/bandwidth select (3-bit, 0-7).
    /// Always 4 in the static config table.
    pub filter: u8,
    /// LMH6518 ladder attenuation in dB (0 to -20, must be even).
    pub ladder: i8,
    /// DAC trim voltage. Not part of the static config table (always 0.0 there).
    /// Computed dynamically per-channel based on desired voltage range.
    pub dac_offset: f64,
}

/// Total number of static AFE configurations (3 attn combos x 2 preamp x 11 ladder values).
pub const TOTAL_AFE_CONFIGURATIONS: usize = 66;

impl Default for AfeChannelConfig {
    /// Default: no attenuation, low-gain preamp, full power, filter=4, no ladder atten.
    /// Matches config index 11 in the static table (no atten, LG, ladder=0).
    fn default() -> Self {
        Self {
            attn1: false,
            attn2: false,
            preamp: false,
            power: true,
            filter: 4,
            ladder: 0,
            dac_offset: 0.0,
        }
    }
}

impl AfeChannelConfig {
    /// Encode the VGA fields into an LMH6518 SPI control word (u16).
    ///
    /// Bit layout (from Ghidra decompilation of `EncodeVgaConfig`):
    ///   [3:0]  = ladder attenuation (0-10, maps to 0 to -20 dB in 2 dB steps)
    ///   [4]    = preamp (0=LG, 1=HG)
    ///   [5]    = unused
    ///   [8:6]  = filter/bandwidth select
    ///   [9]    = unused
    ///   [10]   = power mode (0=low power, 1=full power)
    ///
    /// # Panics
    ///
    /// Panics if ladder is out of range (must be 0 to -20, even) or filter > 7.
    pub fn encode_vga_config(&self) -> u16 {
        assert!(
            self.ladder <= 0 && self.ladder >= -20,
            "ladder must be 0 to -20, got {}",
            self.ladder
        );
        assert!(
            self.ladder % 2 == 0,
            "ladder must be even, got {}",
            self.ladder
        );
        assert!(self.filter <= 7, "filter must be 0-7, got {}", self.filter);

        let mut value: u16 = 0;
        value |= ((-self.ladder) as u16) >> 1; // bits [3:0]
        if self.preamp {
            value |= 1 << 4; // bit [4]
        }
        value |= (self.filter as u16 & 7) << 6; // bits [8:6]
        if self.power {
            value |= 1 << 10; // bit [10]
        }
        value
    }

    /// Decode an LMH6518 SPI control word back into VGA fields.
    ///
    /// Attenuator and DAC fields are set to defaults (false / 0.0) since they
    /// are not encoded in the VGA SPI word.
    pub fn from_vga_config(spi_word: u16) -> Self {
        let ladder_encoded = (spi_word & 0x0F) as i8; // bits [3:0]
        let preamp = (spi_word >> 4) & 1 != 0; // bit [4]
        let filter = ((spi_word >> 6) & 7) as u8; // bits [8:6]
        let power = (spi_word >> 10) & 1 != 0; // bit [10]

        Self {
            attn1: false,
            attn2: false,
            preamp,
            power,
            filter,
            ladder: -(ladder_encoded * 2),
            dac_offset: 0.0,
        }
    }

    /// Compute the config index (0-65) in the static AllConfigs table.
    ///
    /// Returns None if the combination is not representable in the table.
    pub fn config_index(&self) -> Option<usize> {
        let attn_group = match (self.attn1, self.attn2) {
            (false, false) => 0,
            (true, false) => 1,
            (true, true) => 2,
            (false, true) => return None, // invalid: attn2 without attn1
        };
        if self.ladder > 0 || self.ladder < -20 || self.ladder % 2 != 0 {
            return None;
        }
        let ladder_idx = ((-self.ladder) / 2) as usize;
        let preamp_offset = if self.preamp { 0 } else { 11 };
        let index = attn_group * 22 + preamp_offset + ladder_idx;
        if index < TOTAL_AFE_CONFIGURATIONS {
            Some(index)
        } else {
            None
        }
    }

    /// Create an AfeChannelConfig from a config table index (0-65).
    ///
    /// Returns None if index >= 66.
    pub fn from_config_index(index: usize) -> Option<Self> {
        if index >= TOTAL_AFE_CONFIGURATIONS {
            return None;
        }
        let attn_group = index / 22;
        let remainder = index % 22;
        let (preamp, ladder_idx) = if remainder < 11 {
            (true, remainder)
        } else {
            (false, remainder - 11)
        };
        let (attn1, attn2) = match attn_group {
            0 => (false, false),
            1 => (true, false),
            2 => (true, true),
            _ => return None,
        };
        Some(Self {
            attn1,
            attn2,
            preamp,
            power: true,
            filter: 4,
            ladder: -(ladder_idx as i8 * 2),
            dac_offset: 0.0,
        })
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
            return Err(DriverError::ReadbackMismatch {
                register: addr,
                expected,
                actual,
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
/// Delegates to [`compute_capture_registers`] and converts to pairs.
/// The order matches the decompiled `SetCaptureParameters` function.
///
/// Pcap Phase 7 example (Logic Pro 8, all channels, 12.5 MSa/s analog+digital):
/// ```text
/// r0x00=0x00  r0x08=0xff  r0x06=0xff  r0x0a=0x20
/// r0x0b=0x10  r0x0c=0x00  r0x1a=0x00  r0x0d=0x18
/// r0x0e=0x08  r0x12=0x02  r0x13=0x00  r0x14=0x10
/// ```
pub fn build_capture_register_pairs(config: &CaptureConfig) -> Vec<(u8, u8)> {
    compute_capture_registers(config).to_register_pairs()
}

/// Compute analog/digital mix ratios for GPIF interleaving.
///
/// The mix ratios tell the FPGA how many 32-bit GPIF words belong to
/// analog data vs digital data in each interleave cycle.
///
/// For Pro devices with 32-bit GPIF and 12-bit ADC:
///   mix_ratio_a = analog GPIF words per cycle
///               = (analog_channel_bytes * num_analog) / packet_size
///   mix_ratio_b = digital GPIF words per cycle
///               = (digital_ratio * packet_size * num_digital) / packet_size
///               = digital_ratio * num_digital
///
/// Validated against pcap for Logic Pro 8 (all 8 channels, 12.5 MSa/s):
///   mix_ratio_a = (12 * 8) / 4 = 24 = 0x18
///   mix_ratio_b = 1 * 8 = 8 = 0x08
fn compute_mix_ratios(
    analog_count: u32,
    digital_count: u32,
    digital_ratio: u32,
    is_pro: bool,
) -> (u8, u8) {
    if !is_pro {
        // Non-Pro (16-bit GPIF, 12-bit ADC): packet_size=2, analog_group_size=4
        // analog_channel_bytes = 3 * 4 / 2 = 6
        let analog_channel_bytes = 6u32;
        let packet_size = 2u32;
        let a = ((analog_channel_bytes * analog_count) / packet_size.max(1)).min(0xFF) as u8;
        let b = (digital_ratio * digital_count).min(0xFF) as u8;
        return (a, b);
    }

    // Pro (32-bit GPIF, 12-bit ADC): packet_size=4, analog_group_size=8
    // analog_channel_bytes = 3 * 8 / 2 = 12
    let analog_channel_bytes = 12u32;
    let packet_size = 4u32;
    let a = ((analog_channel_bytes * analog_count) / packet_size.max(1)).min(0xFF) as u8;
    let b = (digital_ratio * digital_count).min(0xFF) as u8;
    (a, b)
}

// ---------------------------------------------------------------------------
// CaptureRegisters: structured register output
// ---------------------------------------------------------------------------

/// FPGA capture register values computed from a [`CaptureConfig`].
///
/// Each field corresponds to one FpgaDeviceFeatures register written
/// during `SetCaptureParameters`. The struct makes the register semantics
/// explicit and enables unit testing without constructing raw byte pairs.
///
/// # Pcap reference (Logic Pro 8, 12.5 MSa/s, 8 digital + 8 analog)
///
/// ```text
/// r0x00=0x00  r0x08=0xff  r0x06=0xff  r0x0a=0x20
/// r0x0b=0x10  r0x0c=0x00  r0x1a=0x00  r0x0d=0x18
/// r0x0e=0x08  r0x12=0x02  r0x13=0x00  r0x14=0x10
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureRegisters {
    /// r0x00: capture init, always 0x00 during configuration.
    pub capture_init: u8,
    /// r0x08: digital channel enable bitmask (bit N = channel N enabled).
    pub digital_ch_enable_lo: u8,
    /// r0x09: digital channel enable high byte (16-channel devices only).
    pub digital_ch_enable_hi: Option<u8>,
    /// r0x06: analog channel enable bitmask (bit N = channel N enabled).
    pub analog_ch_enable_lo: u8,
    /// r0x07: analog channel enable high byte (16-channel devices only).
    pub analog_ch_enable_hi: Option<u8>,
    /// r0x0a: analog decimation factor low byte.
    ///
    /// For Pro devices with 4x interleaved ADC:
    ///   analog_decimation = (base_analog_rate * 4) / sample_rate
    pub analog_decimation: u8,
    /// r0x0b: digital sample rate divider low byte.
    ///
    /// For Pro devices: digital_divider = fpga_master_clock / sample_rate
    /// where fpga_master_clock = 200 MHz.
    pub digital_divider_lo: u8,
    /// r0x0c: digital sample rate divider high byte.
    pub digital_divider_hi: u8,
    /// r0x1a: ADC configuration (Pro devices only).
    ///
    /// 0 = 4x interleaved, 1 = standard, absent for non-Pro.
    pub adc_config: Option<u8>,
    /// r0x1b: upper analog decimation (board rev 1 only).
    pub analog_dec_upper: Option<u8>,
    /// r0x0d: analog GPIF words per interleave cycle (mix ratio A).
    ///
    /// = (analog_channel_bytes * num_analog_channels) / gpif_packet_size
    pub mix_ratio_a: u8,
    /// r0x0e: digital GPIF words per interleave cycle (mix ratio B).
    ///
    /// = digital_ratio * num_digital_channels
    pub mix_ratio_b: u8,
    /// r0x12: ADC type / capture config.
    ///
    /// Board rev 0-1 (Xilinx, AD9637): 0x02
    /// Board rev 3+ (Lattice): 0x01
    pub capture_config_2: u8,
    /// r0x13: capture control flags.
    ///
    /// Board rev 2+: 0x01 (capture enabled), rev 0-1: 0x00.
    pub capture_control: u8,
    /// r0x14: GPIF bus width config.
    ///
    /// 0x10 = 32-bit GPIF (Pro 8/16), 0x08 = 16-bit GPIF (Logic 4/8).
    pub gpif_bus_width: u8,

    // Derived values (not written to registers, but needed for disaggregator).
    /// Digital-to-analog sample ratio (digital packets per cycle).
    /// For equal-rate digital+analog: 1. For 2x digital: 2. Digital-only: 1.
    pub digital_ratio: u32,
    /// Low-speed divider selector value for ContractReadData.
    /// 0=div1, 1=div2, 2=div4, 3=div8.
    pub low_speed_divider_selector: u8,
    /// Actual low-speed divider (1, 2, 4, or 8).
    pub low_speed_divider: u32,
    /// GPIF packet size in bytes (2 for 16-bit, 4 for 32-bit).
    pub gpif_packet_size: u32,
    /// Analog group size (samples per ADC group): 4 for 16-bit GPIF, 8 for 32-bit.
    pub analog_group_size: u32,
}

/// FPGA master clock frequency in Hz for Logic Pro 8/16.
///
/// Used to compute the digital sample rate divider (r0x0b/r0x0c).
/// Derived from pcap: 200 MHz / 12.5 MSa/s = 16 = 0x10 (matches pcap r0x0b).
pub const PRO_FPGA_MASTER_CLOCK_HZ: u64 = 200_000_000;

/// ADC interleave factor for Pro devices when analog decimation > 1.
///
/// The AD9637 ADC runs at 4x the sample rate in interleaved mode.
/// This factor is applied to base_analog_rate when computing analog_decimation.
/// Validated: (100 MHz * 4) / 12.5 MSa/s = 32 = 0x20 (matches pcap r0x0a).
const ADC_4X_INTERLEAVE_FACTOR: u64 = 4;

impl CaptureRegisters {
    /// Convert to a vector of (addr, value) pairs suitable for WriteRegisters.
    ///
    /// The order matches the decompiled `SetCaptureParameters` function.
    pub fn to_register_pairs(&self) -> Vec<(u8, u8)> {
        let mut pairs = Vec::with_capacity(15);

        pairs.push((fpga_regs::FEAT_CAPTURE_INIT, self.capture_init));
        pairs.push((
            fpga_regs::FEAT_DIGITAL_CH_ENABLE_LO,
            self.digital_ch_enable_lo,
        ));
        if let Some(hi) = self.digital_ch_enable_hi {
            pairs.push((fpga_regs::FEAT_DIGITAL_CH_ENABLE_HI, hi));
        }
        pairs.push((
            fpga_regs::FEAT_ANALOG_CH_ENABLE_LO,
            self.analog_ch_enable_lo,
        ));
        if let Some(hi) = self.analog_ch_enable_hi {
            pairs.push((fpga_regs::FEAT_ANALOG_CH_ENABLE_HI, hi));
        }
        pairs.push((fpga_regs::FEAT_ANALOG_DECIMATION, self.analog_decimation));
        pairs.push((fpga_regs::FEAT_DIGITAL_DIVIDER_LO, self.digital_divider_lo));
        pairs.push((fpga_regs::FEAT_DIGITAL_DIVIDER_HI, self.digital_divider_hi));
        if let Some(adc) = self.adc_config {
            pairs.push((fpga_regs::FEAT_ADC_CONFIG, adc));
        }
        if let Some(dec) = self.analog_dec_upper {
            pairs.push((fpga_regs::FEAT_ANALOG_DEC_UPPER, dec));
        }
        pairs.push((fpga_regs::FEAT_MIX_RATIO_A, self.mix_ratio_a));
        pairs.push((fpga_regs::FEAT_MIX_RATIO_B, self.mix_ratio_b));
        pairs.push((fpga_regs::FEAT_CAPTURE_CONFIG_2, self.capture_config_2));
        pairs.push((fpga_regs::FEAT_CAPTURE_CONTROL, self.capture_control));
        pairs.push((fpga_regs::FEAT_DECIMATION_FACTOR, self.gpif_bus_width));

        pairs
    }
}

/// Compute FPGA capture register values from a capture configuration.
///
/// This is the structured replacement for [`build_capture_register_pairs`].
/// It returns a [`CaptureRegisters`] with all computed values, which can
/// then be converted to register pairs via [`CaptureRegisters::to_register_pairs`].
///
/// The register formulas are derived from decompiled `SetCaptureParameters`
/// (ELF 0x1c24ca0) and validated against the single known pcap reference
/// (Logic Pro 8, 12.5 MSa/s, all channels).
///
/// # Pcap validation
///
/// For the reference config (12.5 MSa/s, 8 digital + 8 analog, Pro 8):
/// - analog_decimation = (100 MHz * 4) / 12.5 MHz = 32 = 0x20
/// - digital_divider = 200 MHz / 12.5 MHz = 16 = 0x10
/// - mix_ratio_a = (12 * 8) / 4 = 24 = 0x18
/// - mix_ratio_b = 1 * 8 = 8 = 0x08
/// - gpif_bus_width = 0x10 (32-bit GPIF)
///
/// # Warnings
///
/// Logs a warning for sample rate / channel combinations that have not
/// been hardware-validated.
pub fn compute_capture_registers(config: &CaptureConfig) -> CaptureRegisters {
    let analog_count = config.analog_channels.len() as u32;
    let digital_count = config.digital_channels.len() as u32;

    // Channel enable bitmasks.
    let mut digital_ch_mask: u16 = 0;
    for &ch in &config.digital_channels {
        digital_ch_mask |= 1 << ch;
    }
    let mut analog_ch_mask: u16 = 0;
    for &ch in &config.analog_channels {
        analog_ch_mask |= 1 << ch;
    }

    // GPIF bus width: Pro devices use 32-bit, non-Pro use 16-bit.
    let (gpif_packet_size, gpif_bus_width_reg, analog_group_size): (u32, u8, u32) = if config.is_pro
    {
        (4, 0x10, 8)
    } else {
        (2, 0x08, 4)
    };

    // Analog decimation.
    // For Pro devices with ADC in 4x interleaved mode:
    //   analog_decimation = (base_analog_rate * 4) / sample_rate
    // Validated: (100 MHz * 4) / 12.5 MHz = 32 = 0x20 (matches pcap r0x0a).
    let analog_decimation: u16 = if analog_count > 0 && config.base_analog_rate > 0 {
        let effective_analog_clock = if config.is_pro {
            config.base_analog_rate * ADC_4X_INTERLEAVE_FACTOR
        } else {
            config.base_analog_rate
        };
        (effective_analog_clock / config.sample_rate.max(1)) as u16
    } else {
        0
    };

    // Digital divider: fpga_master_clock / sample_rate.
    // For Pro 8: 200 MHz / 12.5 MHz = 16 = 0x10 (matches pcap r0x0b).
    // For non-Pro: divider = rate_ratio - 1 (from decompilation).
    let fpga_clock = if config.is_pro {
        PRO_FPGA_MASTER_CLOCK_HZ
    } else {
        config.base_digital_rate
    };
    let digital_divider: u16 = if config.sample_rate > 0 {
        let ratio = (fpga_clock / config.sample_rate) as u16;
        if config.is_pro {
            ratio
        } else {
            ratio.saturating_sub(1)
        }
    } else {
        0
    };

    // Digital ratio: how many digital packet groups per analog group.
    // For equal digital/analog rate: 1. For digital=2x analog: 2.
    // Digital-only: 1 (one digital group, no analog).
    let digital_ratio: u32 = if analog_count > 0 && digital_count > 0 {
        // Both rates are the same in the pcap reference (12.5 MSa/s).
        // digital_ratio = digital_divider / analog_decimation scaled to
        // common rate. For equal rates: 1.
        // TODO: More pcap data needed for non-equal rate configs.
        let analog_dec = analog_decimation as u32;
        let digital_div = digital_divider as u32;
        if analog_dec > 0 && digital_div > 0 {
            // The ratio of analog samples to digital samples per cycle.
            // analog_decimation / digital_divider gives the ratio.
            // For pcap: 32 / 16 = 2... but digital_ratio should be 1?
            // Actually: digital rate is higher, so digital_ratio = analog_dec / digital_div
            // when they sample at same effective rate, this gives how many
            // digital groups fit in one analog group.
            // For pcap: digital captures at fpga_clock/divider = 200/16 = 12.5 MSa/s
            // analog captures at effective_clock/decimation = 400/32 = 12.5 MSa/s
            // Same rate -> ratio 1.
            //
            // More generally: digital_rate = fpga_clock / digital_divider
            //                 analog_rate = effective_analog_clock / analog_decimation
            // digital_ratio = digital_rate / analog_rate
            // But we need integer ratios via GCD.
            let d_rate = fpga_clock / digital_divider.max(1) as u64;
            let a_rate = if config.is_pro {
                (config.base_analog_rate * ADC_4X_INTERLEAVE_FACTOR)
                    / analog_decimation.max(1) as u64
            } else {
                config.base_analog_rate / analog_decimation.max(1) as u64
            };
            if a_rate > 0 {
                let g = gcd(d_rate, a_rate);
                (d_rate / g) as u32
            } else {
                1
            }
        } else {
            1
        }
    } else {
        1
    };

    // Mix ratios (GPIF words per cycle for analog and digital).
    let (mix_ratio_a, mix_ratio_b) = if analog_count > 0 && digital_count > 0 {
        compute_mix_ratios(analog_count, digital_count, digital_ratio, config.is_pro)
    } else if analog_count > 0 {
        // Analog-only: all GPIF bandwidth for analog.
        let analog_channel_bytes = if config.is_pro { 12u32 } else { 6u32 };
        let a = ((analog_channel_bytes * analog_count) / gpif_packet_size).min(0xFF) as u8;
        (a, 0x00)
    } else {
        // Digital-only: all GPIF bandwidth for digital.
        (0x00, (digital_ratio * digital_count).min(0xFF) as u8)
    };

    // ADC config (0x1A): only for Pro/JESD devices.
    let adc_config: Option<u8> = if config.is_pro {
        Some(match analog_decimation {
            0 | 1 => 0, // 4x interleaved or no analog
            2 => 1,     // standard
            _ => 0,     // 4x interleaved (most common for Pro)
        })
    } else {
        None
    };

    // Upper analog decimation (board rev 1 only).
    let analog_dec_upper: Option<u8> = if config.board_revision == 1 {
        Some(((analog_decimation >> 8) & 0xFF).min(1) as u8)
    } else {
        None
    };

    // Capture control flags.
    let capture_control: u8 = if config.board_revision >= 2 {
        0x01
    } else {
        0x00
    };

    // Capture config 2 (0x12): ADC type selection.
    let capture_config_2: u8 = if config.board_revision <= 1 {
        0x02 // AD9637 = index 1, value = 1 << 1
    } else {
        0x01
    };

    // Low-speed divider: determines ContractReadData decimation.
    // At high sample rates (near max GPIF throughput): divider=1.
    // At lower rates, the FPGA pads data and the host contracts.
    // For 12.5 MSa/s on Pro 8: divider=1 (pcap r0x13=0x00).
    // The threshold is approximate; needs more pcap data.
    let low_speed_divider_selector: u8 = 0; // div1 for now
    let low_speed_divider: u32 = 1;

    // Warn for unvalidated configurations.
    let is_pcap_reference = config.sample_rate == 12_500_000
        && digital_count == 8
        && analog_count == 8
        && config.is_pro
        && config.total_device_channels == 8;

    if !is_pcap_reference {
        warn!(
            sample_rate = config.sample_rate,
            digital = digital_count,
            analog = analog_count,
            "Capture register values not hardware-validated for this configuration"
        );
    }

    CaptureRegisters {
        capture_init: 0x00,
        digital_ch_enable_lo: (digital_ch_mask & 0xFF) as u8,
        digital_ch_enable_hi: if config.total_device_channels == 16 {
            Some(((digital_ch_mask >> 8) & 0xFF) as u8)
        } else {
            None
        },
        analog_ch_enable_lo: (analog_ch_mask & 0xFF) as u8,
        analog_ch_enable_hi: if config.total_device_channels == 16 {
            Some(((analog_ch_mask >> 8) & 0xFF) as u8)
        } else {
            None
        },
        analog_decimation: (analog_decimation & 0xFF) as u8,
        digital_divider_lo: (digital_divider & 0xFF) as u8,
        digital_divider_hi: ((digital_divider >> 8) & 0xFF) as u8,
        adc_config,
        analog_dec_upper,
        mix_ratio_a,
        mix_ratio_b,
        capture_config_2,
        capture_control,
        gpif_bus_width: gpif_bus_width_reg,
        digital_ratio,
        low_speed_divider_selector,
        low_speed_divider,
        gpif_packet_size,
        analog_group_size,
    }
}

/// GCD helper for ratio computation. Mirrors ELF VA 0x1c47140.
fn gcd(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a
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
        let transfer_size = config.transfer_config().transfer_size;
        Self {
            read_buf: vec![0u8; transfer_size],
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
// Unified capture pipeline: configure -> start -> stream -> stop
// ---------------------------------------------------------------------------

/// Result of a capture pipeline run.
#[derive(Debug, Clone)]
pub struct CaptureResult {
    /// Raw EP2 bytes collected during the capture.
    pub raw_data: Vec<u8>,
    /// Number of individual EP2 bulk transfer reads completed.
    pub read_count: u64,
    /// Wall-clock duration of the EP2 read loop.
    pub elapsed: std::time::Duration,
    /// Whether the capture ran for the full requested duration.
    ///
    /// `false` if the EP2 read loop broke out early due to a USB transfer
    /// error. Callers should treat partial captures with caution.
    pub completed_normally: bool,
    /// Warnings from the capture teardown phase.
    ///
    /// Teardown sends `write_registers(r0x00=0x00)` to clear the trigger
    /// and `StopStream(0x02)` to halt the FPGA. Failures here do not
    /// prevent data from being returned, but indicate the device may be
    /// in an inconsistent state (requiring power cycle before next capture).
    pub teardown_warnings: Vec<String>,
}

impl CaptureResult {
    /// Number of raw bytes read from EP2.
    pub fn bytes_read(&self) -> usize {
        self.raw_data.len()
    }

    /// First N bytes of EP2 data for diagnostic display.
    pub fn first_bytes(&self, n: usize) -> &[u8] {
        &self.raw_data[..self.raw_data.len().min(n)]
    }

    /// Throughput in megabits per second.
    pub fn throughput_mbps(&self) -> f64 {
        let secs = self.elapsed.as_secs_f64();
        if secs > 0.0 {
            (self.raw_data.len() as f64 * 8.0) / secs / 1_000_000.0
        } else {
            0.0
        }
    }

    /// Throughput in kilobytes per second.
    pub fn throughput_kbps(&self) -> f64 {
        let secs = self.elapsed.as_secs_f64();
        if secs > 0.0 {
            self.raw_data.len() as f64 / secs / 1024.0
        } else {
            0.0
        }
    }

    /// Whether teardown encountered any errors.
    ///
    /// When true, the device may be in an inconsistent state and could
    /// require a power cycle before the next capture.
    pub fn has_teardown_warnings(&self) -> bool {
        !self.teardown_warnings.is_empty()
    }
}

/// Run the capture pipeline: configure registers, start, stream EP2, stop.
///
/// This encapsulates the common capture sequence shared by all CLI capture
/// commands:
///
/// 1. Validate and write capture registers to FPGA (via `configure_capture`).
/// 2. Send StartSampling (0x01) and trigger capture (r0x00=0x01).
/// 3. Read EP2 bulk data for the specified duration.
/// 4. Stop capture (r0x00=0x00) and send StopStream (0x02).
///
/// The caller is responsible for device init (firmware, FPGA programming,
/// ADC init) before calling this, and for post-processing (disaggregation,
/// calibration, export) after.
///
/// # Arguments
///
/// - `ep1`: Initialized EP1 command channel (firmware loaded, FPGA programmed).
/// - `config`: Capture configuration (sample rate, channels, etc.).
/// - `duration`: How long to read EP2 data.
/// - `stop_flag`: Optional atomic flag checked each iteration; when set to
///   `true` (e.g. by a Ctrl-C handler), capture stops gracefully.
///
/// The EP2 read buffer size is computed from capture bandwidth using
/// [`CaptureConfig::transfer_config`], which implements the Saleae formula:
/// `bandwidth / 240` rounded to USB block alignment, capped at 1.6 MB.
///
/// # Errors
///
/// Returns an error if register configuration, start command, or stop
/// command fails. EP2 read errors during streaming cause an early stop
/// but do not return an error (the partial data is returned).
pub fn run_capture(
    ep1: &mut Ep1CommandChannel,
    config: &CaptureConfig,
    duration: std::time::Duration,
    stop_flag: Option<&std::sync::atomic::AtomicBool>,
) -> DriverResult<CaptureResult> {
    // Step 1: Configure FPGA registers + readback verify.
    configure_capture(ep1, config)?;

    // Compute transfer size from capture bandwidth. The sync path uses only
    // the transfer_size (single-buffered), not the full queue_depth.
    let transfer_config = config.transfer_config();
    let buf_size = transfer_config.transfer_size;

    // Step 2: Start sampling.
    info!("Sending StartSampling (0x01)");
    ep1.start_sampling()?;

    // Step 3: Trigger capture (r0x00=0x01).
    debug!("Writing r0x00=0x01 (trigger capture)");
    ep1.write_registers(&[(0x00, 0x01)])?;

    // Step 4: EP2 read loop.
    // Duration of zero means "capture until stop_flag is set" (Ctrl-C).
    let capture_indefinitely = duration.is_zero();
    info!(
        duration_ms = duration.as_millis(),
        capture_indefinitely,
        buf_size, "Reading EP2 capture data"
    );

    // Assume ~1 USB transfer completes per 10ms (conservative for SuperSpeed).
    // Cap at 256 MiB to avoid absurd allocations; Vec will grow if needed.
    let estimated_capacity = if capture_indefinitely {
        // Start with a reasonable default for indefinite capture.
        16 * 1024 * 1024 // 16 MiB
    } else {
        (buf_size as u128 * duration.as_millis() / 10).min(256 * 1024 * 1024) as usize
    };
    let mut raw_data = Vec::with_capacity(estimated_capacity);
    let mut read_count: u64 = 0;
    let mut completed_normally = true;
    let mut buf = vec![0u8; buf_size];
    let start = std::time::Instant::now();

    let should_stop = |start: &std::time::Instant| -> bool {
        // Check stop flag first (Ctrl-C).
        if let Some(flag) = stop_flag {
            if flag.load(std::sync::atomic::Ordering::Relaxed) {
                return true;
            }
        }
        // Then check duration (unless capturing indefinitely).
        !capture_indefinitely && start.elapsed() >= duration
    };

    while !should_stop(&start) {
        match ep1.transport().bulk_transfer_in(EP2_CAPTURE_IN, &mut buf) {
            Ok(n) if n > 0 => {
                raw_data.extend_from_slice(&buf[..n]);
                read_count += 1;
            }
            Ok(_) => {
                debug!("EP2: zero-length read");
            }
            Err(e) => {
                warn!("EP2 read error (stopping early): {e}");
                completed_normally = false;
                break;
            }
        }
    }

    // If stopped by Ctrl-C, that's still a "normal" completion.
    if let Some(flag) = stop_flag {
        if flag.load(std::sync::atomic::Ordering::Relaxed) {
            info!("Capture stopped by signal (Ctrl-C)");
        }
    }

    let elapsed = start.elapsed();

    // Step 5: Stop capture. Warn on failure but don't lose collected data.
    info!(
        bytes_read = raw_data.len(),
        read_count,
        elapsed_ms = elapsed.as_millis(),
        "Stopping capture"
    );
    let mut teardown_warnings = Vec::new();
    if let Err(e) = ep1.write_registers(&[(0x00, 0x00)]) {
        let msg = format!("Failed to clear trigger register: {e}");
        warn!("{msg}");
        teardown_warnings.push(msg);
    }
    if let Err(e) = ep1.stop_stream() {
        let msg = format!("Failed to send StopStream: {e}");
        warn!("{msg}");
        teardown_warnings.push(msg);
    }

    let result = CaptureResult {
        raw_data,
        read_count,
        elapsed,
        completed_normally,
        teardown_warnings,
    };
    info!(
        throughput_mbps = format_args!("{:.1}", result.throughput_mbps()),
        "Capture complete"
    );

    Ok(result)
}

/// Run the capture pipeline using queued bulk transfers for sustained throughput.
///
/// This is the streaming counterpart to [`run_capture`]. Instead of single-buffered
/// synchronous EP2 reads, it uses a
/// [`BulkStreamReader`](saleae_usb::BulkStreamReader) with multiple outstanding
/// transfers to keep the USB pipe saturated and prevent FPGA FIFO overflow.
///
/// The transfer size and queue depth are computed from the capture parameters
/// using [`TransferConfig::from_capture`](crate::transfer_queue::TransferConfig::from_capture),
/// which implements the same formula as the original Saleae binary:
/// `bandwidth / 240` rounded to block alignment, with 128 queued transfers.
///
/// # Arguments
///
/// - `ep1`: Initialized EP1 command channel (firmware loaded, FPGA programmed).
/// - `config`: Capture configuration (sample rate, channels, etc.).
/// - `duration`: How long to read EP2 data (zero = until stop_flag is set).
/// - `stream_reader`: A bulk stream reader implementation (real or mock).
/// - `stop_flag`: Optional atomic flag checked each iteration; when set to
///   `true` (e.g. by a Ctrl-C handler), capture stops gracefully.
///
/// # Errors
///
/// Returns an error if register configuration, start command, stop command,
/// or stream start fails. Individual transfer errors during streaming cause
/// an early stop but do not return an error (the partial data is returned).
pub fn run_capture_streaming(
    ep1: &mut Ep1CommandChannel,
    config: &CaptureConfig,
    duration: std::time::Duration,
    mut stream_reader: Box<dyn saleae_usb::BulkStreamReader>,
    stop_flag: Option<&std::sync::atomic::AtomicBool>,
) -> DriverResult<CaptureResult> {
    // Step 1: Configure FPGA registers + readback verify.
    configure_capture(ep1, config)?;

    // Step 2: Compute transfer sizing from capture bandwidth.
    let transfer_config = config.transfer_config();

    info!(
        transfer_size = transfer_config.transfer_size,
        queue_depth = transfer_config.queue_depth,
        total_buffer_mb = transfer_config.total_buffer_bytes() / (1024 * 1024),
        "Streaming capture: computed transfer config"
    );

    // Step 3: Start the stream reader (pre-submit transfers).
    stream_reader
        .start(
            EP2_CAPTURE_IN,
            transfer_config.transfer_size,
            transfer_config.queue_depth,
        )
        .map_err(DriverError::Usb)?;

    // Step 4: Start sampling.
    info!("Sending StartSampling (0x01)");
    ep1.start_sampling()?;

    // Step 5: Trigger capture (r0x00=0x01).
    debug!("Writing r0x00=0x01 (trigger capture)");
    ep1.write_registers(&[(0x00, 0x01)])?;

    // Step 6: EP2 read loop using queued transfers.
    let capture_indefinitely = duration.is_zero();
    let estimated_capacity = if capture_indefinitely {
        16 * 1024 * 1024 // 16 MiB starting point for indefinite capture
    } else {
        (transfer_config.transfer_size as u128 * duration.as_millis() / 10)
            .min(256 * 1024 * 1024) as usize
    };
    let mut raw_data = Vec::with_capacity(estimated_capacity);
    let mut read_count: u64 = 0;
    let mut completed_normally = true;
    let start = std::time::Instant::now();
    let transfer_timeout = std::time::Duration::from_secs(2);

    info!(
        duration_ms = duration.as_millis(),
        capture_indefinitely,
        "Reading EP2 capture data (streaming)"
    );

    let should_stop = |start: &std::time::Instant| -> bool {
        if let Some(flag) = stop_flag {
            if flag.load(std::sync::atomic::Ordering::Relaxed) {
                return true;
            }
        }
        !capture_indefinitely && start.elapsed() >= duration
    };

    while !should_stop(&start) {
        match stream_reader.next_completion(transfer_timeout) {
            Ok(buf) => {
                let len = buf.len();
                raw_data.extend_from_slice(&buf);
                read_count += 1;

                // Recycle the buffer back to the queue.
                if let Err(e) = stream_reader.recycle(buf) {
                    warn!("Failed to recycle transfer buffer: {e}");
                    completed_normally = false;
                    break;
                }

                if read_count % 1000 == 0 {
                    debug!(
                        read_count,
                        total_bytes = raw_data.len(),
                        last_transfer_bytes = len,
                        pending = stream_reader.pending(),
                        "Streaming progress"
                    );
                }
            }
            Err(e) => {
                // Timeout is expected at the end of capture when data stops flowing,
                // or when stopped by Ctrl-C during indefinite capture.
                let timed_capture_done =
                    !capture_indefinitely && start.elapsed() >= duration;
                let stopped_by_signal = stop_flag
                    .map(|f| f.load(std::sync::atomic::Ordering::Relaxed))
                    .unwrap_or(false);
                if matches!(e, saleae_usb::UsbError::Timeout { .. })
                    && (timed_capture_done || stopped_by_signal)
                {
                    break;
                }
                warn!("EP2 stream error (stopping early): {e}");
                completed_normally = false;
                break;
            }
        }
    }

    let elapsed = start.elapsed();

    // Step 7: Stop the stream reader (cancel pending transfers).
    let mut teardown_warnings = Vec::new();
    if let Err(e) = stream_reader.stop() {
        let msg = format!("Failed to stop stream reader: {e}");
        warn!("{msg}");
        teardown_warnings.push(msg);
    }

    // Step 8: Stop capture. Warn on failure but don't lose collected data.
    info!(
        bytes_read = raw_data.len(),
        read_count,
        elapsed_ms = elapsed.as_millis(),
        "Stopping capture"
    );
    if let Err(e) = ep1.write_registers(&[(0x00, 0x00)]) {
        let msg = format!("Failed to clear trigger register: {e}");
        warn!("{msg}");
        teardown_warnings.push(msg);
    }
    if let Err(e) = ep1.stop_stream() {
        let msg = format!("Failed to send StopStream: {e}");
        warn!("{msg}");
        teardown_warnings.push(msg);
    }

    let result = CaptureResult {
        raw_data,
        read_count,
        elapsed,
        completed_normally,
        teardown_warnings,
    };
    info!(
        throughput_mbps = format_args!("{:.1}", result.throughput_mbps()),
        "Streaming capture complete"
    );

    Ok(result)
}

// ---------------------------------------------------------------------------
// Streaming capture (queued bulk transfers)
// ---------------------------------------------------------------------------

/// Handle for a capture session using queued bulk transfers.
///
/// Unlike [`CaptureStream`] which uses single-buffered synchronous reads,
/// `StreamingCapture` uses a [`BulkStreamReader`] with multiple outstanding
/// transfers to prevent FPGA FIFO overflow at high sample rates.
///
/// # Usage
///
/// ```rust,no_run
/// # use saleae_driver::capture::StreamingCapture;
/// # use saleae_driver::TransferConfig;
/// # fn example(reader: Box<dyn saleae_usb::BulkStreamReader>) {
/// let config = TransferConfig {
///     transfer_size: 16384,
///     queue_depth: 128,
///     block_size: 16384,
/// };
/// let mut capture = StreamingCapture::new(reader, config, 0x82);
/// capture.start().unwrap();
///
/// // Read loop: get completed transfers, process data, recycle buffers.
/// loop {
///     match capture.next_transfer(std::time::Duration::from_secs(1)) {
///         Ok(buf) => {
///             // Process buf...
///             capture.recycle(buf).unwrap();
///         }
///         Err(e) => break,
///     }
/// }
///
/// capture.stop().unwrap();
/// # }
/// ```
pub struct StreamingCapture {
    /// The underlying bulk stream reader.
    reader: Box<dyn saleae_usb::BulkStreamReader>,
    /// Transfer queue configuration (sizes, depth).
    config: crate::transfer_queue::TransferConfig,
    /// EP2 IN endpoint address.
    endpoint: u8,
    /// Total bytes received across all transfers.
    total_bytes: u64,
    /// Total number of completed transfers.
    total_transfers: u64,
    /// Whether the stream is active.
    active: bool,
}

impl StreamingCapture {
    /// Create a new streaming capture.
    ///
    /// The stream is not started until [`start()`](StreamingCapture::start) is called.
    ///
    /// # Arguments
    ///
    /// - `reader`: A bulk stream reader implementation (real or mock).
    /// - `config`: Transfer queue configuration from [`TransferConfig::from_capture`].
    /// - `endpoint`: The EP2 IN bulk endpoint address (typically `0x82`).
    pub fn new(
        reader: Box<dyn saleae_usb::BulkStreamReader>,
        config: crate::transfer_queue::TransferConfig,
        endpoint: u8,
    ) -> Self {
        Self {
            reader,
            config,
            endpoint,
            total_bytes: 0,
            total_transfers: 0,
            active: false,
        }
    }

    /// Start the streaming capture by submitting initial transfer buffers.
    pub fn start(&mut self) -> DriverResult<()> {
        if self.active {
            return Err(DriverError::InvalidState {
                current: "streaming".into(),
                required: "idle".into(),
            });
        }

        info!(
            endpoint = self.endpoint,
            transfer_size = self.config.transfer_size,
            queue_depth = self.config.queue_depth,
            total_buffer_mb = self.config.total_buffer_bytes() / (1024 * 1024),
            "Starting streaming capture"
        );

        self.reader
            .start(
                self.endpoint,
                self.config.transfer_size,
                self.config.queue_depth,
            )
            .map_err(DriverError::Usb)?;

        self.active = true;
        Ok(())
    }

    /// Wait for the next completed transfer and return its data.
    ///
    /// The caller must call [`recycle()`](StreamingCapture::recycle) with the
    /// returned buffer after processing to maintain queue depth.
    pub fn next_transfer(&mut self, timeout: std::time::Duration) -> DriverResult<Vec<u8>> {
        if !self.active {
            return Err(DriverError::InvalidState {
                current: "idle".into(),
                required: "streaming".into(),
            });
        }

        let buf = self
            .reader
            .next_completion(timeout)
            .map_err(DriverError::Usb)?;
        self.total_bytes += buf.len() as u64;
        self.total_transfers += 1;

        if self.total_transfers % 1000 == 0 {
            debug!(
                total_bytes = self.total_bytes,
                total_transfers = self.total_transfers,
                pending = self.reader.pending(),
                "Streaming progress"
            );
        }

        Ok(buf)
    }

    /// Return a processed buffer for reuse (resubmits to the transfer queue).
    pub fn recycle(&mut self, buf: Vec<u8>) -> DriverResult<()> {
        if !self.active {
            return Err(DriverError::InvalidState {
                current: "idle".into(),
                required: "streaming".into(),
            });
        }

        self.reader.recycle(buf).map_err(DriverError::Usb)
    }

    /// Stop the streaming capture, cancelling all pending transfers.
    pub fn stop(&mut self) -> DriverResult<()> {
        if !self.active {
            return Ok(());
        }

        info!(
            total_bytes = self.total_bytes,
            total_transfers = self.total_transfers,
            "Stopping streaming capture"
        );

        self.reader.stop().map_err(DriverError::Usb)?;
        self.active = false;
        Ok(())
    }

    /// Total bytes received across all completed transfers.
    pub fn total_bytes(&self) -> u64 {
        self.total_bytes
    }

    /// Total number of completed transfers.
    pub fn total_transfers(&self) -> u64 {
        self.total_transfers
    }

    /// Number of transfers currently in-flight.
    pub fn pending(&self) -> usize {
        self.reader.pending()
    }

    /// Whether the stream is currently active.
    pub fn is_active(&self) -> bool {
        self.active
    }
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

    /// Create a mock channel with seed=0 (identity XOR).
    fn mock_channel(mock: &std::sync::Arc<MockTransport>) -> Ep1CommandChannel {
        let transport: Box<dyn saleae_usb::transport::UsbTransport> = Box::new(mock.clone());
        let mut channel = Ep1CommandChannel::new(transport, EncryptionMode::XorAll, true);
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

    #[test]
    fn configure_capture_readback_mismatch_returns_error() {
        let mock = std::sync::Arc::new(MockTransport::new());
        let config = CaptureConfig {
            sample_rate: 1_000_000,
            digital_channels: vec![0, 1],
            ..CaptureConfig::default()
        };

        // WriteRegisters ACK (success).
        mock.push_bulk_in_response(ep::BULK_IN, vec![0x00]);

        // ReadRegisters readback: return values that DIFFER from what was written.
        // Build the real register pairs, then corrupt the third value.
        let pairs = build_capture_register_pairs(&config);
        let mut readback: Vec<u8> = pairs.iter().map(|&(_, v)| v).collect();
        assert!(
            readback.len() >= 3,
            "need at least 3 registers to corrupt one"
        );
        let corrupt_idx = 2;
        let expected_val = readback[corrupt_idx];
        readback[corrupt_idx] = expected_val.wrapping_add(1); // corrupt
        mock.push_bulk_in_response(ep::BULK_IN, readback);

        let mut channel = mock_channel(&mock);
        let err = configure_capture(&mut channel, &config).unwrap_err();

        let msg = err.to_string();
        assert!(
            msg.contains("readback mismatch"),
            "expected ReadbackMismatch error, got: {msg}"
        );
        // Verify the error identifies the correct register.
        let corrupt_addr = pairs[corrupt_idx].0;
        assert!(
            msg.contains(&format!("{corrupt_addr:#04x}")),
            "error should mention register address {corrupt_addr:#04x}: {msg}"
        );
    }

    #[test]
    fn configure_capture_readback_short_response_returns_mismatch() {
        let mock = std::sync::Arc::new(MockTransport::new());
        let config = CaptureConfig {
            sample_rate: 1_000_000,
            digital_channels: vec![0, 1],
            ..CaptureConfig::default()
        };

        // WriteRegisters ACK.
        mock.push_bulk_in_response(ep::BULK_IN, vec![0x00]);

        // ReadRegisters readback: return fewer bytes than expected.
        // When readback is too short, the code uses unwrap_or(0xFF) which
        // should mismatch on the first missing register.
        let pairs = build_capture_register_pairs(&config);
        // Return only the first value (rest will be 0xFF from unwrap_or).
        let readback = vec![pairs[0].1];
        mock.push_bulk_in_response(ep::BULK_IN, readback);

        let mut channel = mock_channel(&mock);
        let err = configure_capture(&mut channel, &config).unwrap_err();

        let msg = err.to_string();
        assert!(
            msg.contains("readback mismatch"),
            "short readback should cause mismatch: {msg}"
        );
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
    // compute_capture_registers tests
    // -------------------------------------------------------------------

    #[test]
    fn capture_registers_pcap_exact_match() {
        // The single known pcap reference: Logic Pro 8, 12.5 MSa/s,
        // all 8 digital + 8 analog channels. Every register value must
        // match the pcap exactly.
        let config = CaptureConfig {
            sample_rate: 12_500_000,
            digital_channels: (0..8).collect(),
            analog_channels: (0..8).collect(),
            total_device_channels: 8,
            board_revision: 1,
            is_pro: true,
            base_digital_rate: 500_000_000,
            base_analog_rate: 100_000_000,
            ..CaptureConfig::default()
        };
        let regs = compute_capture_registers(&config);

        assert_eq!(regs.capture_init, 0x00, "r0x00");
        assert_eq!(regs.digital_ch_enable_lo, 0xff, "r0x08");
        assert_eq!(regs.digital_ch_enable_hi, None, "no r0x09 for 8ch");
        assert_eq!(regs.analog_ch_enable_lo, 0xff, "r0x06");
        assert_eq!(regs.analog_ch_enable_hi, None, "no r0x07 for 8ch");
        assert_eq!(regs.analog_decimation, 0x20, "r0x0a: analog decimation");
        assert_eq!(regs.digital_divider_lo, 0x10, "r0x0b: digital divider lo");
        assert_eq!(regs.digital_divider_hi, 0x00, "r0x0c: digital divider hi");
        assert_eq!(regs.adc_config, Some(0x00), "r0x1a: ADC config");
        assert_eq!(regs.analog_dec_upper, Some(0x00), "r0x1b: upper dec");
        assert_eq!(
            regs.mix_ratio_a, 0x18,
            "r0x0d: mix ratio A (analog GPIF words)"
        );
        assert_eq!(
            regs.mix_ratio_b, 0x08,
            "r0x0e: mix ratio B (digital GPIF words)"
        );
        assert_eq!(regs.capture_config_2, 0x02, "r0x12: ADC type");
        assert_eq!(regs.capture_control, 0x00, "r0x13: capture control");
        assert_eq!(regs.gpif_bus_width, 0x10, "r0x14: GPIF bus width");

        // Derived values.
        assert_eq!(regs.digital_ratio, 1, "digital_ratio");
        assert_eq!(regs.low_speed_divider, 1, "low_speed_divider");
        assert_eq!(regs.gpif_packet_size, 4, "gpif_packet_size (32-bit)");
        assert_eq!(regs.analog_group_size, 8, "analog_group_size");
    }

    #[test]
    fn capture_registers_pcap_to_register_pairs() {
        // Verify to_register_pairs produces the exact pcap byte sequence.
        let config = CaptureConfig {
            sample_rate: 12_500_000,
            digital_channels: (0..8).collect(),
            analog_channels: (0..8).collect(),
            total_device_channels: 8,
            board_revision: 1,
            is_pro: true,
            base_digital_rate: 500_000_000,
            base_analog_rate: 100_000_000,
            ..CaptureConfig::default()
        };
        let pairs = config.capture_registers().to_register_pairs();
        let expected: Vec<(u8, u8)> = vec![
            (0x00, 0x00),
            (0x08, 0xff),
            (0x06, 0xff),
            (0x0a, 0x20),
            (0x0b, 0x10),
            (0x0c, 0x00),
            (0x1a, 0x00),
            (0x1b, 0x00),
            (0x0d, 0x18),
            (0x0e, 0x08),
            (0x12, 0x02),
            (0x13, 0x00),
            (0x14, 0x10),
        ];
        assert_eq!(pairs, expected);
    }

    #[test]
    fn capture_registers_digital_only() {
        // Digital-only mode: all 8 digital, no analog.
        let config = CaptureConfig {
            sample_rate: 12_500_000,
            digital_channels: (0..8).collect(),
            analog_channels: vec![],
            total_device_channels: 8,
            board_revision: 1,
            is_pro: true,
            base_digital_rate: 500_000_000,
            base_analog_rate: 100_000_000,
            ..CaptureConfig::default()
        };
        let regs = compute_capture_registers(&config);

        assert_eq!(regs.digital_ch_enable_lo, 0xff);
        assert_eq!(regs.analog_ch_enable_lo, 0x00, "no analog channels");
        assert_eq!(regs.analog_decimation, 0x00, "no analog -> no decimation");
        assert_eq!(regs.mix_ratio_a, 0x00, "no analog -> mix_ratio_a = 0");
        assert_eq!(regs.mix_ratio_b, 0x08, "digital GPIF words = 1 * 8 = 8");
        assert_eq!(regs.digital_ratio, 1);
    }

    #[test]
    fn capture_registers_channel_subset() {
        // Only channels 0-3 digital, channels 0,1 analog.
        let config = CaptureConfig {
            sample_rate: 12_500_000,
            digital_channels: vec![0, 1, 2, 3],
            analog_channels: vec![0, 1],
            total_device_channels: 8,
            board_revision: 1,
            is_pro: true,
            base_digital_rate: 500_000_000,
            base_analog_rate: 100_000_000,
            ..CaptureConfig::default()
        };
        let regs = compute_capture_registers(&config);

        assert_eq!(regs.digital_ch_enable_lo, 0x0f, "channels 0-3 = 0x0f");
        assert_eq!(regs.analog_ch_enable_lo, 0x03, "channels 0,1 = 0x03");
        // Mix ratio A = (12 * 2) / 4 = 6
        assert_eq!(regs.mix_ratio_a, 6, "analog GPIF words = (12*2)/4 = 6");
        // Mix ratio B = 1 * 4 = 4
        assert_eq!(regs.mix_ratio_b, 4, "digital GPIF words = 1*4 = 4");
    }

    #[test]
    fn capture_registers_16ch_device() {
        let config = CaptureConfig {
            sample_rate: 12_500_000,
            digital_channels: vec![0, 1, 8, 9],
            analog_channels: vec![0, 8],
            total_device_channels: 16,
            board_revision: 1,
            is_pro: true,
            base_digital_rate: 500_000_000,
            base_analog_rate: 100_000_000,
            ..CaptureConfig::default()
        };
        let regs = compute_capture_registers(&config);

        assert_eq!(
            regs.digital_ch_enable_lo,
            Some(0x03).map(|_| regs.digital_ch_enable_lo).unwrap()
        );
        // Channels 0,1 -> lo byte = 0x03; channels 8,9 -> hi byte = 0x03
        assert_eq!(regs.digital_ch_enable_lo, 0x03);
        assert_eq!(regs.digital_ch_enable_hi, Some(0x03));
        assert_eq!(regs.analog_ch_enable_lo, 0x01); // channel 0
        assert_eq!(regs.analog_ch_enable_hi, Some(0x01)); // channel 8
    }

    // -------------------------------------------------------------------
    // disaggregator_settings tests
    // -------------------------------------------------------------------

    #[test]
    fn disaggregator_pcap_reference() {
        // The disaggregator for the pcap reference config must
        // produce the same cycle structure as the hardcoded values.
        let config = CaptureConfig {
            sample_rate: 12_500_000,
            digital_channels: (0..8).collect(),
            analog_channels: (0..8).collect(),
            total_device_channels: 8,
            board_revision: 1,
            is_pro: true,
            base_digital_rate: 500_000_000,
            base_analog_rate: 100_000_000,
            ..CaptureConfig::default()
        };
        let disagg = config.disaggregator().unwrap();

        assert_eq!(disagg.num_digital(), 8);
        assert_eq!(disagg.num_analog(), 8);
        assert_eq!(disagg.low_speed_divider(), 1);

        // Verify cycle size: 32 digital + 96 analog = 128 bytes.
        // digital: 1 ratio * 4 bytes * 8 channels = 32
        // analog: 12 bytes/channel * 8 channels = 96
        assert_eq!(disagg.cycle_bytes(), 128);
    }

    #[test]
    fn disaggregator_digital_only() {
        let config = CaptureConfig {
            sample_rate: 12_500_000,
            digital_channels: (0..8).collect(),
            analog_channels: vec![],
            total_device_channels: 8,
            board_revision: 1,
            is_pro: true,
            base_digital_rate: 500_000_000,
            base_analog_rate: 100_000_000,
            ..CaptureConfig::default()
        };
        let disagg = config.disaggregator().unwrap();

        assert_eq!(disagg.num_digital(), 8);
        assert_eq!(disagg.num_analog(), 0);

        // Cycle is digital-only: 1 * 4 * 8 = 32 bytes.
        assert_eq!(disagg.cycle_bytes(), 32);
    }

    #[test]
    fn disaggregator_channel_subset() {
        let config = CaptureConfig {
            sample_rate: 12_500_000,
            digital_channels: vec![0, 1, 2, 3],
            analog_channels: vec![0, 1],
            total_device_channels: 8,
            board_revision: 1,
            is_pro: true,
            base_digital_rate: 500_000_000,
            base_analog_rate: 100_000_000,
            ..CaptureConfig::default()
        };
        let disagg = config.disaggregator().unwrap();

        assert_eq!(disagg.num_digital(), 4);
        assert_eq!(disagg.num_analog(), 2);

        // Digital: 1 * 4 * 4 = 16 bytes.
        // Analog: 12 * 2 = 24 bytes.
        // Total: 40 bytes.
        assert_eq!(disagg.cycle_bytes(), 40);
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
    fn afe_default_vga_config() {
        // Default: LG preamp, full power, filter=4, ladder=0
        // Expected: bits[3:0]=0, bit[4]=0(LG), bits[8:6]=4(0b100), bit[10]=1(power)
        // = 0b0000_0101_0000_0000 = 0x0500 ... wait:
        // filter=4 => bits [8:6] = 0b100 => bit8=1, bit7=0, bit6=0
        // power=1 => bit[10] = 1
        // = (1 << 10) | (4 << 6) = 0x400 | 0x100 = 0x500
        let afe = AfeChannelConfig::default();
        assert_eq!(afe.encode_vga_config(), 0x0500);
    }

    #[test]
    fn afe_vga_encode_high_gain_full_ladder() {
        let afe = AfeChannelConfig {
            attn1: true,
            attn2: false,
            preamp: true,
            power: true,
            filter: 4,
            ladder: -20,
            dac_offset: 0.0,
        };
        // ladder=-20 => (-(-20))/2 = 10 => bits[3:0] = 10 = 0x0A
        // preamp=HG => bit[4] = 1
        // filter=4 => bits[8:6] = 0b100
        // power=1 => bit[10] = 1
        // = 0x0A | 0x10 | 0x100 | 0x400 = 0x051A
        assert_eq!(afe.encode_vga_config(), 0x051A);
    }

    #[test]
    fn afe_vga_roundtrip() {
        let original = AfeChannelConfig {
            attn1: false,
            attn2: false,
            preamp: true,
            power: true,
            filter: 4,
            ladder: -12,
            dac_offset: 0.0,
        };
        let spi = original.encode_vga_config();
        let decoded = AfeChannelConfig::from_vga_config(spi);
        // Attenuators and dac_offset are not in VGA word, so they default
        assert_eq!(decoded.preamp, original.preamp);
        assert_eq!(decoded.power, original.power);
        assert_eq!(decoded.filter, original.filter);
        assert_eq!(decoded.ladder, original.ladder);
    }

    #[test]
    fn afe_config_index_roundtrip_all_66() {
        for i in 0..TOTAL_AFE_CONFIGURATIONS {
            let config = AfeChannelConfig::from_config_index(i)
                .unwrap_or_else(|| panic!("from_config_index({}) returned None", i));
            let idx = config
                .config_index()
                .unwrap_or_else(|| panic!("config_index() returned None for index {}", i));
            assert_eq!(idx, i, "roundtrip failed for index {}", i);
        }
    }

    #[test]
    fn afe_config_index_out_of_range() {
        assert!(AfeChannelConfig::from_config_index(66).is_none());
        assert!(AfeChannelConfig::from_config_index(100).is_none());
    }

    #[test]
    fn afe_config_index_invalid_attn_combo() {
        let afe = AfeChannelConfig {
            attn1: false,
            attn2: true, // invalid: attn2 without attn1
            ..AfeChannelConfig::default()
        };
        assert!(afe.config_index().is_none());
    }

    #[test]
    fn afe_vga_encode_all_configs_valid() {
        // Every config in the 66-entry table should produce a valid VGA word
        for i in 0..TOTAL_AFE_CONFIGURATIONS {
            let config = AfeChannelConfig::from_config_index(i).unwrap();
            let _spi = config.encode_vga_config(); // should not panic
        }
    }

    #[test]
    #[should_panic(expected = "ladder must be 0 to -20")]
    fn afe_vga_encode_rejects_positive_ladder() {
        let afe = AfeChannelConfig {
            ladder: 2,
            ..AfeChannelConfig::default()
        };
        afe.encode_vga_config();
    }

    #[test]
    #[should_panic(expected = "ladder must be even")]
    fn afe_vga_encode_rejects_odd_ladder() {
        let afe = AfeChannelConfig {
            ladder: -3,
            ..AfeChannelConfig::default()
        };
        afe.encode_vga_config();
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
            feature_class: DeviceFeatureClass::Pro,
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
            feature_class: DeviceFeatureClass::Pro,
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
            feature_class: DeviceFeatureClass::Pro,
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
            feature_class: DeviceFeatureClass::Pro,
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
            feature_class: DeviceFeatureClass::Pro,
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
            feature_class: DeviceFeatureClass::Pro,
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
            feature_class: DeviceFeatureClass::Pro,
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
            feature_class: DeviceFeatureClass::Pro,
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
            feature_class: DeviceFeatureClass::Pro,
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
            feature_class: DeviceFeatureClass::Mso,
            ..CaptureConfig::default()
        };
        config_ok.validate_bandwidth().unwrap();

        let config_ok_max = CaptureConfig {
            samples_per_chunk: 6016,
            feature_class: DeviceFeatureClass::Mso,
            ..config_ok.clone()
        };
        config_ok_max.validate_bandwidth().unwrap();

        let config_too_big = CaptureConfig {
            samples_per_chunk: 6017,
            feature_class: DeviceFeatureClass::Mso,
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
            feature_class: DeviceFeatureClass::Pro,
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
            feature_class: DeviceFeatureClass::Pro,
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
            feature_class: DeviceFeatureClass::Pro,
            ..CaptureConfig::default()
        };
        let err = config.validate_bandwidth().unwrap_err();
        assert!(err.to_string().contains("bandwidth"), "{}", err);
    }

    // -------------------------------------------------------------------
    // StreamingCapture tests
    // -------------------------------------------------------------------

    #[test]
    fn streaming_capture_basic_lifecycle() {
        use crate::transfer_queue::TransferConfig;
        use saleae_usb::mock_stream::{MockCompletion, MockStreamReader};

        let mut reader = MockStreamReader::new();
        // Pre-load 3 transfers worth of data.
        for i in 0..3u8 {
            reader.push_completion(MockCompletion::Data(vec![i; 1024]));
        }

        let config = TransferConfig {
            transfer_size: 1024,
            queue_depth: 4,
            block_size: 4096,
        };

        let mut capture = StreamingCapture::new(Box::new(reader), config, 0x82);
        assert!(!capture.is_active());

        capture.start().unwrap();
        assert!(capture.is_active());
        assert_eq!(capture.pending(), 4);

        // Read first transfer.
        let buf = capture
            .next_transfer(std::time::Duration::from_secs(1))
            .unwrap();
        assert_eq!(buf[0], 0);
        assert_eq!(buf.len(), 1024);
        assert_eq!(capture.total_bytes(), 1024);
        assert_eq!(capture.total_transfers(), 1);
        assert_eq!(capture.pending(), 3);

        // Recycle it.
        capture.recycle(buf).unwrap();
        assert_eq!(capture.pending(), 4);

        // Read second.
        let buf = capture
            .next_transfer(std::time::Duration::from_secs(1))
            .unwrap();
        assert_eq!(buf[0], 1);
        capture.recycle(buf).unwrap();

        // Read third.
        let buf = capture
            .next_transfer(std::time::Duration::from_secs(1))
            .unwrap();
        assert_eq!(buf[0], 2);
        capture.recycle(buf).unwrap();

        assert_eq!(capture.total_bytes(), 3072);
        assert_eq!(capture.total_transfers(), 3);

        capture.stop().unwrap();
        assert!(!capture.is_active());
    }

    #[test]
    fn streaming_capture_timeout_propagates() {
        use crate::transfer_queue::TransferConfig;
        use saleae_usb::mock_stream::MockStreamReader;

        let reader = MockStreamReader::new();
        let config = TransferConfig {
            transfer_size: 1024,
            queue_depth: 2,
            block_size: 4096,
        };

        let mut capture = StreamingCapture::new(Box::new(reader), config, 0x82);
        capture.start().unwrap();

        // No completions queued, should get a timeout error.
        let err = capture
            .next_transfer(std::time::Duration::from_millis(100))
            .unwrap_err();
        assert!(
            err.to_string().contains("timed out"),
            "expected timeout error, got: {err}"
        );
    }

    #[test]
    fn streaming_capture_stop_is_idempotent() {
        use crate::transfer_queue::TransferConfig;
        use saleae_usb::mock_stream::MockStreamReader;

        let reader = MockStreamReader::new();
        let config = TransferConfig {
            transfer_size: 1024,
            queue_depth: 2,
            block_size: 4096,
        };

        let mut capture = StreamingCapture::new(Box::new(reader), config, 0x82);
        capture.start().unwrap();
        capture.stop().unwrap();
        // Second stop should be a no-op.
        capture.stop().unwrap();
    }

    #[test]
    fn streaming_capture_next_before_start_errors() {
        use crate::transfer_queue::TransferConfig;
        use saleae_usb::mock_stream::MockStreamReader;

        let reader = MockStreamReader::new();
        let config = TransferConfig {
            transfer_size: 1024,
            queue_depth: 2,
            block_size: 4096,
        };

        let mut capture = StreamingCapture::new(Box::new(reader), config, 0x82);
        let err = capture
            .next_transfer(std::time::Duration::from_secs(1))
            .unwrap_err();
        assert!(
            err.to_string().contains("idle"),
            "expected idle state error, got: {err}"
        );
    }

    #[test]
    fn streaming_capture_error_propagates() {
        use crate::transfer_queue::TransferConfig;
        use saleae_usb::mock_stream::{MockCompletion, MockStreamReader};
        use saleae_usb::UsbError;

        let mut reader = MockStreamReader::new();
        reader.push_completion(MockCompletion::Error(UsbError::BulkTransfer {
            endpoint: 0x82,
            context: "stall".into(),
        }));

        let config = TransferConfig {
            transfer_size: 1024,
            queue_depth: 2,
            block_size: 4096,
        };

        let mut capture = StreamingCapture::new(Box::new(reader), config, 0x82);
        capture.start().unwrap();

        let err = capture
            .next_transfer(std::time::Duration::from_secs(1))
            .unwrap_err();
        assert!(
            err.to_string().contains("stall"),
            "expected stall error, got: {err}"
        );
    }

    // -------------------------------------------------------------------
    // run_capture_streaming tests
    // -------------------------------------------------------------------

    /// Queue mock responses for the full run_capture_streaming sequence:
    /// configure_capture + start_sampling + trigger write + stop write + stop_stream.
    fn queue_full_capture_sequence(mock: &MockTransport, config: &CaptureConfig) {
        // 1. configure_capture: WriteRegisters ACK + ReadRegisters readback
        queue_configure_acks(mock, config);

        // 2. start_sampling: send_command (OUT) + recv_response (IN)
        mock.push_bulk_in_response(ep::BULK_IN, vec![0x07, 0x00]);

        // 3. write_registers(&[(0x00, 0x01)]) trigger: OUT + IN ACK
        mock.push_bulk_in_response(ep::BULK_IN, vec![0x00]);

        // 4. write_registers(&[(0x00, 0x00)]) stop trigger: OUT + IN ACK
        mock.push_bulk_in_response(ep::BULK_IN, vec![0x00]);

        // 5. stop_stream: OUT + IN response
        mock.push_bulk_in_response(ep::BULK_IN, vec![0x07, 0x00]);
    }

    #[test]
    fn run_capture_streaming_basic() {
        use saleae_usb::mock_stream::{MockCompletion, MockStreamReader};

        let config = CaptureConfig::default();
        let mock = std::sync::Arc::new(MockTransport::new());
        let mut channel = mock_channel(&mock);

        queue_full_capture_sequence(&mock, &config);

        // Prepare a mock stream reader with 3 data chunks.
        let mut reader = MockStreamReader::new();
        for i in 0..3u8 {
            reader.push_completion(MockCompletion::Data(vec![i; 512]));
        }

        let result = run_capture_streaming(
            &mut channel,
            &config,
            std::time::Duration::from_millis(50),
            Box::new(reader),
            None,
        )
        .unwrap();

        // Should have read 3 chunks of 512 bytes.
        assert_eq!(result.bytes_read(), 3 * 512);
        assert_eq!(result.read_count, 3);
        // After timeout on 4th read (no more data), loop should exit.
        // completed_normally may be false due to timeout, which is acceptable.
    }

    #[test]
    fn run_capture_streaming_empty() {
        use saleae_usb::mock_stream::MockStreamReader;

        let config = CaptureConfig::default();
        let mock = std::sync::Arc::new(MockTransport::new());
        let mut channel = mock_channel(&mock);

        queue_full_capture_sequence(&mock, &config);

        // No data in stream reader -- will timeout immediately.
        let reader = MockStreamReader::new();

        let result = run_capture_streaming(
            &mut channel,
            &config,
            std::time::Duration::from_millis(10),
            Box::new(reader),
            None,
        )
        .unwrap();

        assert_eq!(result.bytes_read(), 0);
        assert_eq!(result.read_count, 0);
    }

    #[test]
    fn run_capture_streaming_error_stops_early() {
        use saleae_usb::mock_stream::{MockCompletion, MockStreamReader};
        use saleae_usb::UsbError;

        let config = CaptureConfig::default();
        let mock = std::sync::Arc::new(MockTransport::new());
        let mut channel = mock_channel(&mock);

        queue_full_capture_sequence(&mock, &config);

        // Data then error.
        let mut reader = MockStreamReader::new();
        reader.push_completion(MockCompletion::Data(vec![0xAA; 256]));
        reader.push_completion(MockCompletion::Error(UsbError::BulkTransfer {
            endpoint: 0x82,
            context: "device stalled".into(),
        }));

        let result = run_capture_streaming(
            &mut channel,
            &config,
            std::time::Duration::from_secs(5), // long duration, but error stops early
            Box::new(reader),
            None,
        )
        .unwrap();

        assert_eq!(result.bytes_read(), 256);
        assert_eq!(result.read_count, 1);
        assert!(!result.completed_normally);
    }
}
