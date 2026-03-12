//! EP2 stream disaggregator for Saleae Logic Pro devices.
//!
//! Decodes raw interleaved USB data from EP2 into per-channel digital and
//! analog sample streams. The interleaving format was reverse-engineered
//! from `libgraph_server_shared.so` v2.4.40, specifically:
//!
//! - `FUN_01bf1190` (Pro device deinterleaver, types 4/5/6)
//! - `FUN_01d3bad0` (ContractReadData, low-speed decimation)
//! - `DeviceCalculateMixRate` at ELF VA 0x1c19fa0
//!
//! See `re/notes/disaggregator-interleaving.md` for full analysis.
//!
//! # Data flow
//!
//! ```text
//! EP2 raw bytes
//!   -> ContractReadData (decimate by LowSpeedDivider)
//!   -> Disaggregator (split interleaved cycles into per-channel streams)
//!   -> DisaggregatorOutput { digital: Vec<Vec<u8>>, analog: Vec<Vec<i16>> }
//! ```

use crate::error::{DriverError, DriverResult};

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

/// Configuration for the disaggregator, matching the binary's
/// `DisaggregatorSettingsData` inner object layout.
///
/// Constructed either manually or via [`compute_mix_rate`].
#[derive(Debug, Clone)]
pub struct DisaggregatorSettings {
    /// Number of digital packet groups per interleave cycle.
    /// From `settings+0x10` in the binary. Typically `digital_rate / gcd`.
    pub digital_ratio: u32,

    /// Bytes per digital packet (2 for 16-bit GPIF, 4 for 32-bit GPIF).
    /// From `settings+0x14` in the binary.
    pub digital_packet_size: u32,

    /// Number of analog samples per group (4 for 16-bit GPIF + 12-bit ADC,
    /// 8 for 32-bit GPIF + 12-bit ADC).
    /// From `settings+0x18` in the binary.
    pub analog_group_size: u32,

    /// Bytes per pair of analog samples (3 for 12-bit ADC, 5 for 10-bit ADC).
    /// For 12-bit: each sample is 1.5 bytes; this field stores 3 = bytes
    /// per 2 samples. Combined with `analog_group_size`, the per-channel
    /// byte count is `analog_bytes_per_sample * analog_group_size / 2`.
    /// From `settings+0x1c` in the binary.
    pub analog_bytes_per_sample: u32,

    /// Which digital channels are enabled (true = enabled).
    /// Index corresponds to channel number.
    pub digital_channels: Vec<bool>,

    /// Which analog channels are enabled (true = enabled).
    /// Index corresponds to channel number.
    pub analog_channels: Vec<bool>,

    /// Low-speed divider for ContractReadData. Valid values: 1, 2, 4, 8.
    /// When > 1, every Nth byte is kept and the rest discarded before
    /// disaggregation.
    pub low_speed_divider: u32,
}

impl DisaggregatorSettings {
    /// Count of enabled digital channels.
    pub fn num_digital_channels(&self) -> usize {
        self.digital_channels.iter().filter(|&&e| e).count()
    }

    /// Count of enabled analog channels.
    pub fn num_analog_channels(&self) -> usize {
        self.analog_channels.iter().filter(|&&e| e).count()
    }

    /// Bytes consumed by the digital portion of one interleave cycle.
    pub fn digital_group_bytes(&self) -> usize {
        self.digital_ratio as usize
            * self.digital_packet_size as usize
            * self.num_digital_channels()
    }

    /// Bytes consumed by one analog channel's group.
    ///
    /// For 12-bit ADC, samples pack as 3 bytes per 2 samples:
    ///   - 8-sample group = 12 bytes (32-bit GPIF)
    ///   - 4-sample group = 6 bytes (16-bit GPIF)
    ///
    /// The formula is `analog_bytes_per_sample * analog_group_size / 2`
    /// because `analog_bytes_per_sample` (3 for 12-bit) encodes the bytes
    /// per *pair* of samples.
    pub fn analog_channel_bytes(&self) -> usize {
        // 12-bit ADC: 3 bytes per 2 samples. analog_group_size counts
        // individual samples, so divide by 2 to get pairs.
        (self.analog_bytes_per_sample as usize * self.analog_group_size as usize) / 2
    }

    /// Bytes consumed by the analog portion of one interleave cycle
    /// (all enabled analog channels).
    pub fn analog_group_bytes(&self) -> usize {
        self.analog_channel_bytes() * self.num_analog_channels()
    }

    /// Total bytes in one full interleave cycle.
    pub fn full_cycle_size(&self) -> usize {
        self.digital_group_bytes() + self.analog_group_bytes()
    }

    /// Validate that settings are internally consistent.
    fn validate(&self) -> DriverResult<()> {
        if self.digital_packet_size != 2 && self.digital_packet_size != 4 {
            return Err(DriverError::InvalidCaptureConfig {
                reason: format!(
                    "digital_packet_size must be 2 or 4, got {}",
                    self.digital_packet_size
                ),
            });
        }
        if self.analog_group_size != 0 && self.analog_group_size != 4 && self.analog_group_size != 8
        {
            return Err(DriverError::InvalidCaptureConfig {
                reason: format!(
                    "analog_group_size must be 0, 4, or 8, got {}",
                    self.analog_group_size
                ),
            });
        }
        if !matches!(self.low_speed_divider, 1 | 2 | 4 | 8) {
            return Err(DriverError::InvalidCaptureConfig {
                reason: format!(
                    "low_speed_divider must be 1, 2, 4, or 8, got {}",
                    self.low_speed_divider
                ),
            });
        }
        if self.full_cycle_size() == 0 {
            return Err(DriverError::InvalidCaptureConfig {
                reason: "full_cycle_size is 0 (no channels enabled)".into(),
            });
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Output
// ---------------------------------------------------------------------------

/// Output of the disaggregator: per-channel sample streams.
#[derive(Debug, Clone)]
pub struct DisaggregatorOutput {
    /// Per-channel digital data. Each inner `Vec<u8>` contains packed
    /// digital samples (1 bit per sample, 8 samples per byte). Channels
    /// are ordered by their index among enabled channels.
    pub digital: Vec<Vec<u8>>,

    /// Per-channel analog data. Each inner `Vec<i16>` contains 12-bit
    /// sign-extended samples. Channels are ordered by their index among
    /// enabled analog channels.
    pub analog: Vec<Vec<i16>>,
}

// ---------------------------------------------------------------------------
// Disaggregator
// ---------------------------------------------------------------------------

/// Stateless disaggregator that splits interleaved EP2 data into
/// per-channel streams.
///
/// Construct with validated settings, then call [`process`] for each
/// buffer of EP2 data.
pub struct Disaggregator {
    settings: DisaggregatorSettings,
}

impl Disaggregator {
    /// Create a new disaggregator with the given settings.
    ///
    /// Returns an error if settings are invalid.
    pub fn new(settings: DisaggregatorSettings) -> DriverResult<Self> {
        settings.validate()?;
        Ok(Self { settings })
    }

    /// Process one buffer of raw EP2 data.
    ///
    /// Applies ContractReadData (if `low_speed_divider > 1`), then
    /// splits the interleaved cycles into per-channel streams.
    ///
    /// The input length (after contraction) must be a multiple of
    /// [`DisaggregatorSettings::full_cycle_size`].
    pub fn process(&self, data: &[u8]) -> DriverResult<DisaggregatorOutput> {
        let contracted = if self.settings.low_speed_divider > 1 {
            contract_read_data(data, self.settings.low_speed_divider)
        } else {
            data.to_vec()
        };

        let cycle_size = self.settings.full_cycle_size();
        if contracted.len() % cycle_size != 0 {
            return Err(DriverError::InvalidCaptureConfig {
                reason: format!(
                    "data length {} is not a multiple of full_cycle_size {} \
                     (after contraction by divider {})",
                    contracted.len(),
                    cycle_size,
                    self.settings.low_speed_divider,
                ),
            });
        }

        let num_cycles = contracted.len() / cycle_size;
        let num_digital = self.settings.num_digital_channels();
        let num_analog = self.settings.num_analog_channels();
        let digital_group_bytes = self.settings.digital_group_bytes();

        // Pre-allocate output buffers.
        let digital_samples_per_cycle =
            self.settings.digital_ratio as usize * self.settings.digital_packet_size as usize;
        let mut digital: Vec<Vec<u8>> =
            vec![Vec::with_capacity(num_cycles * digital_samples_per_cycle); num_digital];

        let analog_samples_per_cycle = self.settings.analog_group_size as usize;
        let mut analog: Vec<Vec<i16>> =
            vec![Vec::with_capacity(num_cycles * analog_samples_per_cycle); num_analog];

        // Process each interleave cycle.
        for cycle_idx in 0..num_cycles {
            let cycle_start = cycle_idx * cycle_size;
            let digital_data = &contracted[cycle_start..cycle_start + digital_group_bytes];
            let analog_data =
                &contracted[cycle_start + digital_group_bytes..cycle_start + cycle_size];

            // Extract digital channels.
            if num_digital > 0 {
                match self.settings.digital_packet_size {
                    4 => extract_digital_32bit(
                        digital_data,
                        self.settings.digital_ratio as usize,
                        num_digital,
                        &mut digital,
                    ),
                    2 => extract_digital_16bit(
                        digital_data,
                        self.settings.digital_ratio as usize,
                        num_digital,
                        &mut digital,
                    ),
                    _ => unreachable!("validated in new()"),
                }
            }

            // Extract analog channels.
            if num_analog > 0 {
                match self.settings.analog_group_size {
                    8 => extract_analog_12bit_8samples(analog_data, num_analog, &mut analog),
                    4 => extract_analog_12bit_4samples(analog_data, num_analog, &mut analog),
                    _ => unreachable!("validated: analog_group_size is 4 or 8 when analog enabled"),
                }
            }
        }

        Ok(DisaggregatorOutput { digital, analog })
    }
}

// ---------------------------------------------------------------------------
// ContractReadData
// ---------------------------------------------------------------------------

/// Decimate raw EP2 data by keeping every Nth byte, where N is the
/// low-speed divider. Mirrors `ContractReadData` at ELF VA 0x1c3bad0.
///
/// When `divider == 1`, this is a no-op (but callers should skip it).
fn contract_read_data(data: &[u8], divider: u32) -> Vec<u8> {
    debug_assert!(matches!(divider, 2 | 4 | 8));
    let step = divider as usize;
    data.iter().step_by(step).copied().collect()
}

// ---------------------------------------------------------------------------
// Digital extraction
// ---------------------------------------------------------------------------

/// Extract digital channels from a 32-bit GPIF data block.
///
/// For each of `digital_ratio` iterations, reads one u32 per enabled
/// channel, applies bswap32, and appends the 4 swapped bytes to the
/// channel's output buffer.
///
/// Layout: `[ratio iterations] x [num_channels] x [4 bytes]`
fn extract_digital_32bit(
    data: &[u8],
    digital_ratio: usize,
    num_channels: usize,
    output: &mut [Vec<u8>],
) {
    let mut offset = 0;
    for _ratio in 0..digital_ratio {
        for out_ch in output.iter_mut().take(num_channels) {
            let word = u32::from_le_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ]);
            let swapped = word.swap_bytes();
            out_ch.extend_from_slice(&swapped.to_ne_bytes());
            offset += 4;
        }
    }
}

/// Extract digital channels from a 16-bit GPIF data block.
///
/// For each of `digital_ratio` iterations, reads 2 bytes per enabled
/// channel, byte-swaps them (FPGA-to-host correction), and appends to
/// the channel's output buffer.
///
/// Layout: `[ratio iterations] x [num_channels] x [2 bytes]`
fn extract_digital_16bit(
    data: &[u8],
    digital_ratio: usize,
    num_channels: usize,
    output: &mut [Vec<u8>],
) {
    let mut offset = 0;
    for _ratio in 0..digital_ratio {
        for out_ch in output.iter_mut().take(num_channels) {
            // Byte-swap: write byte1 then byte0 (matches decompilation).
            out_ch.push(data[offset + 1]);
            out_ch.push(data[offset]);
            offset += 2;
        }
    }
}

// ---------------------------------------------------------------------------
// Analog extraction
// ---------------------------------------------------------------------------

/// Extract 8 x 12-bit analog samples from 12 bytes (32-bit GPIF mode).
///
/// Uses the overlapping u64 read pattern from the binary's SSE-optimized
/// path. Reads bytes 0..7 as u64 LE and bytes 4..11 as u64 LE, then
/// extracts 8 x 12-bit values by shifting and masking, followed by
/// arithmetic right shift by 4 to sign-extend.
///
/// Layout per channel: 12 bytes -> 8 x i16 samples.
fn extract_analog_12bit_8samples(data: &[u8], num_channels: usize, output: &mut [Vec<i16>]) {
    let bytes_per_channel = 12;
    let mut offset = 0;
    for out_ch in output.iter_mut().take(num_channels) {
        let base = &data[offset..offset + bytes_per_channel];

        // Overlapping u64 reads, matching the decompilation.
        let lo = u64::from_le_bytes([
            base[0], base[1], base[2], base[3], base[4], base[5], base[6], base[7],
        ]);
        let hi = u64::from_le_bytes([
            base[4], base[5], base[6], base[7], base[8], base[9], base[10], base[11],
        ]);

        // Extract 8 x 12-bit samples. Each is left-shifted into a 16-bit
        // container then arithmetic-right-shifted by 4 to sign-extend.
        //
        // The pattern from the binary:
        //   vec[0] = (i16)(lo << 4)      >> 4   bits [0:11]
        //   vec[1] = (i16)(lo >> 8)       >> 4   bits [12:23]  (actually 8:19 with mask)
        //   vec[2] = (i16)(lo >> 20)      >> 4   bits [20:31]
        //   vec[3] = (i16)(lo >> 32)      >> 4   bits [32:43]
        //   vec[4] = (i16)(hi >> 12)      >> 4   bits [44:55]
        //   vec[5] = (i16)(hi >> 24)      >> 4   bits [56:67]
        //   vec[6] = (i16)(hi >> 36)      >> 4   bits [68:79]
        //   vec[7] = (i16)(hi >> 48)      >> 4   bits [80:91]
        //
        // The truncation to i16 keeps only the low 16 bits before the
        // arithmetic right shift sign-extends from bit 15.

        let samples: [i16; 8] = [
            ((lo << 4) as u16 as i16) >> 4,
            ((lo >> 8) as u16 as i16) >> 4,
            ((lo >> 20) as u16 as i16) >> 4,
            ((lo >> 32) as u16 as i16) >> 4,
            ((hi >> 12) as u16 as i16) >> 4,
            ((hi >> 24) as u16 as i16) >> 4,
            ((hi >> 36) as u16 as i16) >> 4,
            ((hi >> 48) as u16 as i16) >> 4,
        ];

        out_ch.extend_from_slice(&samples);
        offset += bytes_per_channel;
    }
}

/// Extract 4 x 12-bit analog samples from 6 bytes (16-bit GPIF mode).
///
/// Uses the same bit-extraction pattern as the 8-sample path: read the
/// 6 bytes as a u64 (zero-extended), then extract 4 x 12-bit values by
/// shifting and truncating to i16, followed by arithmetic right shift
/// by 4 to sign-extend.
///
/// The decompilation shows the binary splits this into a u32 + i16 read,
/// but the bit-level logic is equivalent to a u64-based extraction.
/// The u64 approach is simpler and avoids the split-register complexity.
///
/// Layout per channel: 6 bytes -> 4 x i16 samples.
fn extract_analog_12bit_4samples(data: &[u8], num_channels: usize, output: &mut [Vec<i16>]) {
    let bytes_per_channel = 6;
    let mut offset = 0;
    for out_ch in output.iter_mut().take(num_channels) {
        let base = &data[offset..offset + bytes_per_channel];

        // Zero-extend 6 bytes into a u64.
        let val = u64::from_le_bytes([base[0], base[1], base[2], base[3], base[4], base[5], 0, 0]);

        // Same extraction pattern as the 8-sample path, but only 4 samples.
        // Each 12-bit value is left-shifted to fill the upper bits of an i16,
        // then arithmetic-right-shifted by 4 to sign-extend.
        let samples: [i16; 4] = [
            ((val << 4) as u16 as i16) >> 4,  // bits [0:11]
            ((val >> 8) as u16 as i16) >> 4,  // bits [12:23]
            ((val >> 20) as u16 as i16) >> 4, // bits [24:35]
            ((val >> 32) as u16 as i16) >> 4, // bits [36:47]
        ];

        out_ch.extend_from_slice(&samples);
        offset += bytes_per_channel;
    }
}

// ---------------------------------------------------------------------------
// Mix rate computation
// ---------------------------------------------------------------------------

/// Compute the GCD of two positive integers (Euclidean algorithm).
/// Mirrors the helper at ELF VA 0x1c47140.
fn gcd(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a
}

/// Compute disaggregator settings from capture parameters.
///
/// Implements `DeviceCalculateMixRate` (ELF VA 0x1c19fa0). Given the
/// digital and analog sample rates plus hardware parameters, computes
/// the interleaving ratio and group sizes.
///
/// # Arguments
///
/// - `digital_rate`: Digital sample rate in Hz (0 if digital-only capture
///   with no analog).
/// - `analog_rate`: Analog sample rate in Hz (0 if no analog channels).
/// - `gpif_width`: GPIF bus width in bits (16 or 32).
/// - `adc_bits`: ADC resolution in bits (10 or 12).
/// - `digital_channels`: Which digital channels are enabled.
/// - `analog_channels`: Which analog channels are enabled.
/// - `low_speed_divider`: Low-speed divider (1, 2, 4, or 8).
///
/// # Errors
///
/// Returns an error for unsupported GPIF width / ADC bit combinations.
pub fn compute_mix_rate(
    digital_rate: u64,
    analog_rate: u64,
    gpif_width: u32,
    adc_bits: u32,
    digital_channels: Vec<bool>,
    analog_channels: Vec<bool>,
    low_speed_divider: u32,
) -> DriverResult<DisaggregatorSettings> {
    // Determine analog group parameters from GPIF width + ADC bits.
    // Table from DeviceCalculateMixRate:
    //   16-bit GPIF + 10-bit ADC -> group_size=8, bytes_per_sample=5
    //   16-bit GPIF + 12-bit ADC -> group_size=4, bytes_per_sample=3
    //   32-bit GPIF + 12-bit ADC -> group_size=8, bytes_per_sample=3
    let (analog_group_size, analog_bytes_per_sample) = match (gpif_width, adc_bits) {
        (16, 12) => (4u32, 3u32),
        (32, 12) => (8, 3),
        (16, 10) => (8, 5),
        _ => {
            return Err(DriverError::InvalidCaptureConfig {
                reason: format!(
                    "no valid analog and GPIF width combo: gpif_width={}, adc_bits={}",
                    gpif_width, adc_bits
                ),
            });
        }
    };

    let digital_packet_size = gpif_width / 8;

    // Compute the digital-to-analog ratio using GCD.
    let (digital_ratio, _analog_ratio) = if analog_rate > 0 && digital_rate > 0 {
        let g = gcd(digital_rate, analog_rate);
        ((digital_rate / g) as u32, (analog_rate / g) as u32)
    } else if analog_rate == 0 {
        // Digital-only: ratio is 1 (one digital group per cycle, no analog).
        (1, 0)
    } else {
        // Analog-only: digital_ratio is 0.
        (0, 1)
    };

    let settings = DisaggregatorSettings {
        digital_ratio,
        digital_packet_size,
        analog_group_size,
        analog_bytes_per_sample,
        digital_channels,
        analog_channels,
        low_speed_divider,
    };

    settings.validate()?;
    Ok(settings)
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // ContractReadData tests
    // -----------------------------------------------------------------------

    #[test]
    fn contract_divider_2_keeps_every_other_byte() {
        let data = vec![0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, 0x11, 0x22];
        let result = contract_read_data(&data, 2);
        assert_eq!(result, vec![0xAA, 0xCC, 0xEE, 0x11]);
    }

    #[test]
    fn contract_divider_4_keeps_every_4th_byte() {
        let data = vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08];
        let result = contract_read_data(&data, 4);
        assert_eq!(result, vec![0x01, 0x05]);
    }

    #[test]
    fn contract_divider_8_keeps_every_8th_byte() {
        let data: Vec<u8> = (0..16).collect();
        let result = contract_read_data(&data, 8);
        assert_eq!(result, vec![0x00, 0x08]);
    }

    #[test]
    fn contract_empty_input() {
        let result = contract_read_data(&[], 4);
        assert!(result.is_empty());
    }

    // -----------------------------------------------------------------------
    // Digital extraction: 32-bit GPIF (bswap32)
    // -----------------------------------------------------------------------

    #[test]
    fn digital_32bit_single_channel_single_ratio() {
        // Input: 0x01020304 as LE bytes -> [0x04, 0x03, 0x02, 0x01]
        // bswap32 of 0x04030201 = 0x01020304
        // Output bytes: [0x01, 0x02, 0x03, 0x04]
        let data = [0x04, 0x03, 0x02, 0x01];
        let mut output = vec![Vec::new()];
        extract_digital_32bit(&data, 1, 1, &mut output);
        assert_eq!(output[0], vec![0x01, 0x02, 0x03, 0x04]);
    }

    #[test]
    fn digital_32bit_two_channels_single_ratio() {
        // Channel 0: bytes [0x11, 0x22, 0x33, 0x44] -> u32 LE = 0x44332211
        //   bswap32 = 0x11223344 -> bytes [0x11, 0x22, 0x33, 0x44]
        // Channel 1: bytes [0xAA, 0xBB, 0xCC, 0xDD] -> u32 LE = 0xDDCCBBAA
        //   bswap32 = 0xAABBCCDD -> bytes [0xAA, 0xBB, 0xCC, 0xDD]
        let data = [0x11, 0x22, 0x33, 0x44, 0xAA, 0xBB, 0xCC, 0xDD];
        let mut output = vec![Vec::new(), Vec::new()];
        extract_digital_32bit(&data, 1, 2, &mut output);
        assert_eq!(output[0], vec![0x44, 0x33, 0x22, 0x11]);
        assert_eq!(output[1], vec![0xDD, 0xCC, 0xBB, 0xAA]);
    }

    #[test]
    fn digital_32bit_ratio_2() {
        // 2 ratio iterations, 1 channel. Two u32s.
        // Iteration 0: [0x01, 0x00, 0x00, 0x00] -> u32 LE = 0x00000001
        //   bswap32 = 0x01000000 -> bytes [0x01, 0x00, 0x00, 0x00]
        // Iteration 1: [0x02, 0x00, 0x00, 0x00] -> u32 LE = 0x00000002
        //   bswap32 = 0x02000000 -> bytes [0x02, 0x00, 0x00, 0x00]
        let data = [0x01, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00];
        let mut output = vec![Vec::new()];
        extract_digital_32bit(&data, 2, 1, &mut output);
        assert_eq!(
            output[0],
            vec![0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x02]
        );
    }

    // -----------------------------------------------------------------------
    // Digital extraction: 16-bit GPIF (byte swap)
    // -----------------------------------------------------------------------

    #[test]
    fn digital_16bit_single_channel() {
        let data = [0xAB, 0xCD];
        let mut output = vec![Vec::new()];
        extract_digital_16bit(&data, 1, 1, &mut output);
        // Byte-swap: write byte1 then byte0.
        assert_eq!(output[0], vec![0xCD, 0xAB]);
    }

    #[test]
    fn digital_16bit_two_channels_ratio_2() {
        // Ratio 0: ch0=[0x01, 0x02], ch1=[0x03, 0x04]
        // Ratio 1: ch0=[0x05, 0x06], ch1=[0x07, 0x08]
        let data = [0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08];
        let mut output = vec![Vec::new(), Vec::new()];
        extract_digital_16bit(&data, 2, 2, &mut output);
        assert_eq!(output[0], vec![0x02, 0x01, 0x06, 0x05]);
        assert_eq!(output[1], vec![0x04, 0x03, 0x08, 0x07]);
    }

    // -----------------------------------------------------------------------
    // Analog extraction: 12-bit, 8 samples from 12 bytes
    // -----------------------------------------------------------------------

    #[test]
    fn analog_12bit_8samples_zero() {
        // All zeros -> all zero samples.
        let data = [0u8; 12];
        let mut output = vec![Vec::new()];
        extract_analog_12bit_8samples(&data, 1, &mut output);
        assert_eq!(output[0], vec![0i16; 8]);
    }

    #[test]
    fn analog_12bit_8samples_known_values() {
        // Encode 8 x 12-bit values into 12 bytes (96 bits total).
        // Values: [1, 2, 3, 4, 5, 6, 7, 8]
        //
        // 12-bit values packed MSB-first in bit stream:
        //   sample0 = 1   = 0x001
        //   sample1 = 2   = 0x002
        //   sample2 = 3   = 0x003
        //   sample3 = 4   = 0x004
        //   sample4 = 5   = 0x005
        //   sample5 = 6   = 0x006
        //   sample6 = 7   = 0x007
        //   sample7 = 8   = 0x008
        //
        // Bit layout (96 bits = 12 bytes):
        //   bits[ 0:11] = 0x001
        //   bits[12:23] = 0x002
        //   bits[24:35] = 0x003
        //   bits[36:47] = 0x004
        //   bits[48:59] = 0x005
        //   bits[60:71] = 0x006
        //   bits[72:83] = 0x007
        //   bits[84:95] = 0x008

        // Build the 96-bit value in a u128, then extract bytes.
        let mut bits: u128 = 0;
        let values: [u16; 8] = [1, 2, 3, 4, 5, 6, 7, 8];
        for (i, &v) in values.iter().enumerate() {
            bits |= (v as u128) << (i * 12);
        }
        let all_bytes = bits.to_le_bytes();
        let data: [u8; 12] = all_bytes[..12].try_into().unwrap();

        let mut output = vec![Vec::new()];
        extract_analog_12bit_8samples(&data, 1, &mut output);

        assert_eq!(output[0].len(), 8);
        for (i, (&got, &expected)) in output[0].iter().zip(values.iter()).enumerate() {
            assert_eq!(
                got, expected as i16,
                "sample {} mismatch: got {}, expected {}",
                i, got, expected
            );
        }
    }

    #[test]
    fn analog_12bit_8samples_negative_values() {
        // Encode -1 (0xFFF) and -2048 (0x800) as 12-bit signed values.
        // A 12-bit value of 0xFFF sign-extends to i16 -1.
        // A 12-bit value of 0x800 sign-extends to i16 -2048.
        let values: [i16; 8] = [-1, -2048, 100, -100, 2047, 0, -1024, 1];

        // Pack into 96 bits.
        let mut bits: u128 = 0;
        for (i, &v) in values.iter().enumerate() {
            let unsigned_12bit = (v as u16) & 0x0FFF;
            bits |= (unsigned_12bit as u128) << (i * 12);
        }
        let all_bytes = bits.to_le_bytes();
        let data: [u8; 12] = all_bytes[..12].try_into().unwrap();

        let mut output = vec![Vec::new()];
        extract_analog_12bit_8samples(&data, 1, &mut output);

        assert_eq!(output[0].len(), 8);
        for (i, (&got, &expected)) in output[0].iter().zip(values.iter()).enumerate() {
            assert_eq!(
                got, expected,
                "sample {} mismatch: got {}, expected {}",
                i, got, expected
            );
        }
    }

    #[test]
    fn analog_12bit_8samples_two_channels() {
        // Two channels, each with 12 bytes.
        // Channel 0: all zeros.
        // Channel 1: sample0 = 0x123 = 291.
        let mut data = [0u8; 24];
        // Channel 1 starts at byte 12. Pack sample0 = 0x123 in bits[0:11].
        let val: u128 = 0x123;
        let bytes = val.to_le_bytes();
        data[12] = bytes[0]; // 0x23
        data[13] = bytes[1]; // 0x01

        let mut output = vec![Vec::new(), Vec::new()];
        extract_analog_12bit_8samples(&data, 2, &mut output);

        assert_eq!(output[0], vec![0i16; 8]);
        assert_eq!(output[1][0], 0x123);
    }

    // -----------------------------------------------------------------------
    // Analog extraction: 12-bit, 4 samples from 6 bytes
    // -----------------------------------------------------------------------

    #[test]
    fn analog_12bit_4samples_zero() {
        let data = [0u8; 6];
        let mut output = vec![Vec::new()];
        extract_analog_12bit_4samples(&data, 1, &mut output);
        assert_eq!(output[0], vec![0i16; 4]);
    }

    #[test]
    fn analog_12bit_4samples_known_values() {
        // Pack 4 x 12-bit values into 48 bits (6 bytes).
        let values: [i16; 4] = [1, 2, 3, 4];
        let mut bits: u64 = 0;
        for (i, &v) in values.iter().enumerate() {
            let unsigned_12bit = (v as u16) & 0x0FFF;
            bits |= (unsigned_12bit as u64) << (i * 12);
        }
        let all_bytes = bits.to_le_bytes();
        let data: [u8; 6] = all_bytes[..6].try_into().unwrap();

        let mut output = vec![Vec::new()];
        extract_analog_12bit_4samples(&data, 1, &mut output);

        for (i, (&got, &expected)) in output[0].iter().zip(values.iter()).enumerate() {
            assert_eq!(
                got, expected,
                "sample {} mismatch: got {}, expected {}",
                i, got, expected
            );
        }
    }

    #[test]
    fn analog_12bit_4samples_negative() {
        let values: [i16; 4] = [-1, -2048, 2047, -100];
        let mut bits: u64 = 0;
        for (i, &v) in values.iter().enumerate() {
            let unsigned_12bit = (v as u16) & 0x0FFF;
            bits |= (unsigned_12bit as u64) << (i * 12);
        }
        let all_bytes = bits.to_le_bytes();
        let data: [u8; 6] = all_bytes[..6].try_into().unwrap();

        let mut output = vec![Vec::new()];
        extract_analog_12bit_4samples(&data, 1, &mut output);

        for (i, (&got, &expected)) in output[0].iter().zip(values.iter()).enumerate() {
            assert_eq!(
                got, expected,
                "sample {} mismatch: got {}, expected {}",
                i, got, expected
            );
        }
    }

    // -----------------------------------------------------------------------
    // Full cycle: digital-only (32-bit GPIF)
    // -----------------------------------------------------------------------

    #[test]
    fn full_cycle_digital_only_32bit() {
        let settings = DisaggregatorSettings {
            digital_ratio: 1,
            digital_packet_size: 4,
            analog_group_size: 0,
            analog_bytes_per_sample: 3,
            digital_channels: vec![true, true],
            analog_channels: vec![],
            low_speed_divider: 1,
        };
        assert_eq!(settings.full_cycle_size(), 8);

        let disagg = Disaggregator::new(settings).unwrap();

        // The bswap32 reverses bytes: input [a,b,c,d] -> output [d,c,b,a].
        // read_LE([a,b,c,d]) = d<<24|c<<16|b<<8|a. bswap -> a<<24|b<<16|c<<8|d.
        // write_NE on LE -> [d,c,b,a].
        let data = vec![
            0x01, 0x02, 0x03, 0x04, // ch0
            0xAA, 0xBB, 0xCC, 0xDD, // ch1
        ];
        let out = disagg.process(&data).unwrap();
        assert_eq!(out.digital.len(), 2);
        assert_eq!(out.analog.len(), 0);
        assert_eq!(out.digital[0], vec![0x04, 0x03, 0x02, 0x01]);
        assert_eq!(out.digital[1], vec![0xDD, 0xCC, 0xBB, 0xAA]);
    }

    // -----------------------------------------------------------------------
    // Full cycle: mixed digital + analog (32-bit GPIF, 12-bit ADC)
    // -----------------------------------------------------------------------

    #[test]
    fn full_cycle_mixed_32bit_12bit() {
        // 2 digital channels, 1 analog channel, digital_ratio=1.
        // digital_group_bytes = 1 * 4 * 2 = 8
        // analog_channel_bytes = (3 * 8) / 2 = 12
        // analog_group_bytes = 12 * 1 = 12
        // full_cycle = 8 + 12 = 20
        let settings = DisaggregatorSettings {
            digital_ratio: 1,
            digital_packet_size: 4,
            analog_group_size: 8,
            analog_bytes_per_sample: 3,
            digital_channels: vec![true, true],
            analog_channels: vec![true],
            low_speed_divider: 1,
        };

        // With the /2 formula:
        //   digital_group_bytes = 1 * 4 * 2 = 8
        //   analog_group_bytes = (3 * 8 / 2) * 1 = 12
        //   full_cycle = 20
        assert_eq!(settings.full_cycle_size(), 20);

        let disagg = Disaggregator::new(settings).unwrap();

        // Build one cycle: 8 bytes digital + 12 bytes analog.
        let mut data = vec![0u8; 20];
        // Digital ch0: [0x11, 0x22, 0x33, 0x44]
        data[0] = 0x11;
        data[1] = 0x22;
        data[2] = 0x33;
        data[3] = 0x44;
        // Digital ch1: [0x55, 0x66, 0x77, 0x88]
        data[4] = 0x55;
        data[5] = 0x66;
        data[6] = 0x77;
        data[7] = 0x88;

        // Analog ch0: 12 bytes encoding 8 samples.
        // Encode values [10, 20, 30, 40, 50, 60, 70, 80] as packed 12-bit.
        let values: [i16; 8] = [10, 20, 30, 40, 50, 60, 70, 80];
        let mut bits: u128 = 0;
        for (i, &v) in values.iter().enumerate() {
            bits |= ((v as u16 & 0x0FFF) as u128) << (i * 12);
        }
        let analog_bytes = bits.to_le_bytes();
        data[8..20].copy_from_slice(&analog_bytes[..12]);

        let out = disagg.process(&data).unwrap();
        assert_eq!(out.digital.len(), 2);
        assert_eq!(out.analog.len(), 1);
        assert_eq!(out.digital[0], vec![0x44, 0x33, 0x22, 0x11]);
        assert_eq!(out.digital[1], vec![0x88, 0x77, 0x66, 0x55]);
        assert_eq!(out.analog[0], values.to_vec());
    }

    // -----------------------------------------------------------------------
    // Full cycle with ContractReadData
    // -----------------------------------------------------------------------

    #[test]
    fn full_cycle_with_contraction() {
        // Digital-only, 1 channel, divider=2.
        // After contraction, need 4 bytes (one u32).
        // Before contraction: 8 bytes (every other byte kept).
        let settings = DisaggregatorSettings {
            digital_ratio: 1,
            digital_packet_size: 4,
            analog_group_size: 0,
            analog_bytes_per_sample: 3,
            digital_channels: vec![true],
            analog_channels: vec![],
            low_speed_divider: 2,
        };

        let disagg = Disaggregator::new(settings).unwrap();

        // We want contracted data [0x01, 0x02, 0x03, 0x04].
        // With divider=2, input must be [0x01, X, 0x02, X, 0x03, X, 0x04, X].
        let data = vec![0x01, 0xFF, 0x02, 0xFF, 0x03, 0xFF, 0x04, 0xFF];
        let out = disagg.process(&data).unwrap();
        assert_eq!(out.digital[0], vec![0x04, 0x03, 0x02, 0x01]);
    }

    // -----------------------------------------------------------------------
    // Error cases
    // -----------------------------------------------------------------------

    #[test]
    fn error_on_non_multiple_of_cycle() {
        let settings = DisaggregatorSettings {
            digital_ratio: 1,
            digital_packet_size: 4,
            analog_group_size: 0,
            analog_bytes_per_sample: 3,
            digital_channels: vec![true],
            analog_channels: vec![],
            low_speed_divider: 1,
        };
        let disagg = Disaggregator::new(settings).unwrap();
        // Cycle size is 4, but we provide 5 bytes.
        let result = disagg.process(&[0; 5]);
        assert!(result.is_err());
    }

    #[test]
    fn error_on_invalid_packet_size() {
        let settings = DisaggregatorSettings {
            digital_ratio: 1,
            digital_packet_size: 3,
            analog_group_size: 0,
            analog_bytes_per_sample: 3,
            digital_channels: vec![true],
            analog_channels: vec![],
            low_speed_divider: 1,
        };
        assert!(Disaggregator::new(settings).is_err());
    }

    #[test]
    fn error_on_invalid_divider() {
        let settings = DisaggregatorSettings {
            digital_ratio: 1,
            digital_packet_size: 4,
            analog_group_size: 0,
            analog_bytes_per_sample: 3,
            digital_channels: vec![true],
            analog_channels: vec![],
            low_speed_divider: 3,
        };
        assert!(Disaggregator::new(settings).is_err());
    }

    #[test]
    fn error_on_zero_channels() {
        let settings = DisaggregatorSettings {
            digital_ratio: 1,
            digital_packet_size: 4,
            analog_group_size: 0,
            analog_bytes_per_sample: 3,
            digital_channels: vec![],
            analog_channels: vec![],
            low_speed_divider: 1,
        };
        assert!(Disaggregator::new(settings).is_err());
    }

    // -----------------------------------------------------------------------
    // compute_mix_rate tests
    // -----------------------------------------------------------------------

    #[test]
    fn mix_rate_32bit_12bit_equal_rates() {
        let settings = compute_mix_rate(
            50_000_000,
            50_000_000,
            32,
            12,
            vec![true; 4],
            vec![true; 4],
            1,
        )
        .unwrap();
        assert_eq!(settings.digital_ratio, 1);
        assert_eq!(settings.digital_packet_size, 4);
        assert_eq!(settings.analog_group_size, 8);
        assert_eq!(settings.analog_bytes_per_sample, 3);
    }

    #[test]
    fn mix_rate_32bit_12bit_2x_digital() {
        // 100 MHz digital, 50 MHz analog -> ratio 2:1.
        let settings = compute_mix_rate(
            100_000_000,
            50_000_000,
            32,
            12,
            vec![true; 4],
            vec![true; 4],
            1,
        )
        .unwrap();
        assert_eq!(settings.digital_ratio, 2);
    }

    #[test]
    fn mix_rate_16bit_12bit() {
        let settings = compute_mix_rate(
            50_000_000,
            50_000_000,
            16,
            12,
            vec![true; 4],
            vec![true; 4],
            1,
        )
        .unwrap();
        assert_eq!(settings.digital_packet_size, 2);
        assert_eq!(settings.analog_group_size, 4);
    }

    #[test]
    fn mix_rate_invalid_combo() {
        let result = compute_mix_rate(
            50_000_000,
            50_000_000,
            64, // invalid
            12,
            vec![true],
            vec![true],
            1,
        );
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // GCD helper
    // -----------------------------------------------------------------------

    #[test]
    fn gcd_basic() {
        assert_eq!(gcd(100_000_000, 50_000_000), 50_000_000);
        assert_eq!(gcd(12, 8), 4);
        assert_eq!(gcd(7, 13), 1);
        assert_eq!(gcd(100, 100), 100);
    }

    // -----------------------------------------------------------------------
    // Multiple cycles
    // -----------------------------------------------------------------------

    #[test]
    fn multiple_cycles_accumulate() {
        let settings = DisaggregatorSettings {
            digital_ratio: 1,
            digital_packet_size: 4,
            analog_group_size: 0,
            analog_bytes_per_sample: 3,
            digital_channels: vec![true],
            analog_channels: vec![],
            low_speed_divider: 1,
        };
        let disagg = Disaggregator::new(settings).unwrap();

        // 2 cycles of 4 bytes each = 8 bytes total.
        let data = vec![
            0x01, 0x02, 0x03, 0x04, // cycle 0
            0x05, 0x06, 0x07, 0x08, // cycle 1
        ];
        let out = disagg.process(&data).unwrap();
        assert_eq!(out.digital[0].len(), 8); // 4 bytes * 2 cycles
        assert_eq!(
            out.digital[0],
            vec![0x04, 0x03, 0x02, 0x01, 0x08, 0x07, 0x06, 0x05]
        );
    }

    // -----------------------------------------------------------------------
    // Empty input
    // -----------------------------------------------------------------------

    #[test]
    fn empty_input_produces_empty_output() {
        let settings = DisaggregatorSettings {
            digital_ratio: 1,
            digital_packet_size: 4,
            analog_group_size: 0,
            analog_bytes_per_sample: 3,
            digital_channels: vec![true],
            analog_channels: vec![],
            low_speed_divider: 1,
        };
        let disagg = Disaggregator::new(settings).unwrap();
        let out = disagg.process(&[]).unwrap();
        assert_eq!(out.digital.len(), 1);
        assert!(out.digital[0].is_empty());
    }
}
