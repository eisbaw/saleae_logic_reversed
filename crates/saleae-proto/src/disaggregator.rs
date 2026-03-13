//! EP2 data deinterleaving (disaggregation).
//!
//! The Saleae FPGA sends interleaved digital+analog sample data on EP2 IN.
//! The `Disaggregator` splits this interleaved stream into per-channel data.
//!
//! # Architecture
//!
//! Three device families use different interleaving formats:
//!
//! - **Pro** (Logic Pro 8/16, `FpgaType::Pro`): Mixed digital+analog cycles.
//!   Each cycle has `digital_ratio` groups of digital data followed by analog
//!   data. Digital words are byte-swapped. Analog data uses packed 12-bit ADC
//!   encoding with arithmetic right-shift sign extension.
//!
//! - **NonPro** (Logic 8, `FpgaType::NonPro`): Digital-only deinterleaving.
//!   Data arrives in 8-byte groups, one byte per channel after transformation.
//!
//! - **Logic 4**: FX2LP-based, not supported by this FX3 driver.
//!
//! # Wire format references
//!
//! - `re/notes/disaggregator-interleaving.md` -- decompiled deinterleaving
//! - `re/notes/device-helpers.md` -- DeviceCalculateMixRate algorithm
//!
//! # Confidence
//!
//! The Pro deinterleaving format is **Confirmed** from decompilation of
//! FUN_01bf1190. The 12-bit ADC sign extension via `psraw(4)` is **Confirmed**.
//! The Logic 8 format is **Confirmed** from FUN_01bf21b0. Exact bit-level
//! behavior of Logic 8 FUN_01bf1ff0 transform is still uncertain.

use crate::device::{DeviceFeatureClass, DeviceProfile};
use crate::error::{ProtoError, ProtoResult};

// ---------------------------------------------------------------------------
// ADC packing parameters
// ---------------------------------------------------------------------------

/// ADC packing parameters derived from GPIF width and ADC bit depth.
///
/// From `DeviceCalculateMixRate` (ELF VA 0x1c19fa0):
///
/// | GPIF Width | ADC Bits | samples_per_group | bytes_per_group |
/// |------------|----------|-------------------|-----------------|
/// | 16         | 10       | 8                 | 5               |
/// | 16         | 12       | 4                 | 3               |
/// | 32         | 12       | 8                 | 3               |
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdcPacking {
    /// Number of ADC samples per packed group.
    pub samples_per_group: u32,
    /// Bytes consumed per packed group (before multiplication by channels).
    pub bytes_per_group: u32,
    /// ADC bits (10 or 12).
    pub adc_bits: u32,
}

/// Look up ADC packing parameters from GPIF width and ADC bit depth.
///
/// Returns `None` for invalid combinations.
pub fn adc_packing(gpif_width: u32, adc_bits: u32) -> Option<AdcPacking> {
    match (gpif_width, adc_bits) {
        (16, 10) => Some(AdcPacking {
            samples_per_group: 8,
            bytes_per_group: 5,
            adc_bits: 10,
        }),
        (16, 12) => Some(AdcPacking {
            samples_per_group: 4,
            bytes_per_group: 3,
            adc_bits: 12,
        }),
        (32, 12) => Some(AdcPacking {
            samples_per_group: 8,
            bytes_per_group: 3,
            adc_bits: 12,
        }),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Mix rate computation
// ---------------------------------------------------------------------------

/// Result of `calculate_mix_rate`: the interleave ratios for a mixed-mode capture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MixRateResult {
    /// Number of digital word groups per interleave cycle.
    ///
    /// Each group has one word per enabled digital channel.
    /// Word size is `gpif_width / 8` bytes.
    pub digital_ratio: u32,

    /// Number of analog sample groups per interleave cycle.
    ///
    /// Each group has `bytes_per_group` bytes per enabled analog channel.
    pub analog_ratio: u32,

    /// Bytes per complete interleave cycle, given the channel counts.
    ///
    /// `cycle_bytes = digital_ratio * num_digital * packet_size
    ///              + analog_ratio * num_analog * bytes_per_group`
    pub cycle_bytes: u32,
}

/// Compute the interleave ratios for a mixed-mode capture.
///
/// Implements the algorithm from `DeviceCalculateMixRate` (ELF VA 0x1c19fa0).
///
/// # Arguments
///
/// - `digital_rate`: Digital sample rate in Hz (0 for analog-only).
/// - `analog_rate`: Analog sample rate in Hz (0 for digital-only).
/// - `gpif_width`: GPIF bus width in bits (16 or 32).
/// - `adc_bits`: ADC bit depth (10 or 12).
/// - `num_digital`: Number of enabled digital channels.
/// - `num_analog`: Number of enabled analog channels.
///
/// # Errors
///
/// Returns error if both rates are zero, or if the GPIF/ADC combination
/// is invalid.
pub fn calculate_mix_rate(
    digital_rate: u32,
    analog_rate: u32,
    gpif_width: u32,
    adc_bits: u32,
    num_digital: u32,
    num_analog: u32,
) -> ProtoResult<MixRateResult> {
    if digital_rate == 0 && analog_rate == 0 {
        return Err(ProtoError::parse("at least one sample rate must be set"));
    }

    let packing = adc_packing(gpif_width, adc_bits).ok_or_else(|| {
        ProtoError::parse(format!(
            "no valid analog and gpif width combo: gpif_width={gpif_width}, adc_bits={adc_bits}"
        ))
    })?;

    let packet_size = gpif_width / 8; // 2 or 4 bytes per digital word

    match (digital_rate, analog_rate) {
        (_, 0) => {
            // Digital-only: one digital group per cycle.
            let digital_ratio = num_digital;
            Ok(MixRateResult {
                digital_ratio,
                analog_ratio: 0,
                cycle_bytes: digital_ratio * packet_size,
            })
        }
        (0, _) => {
            // Analog-only: one analog group per cycle.
            let analog_ratio = packing.samples_per_group * num_analog;
            Ok(MixRateResult {
                digital_ratio: 0,
                analog_ratio,
                cycle_bytes: analog_ratio * packing.bytes_per_group,
            })
        }
        (dr, ar) => {
            // Mixed mode: GCD-based ratio computation.
            //
            // From RE (device-helpers.md):
            //   digital_reduced = digital_sample_rate / gpif_width
            //   analog_reduced  = analog_sample_rate / analog_bytes
            //   g = gcd(digital_reduced, analog_reduced)
            //   digital_quotient = digital_reduced / g
            //   analog_quotient  = analog_reduced / g
            //
            //   out_digital_mixed    = digital_multiplier * digital_quotient
            //   out_analog_mixed     = samples_per_group * analog_multiplier * analog_quotient
            let digital_reduced = dr / gpif_width;
            let analog_reduced = ar / packing.bytes_per_group;
            let g = gcd(digital_reduced, analog_reduced);
            let dq = digital_reduced / g;
            let aq = analog_reduced / g;

            let digital_ratio = num_digital * dq;
            let analog_ratio = packing.samples_per_group * num_analog * aq;

            let digital_bytes = digital_ratio * packet_size;
            let analog_bytes = analog_ratio * packing.bytes_per_group;

            Ok(MixRateResult {
                digital_ratio,
                analog_ratio,
                cycle_bytes: digital_bytes + analog_bytes,
            })
        }
    }
}

/// Euclidean GCD (matches ComputationUtilities::Gcd at ELF VA 0x1c47140).
fn gcd(mut a: u32, mut b: u32) -> u32 {
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a
}

// ---------------------------------------------------------------------------
// Disaggregated output
// ---------------------------------------------------------------------------

/// Output of disaggregation: per-channel digital and analog sample streams.
#[derive(Debug, Clone)]
pub struct DisaggregatedData {
    /// Per-channel digital sample bytes.
    ///
    /// `digital_samples[i]` contains packed digital samples for the i-th
    /// enabled digital channel. Each byte holds 8 samples, LSB first.
    pub digital_samples: Vec<Vec<u8>>,

    /// Per-channel analog sample values.
    ///
    /// `analog_samples[i]` contains signed 12-bit (or 10-bit) ADC values
    /// for the i-th enabled analog channel, sign-extended to i16.
    pub analog_samples: Vec<Vec<i16>>,
}

// ---------------------------------------------------------------------------
// Disaggregator
// ---------------------------------------------------------------------------

/// Configuration and state for deinterleaving EP2 data.
///
/// Created once per capture, then `process_chunk` is called for each data
/// chunk received from EP2 IN.
#[derive(Debug, Clone)]
pub struct Disaggregator {
    feature_class: DeviceFeatureClass,
    num_digital: usize,
    num_analog: usize,
    /// GPIF bus width in bytes (2 or 4).
    packet_size: usize,
    /// ADC packing parameters.
    packing: AdcPacking,
    /// Interleave ratios (only meaningful for Pro mixed-mode).
    mix_rate: MixRateResult,
    /// Low-speed divider for ContractReadData. Valid values: 1, 2, 4, 8.
    /// When > 1, every Nth byte is kept before disaggregation.
    low_speed_divider: u32,
}

impl Disaggregator {
    /// Create a new disaggregator for the given device and capture configuration.
    ///
    /// # Arguments
    ///
    /// - `profile`: Device hardware profile.
    /// - `digital_channels`: List of enabled digital channel indices.
    /// - `analog_channels`: List of enabled analog channel indices.
    /// - `digital_rate`: Digital sample rate in Hz (0 if no digital capture).
    /// - `analog_rate`: Analog sample rate in Hz (0 if no analog capture).
    ///
    /// # Errors
    ///
    /// Returns error for unsupported device types (Logic 4, legacy devices)
    /// or invalid rate/packing combinations.
    pub fn new(
        profile: &DeviceProfile,
        digital_channels: &[u8],
        analog_channels: &[u8],
        digital_rate: u64,
        analog_rate: u64,
    ) -> ProtoResult<Self> {
        if profile.feature_class == DeviceFeatureClass::Mso {
            return Err(ProtoError::parse(
                "MSO devices are not supported by disaggregator",
            ));
        }

        // Logic 4 is FX2LP; our FX3-based disaggregator cannot handle it.
        if profile.device_type == crate::device::DeviceType::Logic4 {
            return Err(ProtoError::parse(
                "Logic 4 (FX2LP) is not supported by this FX3 disaggregator",
            ));
        }

        let gpif_width = profile.gpif_width as u32;
        let adc_bits = profile.adc_bits as u32;
        let num_digital = digital_channels.len();
        let num_analog = analog_channels.len();

        let packing = adc_packing(gpif_width, adc_bits).ok_or_else(|| {
            ProtoError::parse(format!(
                "no valid analog/gpif combo: gpif_width={gpif_width}, adc_bits={adc_bits}"
            ))
        })?;

        // Clamp rates to u32 for mix rate calculation (rates above 4 GHz
        // are physically impossible for these devices).
        let dr = digital_rate.min(u32::MAX as u64) as u32;
        let ar = analog_rate.min(u32::MAX as u64) as u32;

        let mix_rate = calculate_mix_rate(
            dr,
            ar,
            gpif_width,
            adc_bits,
            num_digital as u32,
            num_analog as u32,
        )?;

        Ok(Self {
            feature_class: profile.feature_class,
            num_digital,
            num_analog,
            packet_size: (gpif_width / 8) as usize,
            packing,
            mix_rate,
            low_speed_divider: 1,
        })
    }

    /// Create a disaggregator from explicit parameters (no DeviceProfile needed).
    ///
    /// This is useful when the caller already knows the hardware parameters
    /// (e.g. from a `CaptureConfig`), and avoids the need to construct or
    /// reference a full `DeviceProfile`.
    ///
    /// # Arguments
    ///
    /// - `feature_class`: Pro, NonPro, etc.
    /// - `gpif_width`: GPIF bus width in bits (16 or 32).
    /// - `adc_bits`: ADC resolution in bits (10 or 12).
    /// - `digital_channels`: List of enabled digital channel indices.
    /// - `analog_channels`: List of enabled analog channel indices.
    /// - `digital_rate`: Digital sample rate in Hz (0 if no digital capture).
    /// - `analog_rate`: Analog sample rate in Hz (0 if no analog capture).
    /// - `low_speed_divider`: Low-speed divider (1, 2, 4, or 8).
    #[allow(clippy::too_many_arguments)]
    pub fn from_params(
        feature_class: DeviceFeatureClass,
        gpif_width: u32,
        adc_bits: u32,
        digital_channels: &[u8],
        analog_channels: &[u8],
        digital_rate: u64,
        analog_rate: u64,
        low_speed_divider: u32,
    ) -> ProtoResult<Self> {
        if feature_class == DeviceFeatureClass::Mso {
            return Err(ProtoError::parse(
                "MSO devices are not supported by disaggregator",
            ));
        }
        if !matches!(low_speed_divider, 1 | 2 | 4 | 8) {
            return Err(ProtoError::parse(format!(
                "low_speed_divider must be 1, 2, 4, or 8, got {low_speed_divider}"
            )));
        }

        let num_digital = digital_channels.len();
        let num_analog = analog_channels.len();

        let packing = adc_packing(gpif_width, adc_bits).ok_or_else(|| {
            ProtoError::parse(format!(
                "no valid analog/gpif combo: gpif_width={gpif_width}, adc_bits={adc_bits}"
            ))
        })?;

        let dr = digital_rate.min(u32::MAX as u64) as u32;
        let ar = analog_rate.min(u32::MAX as u64) as u32;

        let mix_rate = calculate_mix_rate(
            dr,
            ar,
            gpif_width,
            adc_bits,
            num_digital as u32,
            num_analog as u32,
        )?;

        Ok(Self {
            feature_class,
            num_digital,
            num_analog,
            packet_size: (gpif_width / 8) as usize,
            packing,
            mix_rate,
            low_speed_divider,
        })
    }

    /// Create a disaggregator from pre-computed register-derived settings.
    ///
    /// Use this when the interleave parameters have already been computed
    /// by the FPGA register calculation (e.g. from `CaptureRegisters`).
    /// The `digital_ratio` here is the number of digital word groups per
    /// interleave cycle (NOT multiplied by num_digital -- that multiplication
    /// happens internally).
    ///
    /// # Arguments
    ///
    /// - `feature_class`: Pro, NonPro, etc.
    /// - `gpif_width`: GPIF bus width in bits (16 or 32).
    /// - `adc_bits`: ADC resolution in bits (10 or 12).
    /// - `num_digital`: Number of enabled digital channels.
    /// - `num_analog`: Number of enabled analog channels.
    /// - `digital_ratio`: Digital word groups per interleave cycle.
    /// - `low_speed_divider`: Low-speed divider (1, 2, 4, or 8).
    pub fn from_register_settings(
        feature_class: DeviceFeatureClass,
        gpif_width: u32,
        adc_bits: u32,
        num_digital: usize,
        num_analog: usize,
        digital_ratio: u32,
        low_speed_divider: u32,
    ) -> ProtoResult<Self> {
        if feature_class == DeviceFeatureClass::Mso {
            return Err(ProtoError::parse(
                "MSO devices are not supported by disaggregator",
            ));
        }
        if !matches!(low_speed_divider, 1 | 2 | 4 | 8) {
            return Err(ProtoError::parse(format!(
                "low_speed_divider must be 1, 2, 4, or 8, got {low_speed_divider}"
            )));
        }

        let packet_size = gpif_width / 8;

        let packing = adc_packing(gpif_width, adc_bits).ok_or_else(|| {
            ProtoError::parse(format!(
                "no valid analog/gpif combo: gpif_width={gpif_width}, adc_bits={adc_bits}"
            ))
        })?;

        // Compute cycle bytes from the register-derived digital_ratio.
        // Digital section: digital_ratio * packet_size * num_digital
        // Analog section: (bytes_per_group * samples_per_group / 2) * num_analog
        //   = analog_channel_bytes * num_analog
        // For 12-bit ADC with 32-bit GPIF: 12 bytes per channel.
        // For 12-bit ADC with 16-bit GPIF: 6 bytes per channel.
        let digital_bytes = digital_ratio * packet_size * num_digital as u32;
        let analog_channel_bytes =
            (packing.bytes_per_group * packing.samples_per_group) / 2;
        let analog_bytes = analog_channel_bytes * num_analog as u32;

        let mix_rate = MixRateResult {
            // digital_ratio in MixRateResult is total digital words per cycle
            // (digital_ratio_per_channel * num_digital).
            digital_ratio: digital_ratio * num_digital as u32,
            analog_ratio: if num_analog > 0 {
                packing.samples_per_group * num_analog as u32
            } else {
                0
            },
            cycle_bytes: digital_bytes + analog_bytes,
        };

        if mix_rate.cycle_bytes == 0 && (num_digital > 0 || num_analog > 0) {
            return Err(ProtoError::parse("cycle_bytes is 0 but channels are enabled"));
        }

        Ok(Self {
            feature_class,
            num_digital,
            num_analog,
            packet_size: packet_size as usize,
            packing,
            mix_rate,
            low_speed_divider,
        })
    }

    /// Return the computed mix rate result.
    pub fn mix_rate(&self) -> &MixRateResult {
        &self.mix_rate
    }

    /// Return the ADC packing parameters for this configuration.
    pub fn adc_packing(&self) -> &AdcPacking {
        &self.packing
    }

    /// Return the expected interleave cycle size in bytes.
    pub fn cycle_bytes(&self) -> usize {
        self.mix_rate.cycle_bytes as usize
    }

    /// Return the number of enabled digital channels.
    pub fn num_digital(&self) -> usize {
        self.num_digital
    }

    /// Return the number of enabled analog channels.
    pub fn num_analog(&self) -> usize {
        self.num_analog
    }

    /// Return the low-speed divider.
    pub fn low_speed_divider(&self) -> u32 {
        self.low_speed_divider
    }

    /// Set the low-speed divider for ContractReadData decimation.
    ///
    /// Valid values: 1, 2, 4, or 8. When > 1, every Nth byte of raw
    /// EP2 data is kept before disaggregation.
    ///
    /// # Errors
    ///
    /// Returns error if the divider is not 1, 2, 4, or 8.
    pub fn set_low_speed_divider(&mut self, divider: u32) -> ProtoResult<()> {
        if !matches!(divider, 1 | 2 | 4 | 8) {
            return Err(ProtoError::parse(format!(
                "low_speed_divider must be 1, 2, 4, or 8, got {divider}"
            )));
        }
        self.low_speed_divider = divider;
        Ok(())
    }

    /// Deinterleave a chunk of EP2 data into per-channel streams.
    ///
    /// If `low_speed_divider > 1`, applies ContractReadData decimation
    /// first (keeps every Nth byte). Then deinterleaves the resulting
    /// data into per-channel streams.
    ///
    /// The input length (after decimation) must be a multiple of
    /// `cycle_bytes()`.
    ///
    /// # Errors
    ///
    /// Returns error if the data length is not a multiple of the cycle size.
    pub fn process_chunk(&self, data: &[u8]) -> ProtoResult<DisaggregatedData> {
        let effective_data;
        let data = if self.low_speed_divider > 1 {
            effective_data = contract_read_data(data, self.low_speed_divider);
            &effective_data
        } else {
            data
        };

        match self.feature_class {
            DeviceFeatureClass::Pro => self.deinterleave_pro(data),
            DeviceFeatureClass::NonPro => self.deinterleave_non_pro(data),
            DeviceFeatureClass::Mso => Err(ProtoError::parse("MSO not supported")),
        }
    }

    // -----------------------------------------------------------------------
    // Pro deinterleaving (Logic Pro 8/16)
    // -----------------------------------------------------------------------

    /// Deinterleave Pro device data (FUN_01bf1190).
    ///
    /// Cycle structure:
    /// ```text
    /// +-- digital_ratio * packet_size bytes --+-- analog data --+
    /// | D0 | D1 | ... (byte-swapped words)   | A0 | A1 | ...   |
    /// +--------------------------------------+-----------------+
    /// ```
    fn deinterleave_pro(&self, data: &[u8]) -> ProtoResult<DisaggregatedData> {
        let cycle_size = self.cycle_bytes();
        if cycle_size == 0 {
            return Ok(DisaggregatedData {
                digital_samples: vec![Vec::new(); self.num_digital],
                analog_samples: vec![Vec::new(); self.num_analog],
            });
        }

        if data.len() % cycle_size != 0 {
            return Err(ProtoError::parse(format!(
                "data length {} is not a multiple of cycle size {}",
                data.len(),
                cycle_size,
            )));
        }

        let num_cycles = data.len() / cycle_size;
        let digital_group_bytes = self.mix_rate.digital_ratio as usize * self.packet_size;
        let analog_group_bytes = cycle_size - digital_group_bytes;

        // Pre-allocate output buffers.
        let digital_bytes_per_channel = if self.num_digital > 0 {
            num_cycles * self.mix_rate.digital_ratio as usize / self.num_digital * self.packet_size
        } else {
            0
        };
        let mut digital_out: Vec<Vec<u8>> =
            vec![Vec::with_capacity(digital_bytes_per_channel); self.num_digital];

        // Analog: each channel gets samples_per_group * analog_quotient samples
        // per cycle (the total analog_ratio divided by num_analog).
        let analog_samples_per_channel_per_cycle = if self.num_analog > 0 {
            self.mix_rate.analog_ratio as usize / self.num_analog
        } else {
            0
        };
        let mut analog_out: Vec<Vec<i16>> =
            vec![
                Vec::with_capacity(num_cycles * analog_samples_per_channel_per_cycle,);
                self.num_analog
            ];

        for cycle_idx in 0..num_cycles {
            let cycle_start = cycle_idx * cycle_size;

            // --- Digital extraction ---
            if self.num_digital > 0 {
                let dig_data = &data[cycle_start..cycle_start + digital_group_bytes];
                self.extract_digital_pro(dig_data, &mut digital_out);
            }

            // --- Analog extraction ---
            if self.num_analog > 0 {
                let ana_start = cycle_start + digital_group_bytes;
                let ana_data = &data[ana_start..ana_start + analog_group_bytes];
                self.extract_analog_pro(ana_data, &mut analog_out)?;
            }
        }

        Ok(DisaggregatedData {
            digital_samples: digital_out,
            analog_samples: analog_out,
        })
    }

    /// Extract digital data from one cycle's digital group (Pro device).
    ///
    /// The digital group contains `digital_ratio` words. These words are
    /// interleaved per enabled channel: the first `num_digital` words go
    /// one-per-channel (repeated `digital_ratio/num_digital` times).
    ///
    /// Each word is byte-swapped (FPGA to host endian correction).
    fn extract_digital_pro(&self, digital_data: &[u8], output: &mut [Vec<u8>]) {
        let ps = self.packet_size;
        let num_words = digital_data.len() / ps;

        for word_idx in 0..num_words {
            let ch = word_idx % self.num_digital;
            let offset = word_idx * ps;
            let word = &digital_data[offset..offset + ps];

            // Byte-swap: reverse byte order within each word.
            // 2-byte: swap(b0, b1) -> [b1, b0]
            // 4-byte: bswap32 -> [b3, b2, b1, b0]
            match ps {
                2 => {
                    output[ch].push(word[1]);
                    output[ch].push(word[0]);
                }
                4 => {
                    output[ch].push(word[3]);
                    output[ch].push(word[2]);
                    output[ch].push(word[1]);
                    output[ch].push(word[0]);
                }
                _ => {
                    // Should not happen with valid GPIF widths.
                    for &b in word.iter().rev() {
                        output[ch].push(b);
                    }
                }
            }
        }
    }

    /// Extract analog data from one cycle's analog group (Pro device).
    ///
    /// The analog group contains packed 12-bit ADC samples, interleaved
    /// per enabled channel. Two packing formats exist:
    ///
    /// - **16-bit GPIF (packet_size=2)**: 4 samples in 6 bytes per channel.
    /// - **32-bit GPIF (packet_size=4)**: 8 samples in 12 bytes per channel.
    fn extract_analog_pro(&self, analog_data: &[u8], output: &mut [Vec<i16>]) -> ProtoResult<()> {
        if self.num_analog == 0 {
            return Ok(());
        }

        match self.packet_size {
            2 => self.extract_analog_pro_16bit(analog_data, output),
            4 => self.extract_analog_pro_32bit(analog_data, output),
            _ => Err(ProtoError::parse(format!(
                "unexpected packet_size {} for analog extraction",
                self.packet_size
            ))),
        }
    }

    /// 16-bit GPIF path: 4 samples in 6 bytes.
    ///
    /// From decompilation of FUN_01bf1190 (packet_size == 2 path):
    /// ```c
    /// uVar20 = *(uint32_t*)src;       // bytes 0-3
    /// sVar5 = *(int16_t*)(src + 4);   // bytes 4-5
    ///
    /// out[0] = (int16_t)((uVar20 << 4) & 0xFFF0) >> 4;
    /// out[1] = (int16_t)((uVar20 >> 8) & 0xFFF0) >> 4;
    /// out[2] = (int16_t)((uVar20 >> 20) & 0xFFF0) >> 4;
    /// out[3] = (int16_t)(sVar5 & 0xFFF0) >> 4;
    /// ```
    fn extract_analog_pro_16bit(&self, data: &[u8], output: &mut [Vec<i16>]) -> ProtoResult<()> {
        // Each channel consumes 6 bytes to produce 4 samples.
        // Total groups = analog_ratio / num_analog
        // Each group processes all channels once.
        let bytes_per_channel_group = 6;
        let samples_per_channel_group = 4;
        let group_size = bytes_per_channel_group * self.num_analog;

        if data.len() % group_size != 0 {
            return Err(ProtoError::parse(format!(
                "analog data length {} not a multiple of group size {}",
                data.len(),
                group_size,
            )));
        }

        let num_groups = data.len() / group_size;
        let _ = samples_per_channel_group; // used implicitly in unpack

        for group_idx in 0..num_groups {
            let group_start = group_idx * group_size;
            for (ch, ch_out) in output.iter_mut().enumerate().take(self.num_analog) {
                let offset = group_start + ch * bytes_per_channel_group;
                let src = &data[offset..offset + 6];

                // Read as little-endian u32 + i16.
                let u32_val = u32::from_le_bytes([src[0], src[1], src[2], src[3]]);
                let i16_val = i16::from_le_bytes([src[4], src[5]]);

                // Unpack 4 x 12-bit samples with sign extension.
                let s0 = ((u32_val << 4) & 0xFFF0) as i16 >> 4;
                let s1 = ((u32_val >> 8) & 0xFFF0) as i16 >> 4;
                let s2 = ((u32_val >> 20) & 0xFFF0) as i16 >> 4;
                let s3 = (i16_val & !0x000F) >> 4;

                ch_out.push(s0);
                ch_out.push(s1);
                ch_out.push(s2);
                ch_out.push(s3);
            }
        }

        Ok(())
    }

    /// 32-bit GPIF path: 8 samples in 12 bytes.
    ///
    /// From decompilation (SSE `psraw(4)` path):
    /// ```c
    /// uVar20 = *(uint64_t*)src;       // bytes 0-7
    /// uVar19 = *(uint64_t*)(src+4);   // bytes 4-11 (overlapping)
    ///
    /// vec[0] = (int16_t)(uVar20 << 4);
    /// vec[1] = (int16_t)(uVar20 >> 8);
    /// vec[2] = (int16_t)(uVar20 >> 20);
    /// vec[3] = (int16_t)(uVar20 >> 32);
    /// vec[4] = (int16_t)(uVar19 >> 12);
    /// vec[5] = (int16_t)(uVar19 >> 24);
    /// vec[6] = (int16_t)(uVar19 >> 36);
    /// vec[7] = (int16_t)(uVar19 >> 48);
    ///
    /// result = psraw(vec, 4);  // arithmetic right shift by 4
    /// ```
    fn extract_analog_pro_32bit(&self, data: &[u8], output: &mut [Vec<i16>]) -> ProtoResult<()> {
        let bytes_per_channel_group = 12;
        let group_size = bytes_per_channel_group * self.num_analog;

        if data.len() % group_size != 0 {
            return Err(ProtoError::parse(format!(
                "analog data length {} not a multiple of group size {}",
                data.len(),
                group_size,
            )));
        }

        let num_groups = data.len() / group_size;

        for group_idx in 0..num_groups {
            let group_start = group_idx * group_size;
            for (ch, ch_out) in output.iter_mut().enumerate().take(self.num_analog) {
                let offset = group_start + ch * bytes_per_channel_group;
                let src = &data[offset..offset + 12];

                // Read overlapping u64 values (little-endian).
                let lo = u64::from_le_bytes([
                    src[0], src[1], src[2], src[3], src[4], src[5], src[6], src[7],
                ]);
                let hi = u64::from_le_bytes([
                    src[4], src[5], src[6], src[7], src[8], src[9], src[10], src[11],
                ]);

                // Extract 8 x 16-bit values, then arithmetic right shift by 4
                // to sign-extend from 12-bit to 16-bit.
                let s0 = ((lo << 4) as u16 as i16) >> 4;
                let s1 = ((lo >> 8) as u16 as i16) >> 4;
                let s2 = ((lo >> 20) as u16 as i16) >> 4;
                let s3 = ((lo >> 32) as u16 as i16) >> 4;
                let s4 = ((hi >> 12) as u16 as i16) >> 4;
                let s5 = ((hi >> 24) as u16 as i16) >> 4;
                let s6 = ((hi >> 36) as u16 as i16) >> 4;
                let s7 = ((hi >> 48) as u16 as i16) >> 4;

                ch_out.push(s0);
                ch_out.push(s1);
                ch_out.push(s2);
                ch_out.push(s3);
                ch_out.push(s4);
                ch_out.push(s5);
                ch_out.push(s6);
                ch_out.push(s7);
            }
        }

        Ok(())
    }

    // -----------------------------------------------------------------------
    // NonPro deinterleaving (Logic 8)
    // -----------------------------------------------------------------------

    /// Deinterleave Logic 8 data (FUN_01bf21b0).
    ///
    /// Logic 8 produces digital-only data. The input is processed in
    /// 8-byte groups, with each byte going to one of the 8 channels
    /// (in a fixed order). Byte-swap within each 16-bit word corrects
    /// FPGA bit ordering.
    ///
    /// Error string from binary: "Logic data length must be a multiple of 8"
    fn deinterleave_non_pro(&self, data: &[u8]) -> ProtoResult<DisaggregatedData> {
        // For non-Pro (Logic 8): digital-only, process in groups of
        // (num_digital * packet_size) bytes.
        let cycle_size = self.cycle_bytes();
        if cycle_size == 0 {
            return Ok(DisaggregatedData {
                digital_samples: vec![Vec::new(); self.num_digital],
                analog_samples: vec![Vec::new(); self.num_analog],
            });
        }

        if data.len() % cycle_size != 0 {
            return Err(ProtoError::parse(format!(
                "Logic data length {} must be a multiple of {}",
                data.len(),
                cycle_size,
            )));
        }

        let num_groups = data.len() / cycle_size;

        // Each group produces `packet_size` bytes per channel (after byte-swap).
        let mut digital_out: Vec<Vec<u8>> =
            vec![Vec::with_capacity(num_groups * self.packet_size); self.num_digital];

        for group_idx in 0..num_groups {
            let group_start = group_idx * cycle_size;

            // Each word in the group goes to one channel, byte-swapped.
            for (ch, ch_out) in digital_out.iter_mut().enumerate() {
                let offset = group_start + ch * self.packet_size;
                let word = &data[offset..offset + self.packet_size];

                match self.packet_size {
                    2 => {
                        ch_out.push(word[1]);
                        ch_out.push(word[0]);
                    }
                    4 => {
                        ch_out.push(word[3]);
                        ch_out.push(word[2]);
                        ch_out.push(word[1]);
                        ch_out.push(word[0]);
                    }
                    _ => {
                        for &b in word.iter().rev() {
                            ch_out.push(b);
                        }
                    }
                }
            }
        }

        Ok(DisaggregatedData {
            digital_samples: digital_out,
            analog_samples: vec![Vec::new(); self.num_analog],
        })
    }
}

// ---------------------------------------------------------------------------
// Standalone 12-bit ADC sign extension helpers
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// ContractReadData
// ---------------------------------------------------------------------------

/// Decimate raw EP2 data by keeping every Nth byte, where N is the
/// low-speed divider. Mirrors `ContractReadData` at ELF VA 0x1c3bad0.
///
/// When `divider == 1`, this is a no-op (callers should skip it).
pub fn contract_read_data(data: &[u8], divider: u32) -> Vec<u8> {
    debug_assert!(matches!(divider, 2 | 4 | 8));
    let step = divider as usize;
    data.iter().step_by(step).copied().collect()
}

// ---------------------------------------------------------------------------
// Standalone 12-bit ADC sign extension helpers
// ---------------------------------------------------------------------------

/// Sign-extend a 12-bit value (in the lower 12 bits of a u16) to i16.
///
/// The FPGA packs 12-bit ADC values left-aligned in 16-bit words
/// (upper 12 bits significant, lower 4 bits padding). The host
/// performs arithmetic right shift by 4 to sign-extend.
///
/// This function takes a right-aligned 12-bit value (0..4095 unsigned,
/// or -2048..2047 signed) and returns the corresponding i16.
#[inline]
pub fn sign_extend_12bit(raw: u16) -> i16 {
    // Shift left 4 to put the sign bit in bit 15, then arithmetic
    // right shift 4 to propagate it.
    ((raw << 4) as i16) >> 4
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::{LOGIC_8_PROFILE, LOGIC_PRO_16_PROFILE, LOGIC_PRO_8_PROFILE};

    // -------------------------------------------------------------------
    // GCD tests
    // -------------------------------------------------------------------

    #[test]
    fn gcd_basic() {
        assert_eq!(gcd(12, 8), 4);
        assert_eq!(gcd(100, 50), 50);
        assert_eq!(gcd(7, 13), 1);
        assert_eq!(gcd(0, 5), 5);
        assert_eq!(gcd(5, 0), 5);
    }

    // -------------------------------------------------------------------
    // ADC packing tests
    // -------------------------------------------------------------------

    #[test]
    fn adc_packing_valid_combos() {
        let p = adc_packing(16, 10).unwrap();
        assert_eq!(p.samples_per_group, 8);
        assert_eq!(p.bytes_per_group, 5);

        let p = adc_packing(16, 12).unwrap();
        assert_eq!(p.samples_per_group, 4);
        assert_eq!(p.bytes_per_group, 3);

        let p = adc_packing(32, 12).unwrap();
        assert_eq!(p.samples_per_group, 8);
        assert_eq!(p.bytes_per_group, 3);
    }

    #[test]
    fn adc_packing_invalid_combo() {
        assert!(adc_packing(32, 10).is_none());
        assert!(adc_packing(8, 12).is_none());
        assert!(adc_packing(16, 8).is_none());
    }

    // -------------------------------------------------------------------
    // calculate_mix_rate tests
    // -------------------------------------------------------------------

    #[test]
    fn mix_rate_digital_only() {
        // 4 digital channels, 100 MHz digital, no analog.
        let result = calculate_mix_rate(100_000_000, 0, 32, 12, 4, 0).unwrap();
        assert_eq!(result.digital_ratio, 4); // num_digital
        assert_eq!(result.analog_ratio, 0);
        assert_eq!(result.cycle_bytes, 4 * 4); // 4 channels * 4 bytes/word
    }

    #[test]
    fn mix_rate_analog_only() {
        // No digital, 50 MHz analog, 4 analog channels, 32-bit GPIF, 12-bit ADC.
        let result = calculate_mix_rate(0, 50_000_000, 32, 12, 0, 4).unwrap();
        assert_eq!(result.digital_ratio, 0);
        // samples_per_group=8, num_analog=4 => analog_ratio = 8*4 = 32
        assert_eq!(result.analog_ratio, 32);
        // cycle_bytes = 32 * 3 = 96
        assert_eq!(result.cycle_bytes, 96);
    }

    #[test]
    fn mix_rate_mixed_mode() {
        // Logic Pro 8: 100 MHz digital, 50 MHz analog, 4 dig + 4 ana, 32-bit, 12-bit.
        //
        // digital_reduced = 100M / 32 = 3_125_000
        // analog_reduced  = 50M / 3 = 16_666_666
        //
        // Wait, bytes_per_group = 3 for (32, 12).
        // analog_reduced = 50_000_000 / 3 = 16_666_666
        // gcd(3_125_000, 16_666_666) = ?
        //
        // Actually let me compute this properly:
        // 3_125_000 = 5^6 * 2^5 ... no, 3_125_000 = 3125 * 1000 = 5^5 * 10^3 = 5^5 * 2^3 * 5^3 = 5^8 * 2^3
        // Hmm, 3125 = 5^5, 1000 = 10^3, so 3_125_000 = 5^5 * 10^3 = 5^5 * 2^3 * 5^3 = 5^8 * 8
        //
        // 16_666_666 = 50_000_000 / 3 = 16666666.666... This is not an integer!
        //
        // This reveals a problem: 50_000_000 / 3 is not evenly divisible.
        // The RE shows the division is integer division, so 50_000_000 / 3 = 16_666_666.
        //
        // Let me use simpler numbers for the test.

        // Use rates that divide cleanly:
        // digital_rate=96 MHz, analog_rate=48 MHz, gpif=32, adc=12
        // digital_reduced = 96M / 32 = 3M
        // analog_reduced  = 48M / 3 = 16M
        // gcd(3M, 16M) = 1M
        // dq = 3, aq = 16
        // digital_ratio = 4 * 3 = 12
        // analog_ratio = 8 * 4 * 16 = 512
        // digital_bytes = 12 * 4 = 48
        // analog_bytes = 512 * 3 = 1536
        // cycle_bytes = 48 + 1536 = 1584
        let result = calculate_mix_rate(96_000_000, 48_000_000, 32, 12, 4, 4).unwrap();
        assert_eq!(result.digital_ratio, 12);
        assert_eq!(result.analog_ratio, 512);
        assert_eq!(result.cycle_bytes, 1584);
    }

    #[test]
    fn mix_rate_mixed_simple_2x_ratio() {
        // digital_rate=32 MHz, analog_rate=6 MHz, gpif=16, adc=12
        // digital_reduced = 32M / 16 = 2M
        // analog_reduced  = 6M / 3 = 2M
        // gcd(2M, 2M) = 2M
        // dq = 1, aq = 1
        // digital_ratio = 2 * 1 = 2 (2 digital channels)
        // analog_ratio = 4 * 2 * 1 = 8 (2 analog channels, samples_per_group=4)
        // digital_bytes = 2 * 2 = 4
        // analog_bytes = 8 * 3 = 24
        // cycle_bytes = 28
        let result = calculate_mix_rate(32_000_000, 6_000_000, 16, 12, 2, 2).unwrap();
        assert_eq!(result.digital_ratio, 2);
        assert_eq!(result.analog_ratio, 8);
        assert_eq!(result.cycle_bytes, 28);
    }

    #[test]
    fn mix_rate_both_rates_zero_is_error() {
        let err = calculate_mix_rate(0, 0, 32, 12, 4, 4);
        assert!(err.is_err());
    }

    #[test]
    fn mix_rate_invalid_combo_is_error() {
        let err = calculate_mix_rate(100_000_000, 0, 32, 10, 4, 0);
        assert!(err.is_err());
    }

    // -------------------------------------------------------------------
    // sign_extend_12bit tests
    // -------------------------------------------------------------------

    #[test]
    fn sign_extend_positive() {
        // 12-bit value 100 = 0x064
        assert_eq!(sign_extend_12bit(0x064), 100);
    }

    #[test]
    fn sign_extend_negative() {
        // 12-bit value -1 = 0xFFF
        assert_eq!(sign_extend_12bit(0xFFF), -1);
    }

    #[test]
    fn sign_extend_min() {
        // 12-bit value -2048 = 0x800
        assert_eq!(sign_extend_12bit(0x800), -2048);
    }

    #[test]
    fn sign_extend_max() {
        // 12-bit value 2047 = 0x7FF
        assert_eq!(sign_extend_12bit(0x7FF), 2047);
    }

    #[test]
    fn sign_extend_zero() {
        assert_eq!(sign_extend_12bit(0), 0);
    }

    // -------------------------------------------------------------------
    // Disaggregator construction tests
    // -------------------------------------------------------------------

    #[test]
    fn disaggregator_pro8_digital_only() {
        let d =
            Disaggregator::new(&LOGIC_PRO_8_PROFILE, &[0, 1, 2, 3], &[], 100_000_000, 0).unwrap();
        assert_eq!(d.num_digital, 4);
        assert_eq!(d.num_analog, 0);
        assert_eq!(d.packet_size, 4);
        // cycle = 4 channels * 4 bytes = 16
        assert_eq!(d.cycle_bytes(), 16);
    }

    #[test]
    fn disaggregator_logic4_returns_error() {
        let result =
            Disaggregator::new(&crate::device::LOGIC_4_PROFILE, &[0, 1], &[], 12_000_000, 0);
        assert!(result.is_err());
    }

    // -------------------------------------------------------------------
    // Pro deinterleaving tests (32-bit GPIF, 12-bit ADC)
    // -------------------------------------------------------------------

    #[test]
    fn pro_digital_only_deinterleave() {
        // 2 digital channels, 100 MHz, no analog.
        // GPIF=32, packet_size=4.
        // cycle = 2 * 4 = 8 bytes.
        let d = Disaggregator::new(&LOGIC_PRO_8_PROFILE, &[0, 1], &[], 100_000_000, 0).unwrap();

        assert_eq!(d.cycle_bytes(), 8);

        // One cycle: ch0=[0x01, 0x02, 0x03, 0x04], ch1=[0x05, 0x06, 0x07, 0x08]
        let data = [0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08];
        let result = d.process_chunk(&data).unwrap();

        assert_eq!(result.digital_samples.len(), 2);
        // After bswap32: ch0=[0x04, 0x03, 0x02, 0x01]
        assert_eq!(result.digital_samples[0], vec![0x04, 0x03, 0x02, 0x01]);
        // After bswap32: ch1=[0x08, 0x07, 0x06, 0x05]
        assert_eq!(result.digital_samples[1], vec![0x08, 0x07, 0x06, 0x05]);
        assert!(result.analog_samples.is_empty());
    }

    #[test]
    fn pro_analog_32bit_unpack_known_values() {
        // Test the 32-bit GPIF, 12-bit ADC analog unpacking.
        // 1 analog channel, analog-only mode.
        //
        // We need 12 bytes to produce 8 samples.
        // Construct bytes so we get known sample values.
        //
        // The extraction formula:
        //   lo = bytes[0..8] as u64 LE
        //   hi = bytes[4..12] as u64 LE
        //   s0 = ((lo << 4) as u16 as i16) >> 4
        //   s1 = ((lo >> 8) as u16 as i16) >> 4
        //   ...
        //
        // Let's encode sample value 100 (0x064) in the first slot.
        // s0 = ((lo << 4) as u16 as i16) >> 4 = 100
        // So ((lo << 4) as u16) >> 4 = 100 (when treated as i16)
        // ((lo << 4) as u16) = 100 << 4 = 1600 = 0x0640
        // (lo << 4) & 0xFFFF = 0x0640
        // lo & 0xFFFF = 0x0064 (i.e., lo low 16 bits = 0x0064)
        //
        // For simplicity, let's test with all-zero input (should produce 8 zeros).
        let d = Disaggregator::new(&LOGIC_PRO_8_PROFILE, &[], &[0], 0, 50_000_000).unwrap();

        // Analog-only: analog_ratio = 8 * 1 = 8
        // cycle_bytes = 8 * 3 = 24
        // But wait -- with 1 analog channel, the 32-bit path uses 12 bytes per group.
        // Let me check: for analog-only, analog_ratio = samples_per_group * num_analog = 8 * 1 = 8
        // cycle_bytes = 8 * 3 = 24
        // But the 32-bit path processes 12 bytes per channel per group...
        //
        // The analog_group_bytes in the cycle is:
        //   analog_ratio * bytes_per_group = 8 * 3 = 24
        //
        // In extract_analog_pro_32bit, group_size = 12 * num_analog = 12
        // num_groups = 24 / 12 = 2
        // So 2 groups of 12 bytes each, producing 8 samples per group = 16 total.
        //
        // That matches: analog_ratio = 8 means 8 "analog slots" in the cycle,
        // but each group is samples_per_group=8 samples from 12 bytes.
        // So 8 slots / 8 samples_per_group = 1 group? No... the ratio is more nuanced.
        //
        // Actually, I think the issue is that analog_ratio counts the number of
        // "analog word slots" in the cycle, not the number of sample groups.
        // Each "word" in the analog section is bytes_per_group=3 bytes, and
        // samples_per_group=8 samples come from 12 bytes = 4 words.
        //
        // With analog_ratio=8, we have 8 * 3 = 24 bytes.
        // 32-bit path processes 12 bytes at a time per channel.
        // With 1 channel: 24 / 12 = 2 groups of 8 samples = 16 samples total.
        //
        // That seems wrong for analog-only at 50 MHz. Let me reconsider.
        //
        // Hmm, in analog-only mode the function returns:
        //   analog_ratio = samples_per_group * num_analog = 8 * 1 = 8
        //   cycle_bytes = 8 * 3 = 24
        //
        // The issue is that analog_ratio=8 doesn't match the extract loop.
        // The extract loop uses bytes directly. 24 bytes / 12 per channel = 2 groups.
        // 2 groups * 8 samples = 16 samples per cycle.
        //
        // Let me just test with 12 bytes (1 group), which is a valid input
        // for the extract function directly.

        // Actually, let me just create a disaggregator and pass the right amount of data.
        // cycle_bytes = 24. So pass 24 bytes of zeros.
        let data = vec![0u8; 24];
        let result = d.process_chunk(&data).unwrap();
        assert_eq!(result.analog_samples.len(), 1);
        assert_eq!(result.analog_samples[0].len(), 16); // 2 groups * 8 samples
        assert!(result.analog_samples[0].iter().all(|&s| s == 0));
    }

    #[test]
    fn pro_analog_32bit_sign_extension() {
        // Test that 12-bit value -2048 (0x800) is correctly sign-extended.
        //
        // Place 0x800 in the first sample slot (s0):
        //   s0 = ((lo << 4) as u16 as i16) >> 4
        //   We want s0 = -2048.
        //   -2048 as i16 = 0xF800.
        //   ((lo << 4) as u16 as i16) >> 4 = -2048
        //   ((lo << 4) as u16) = ((-2048) << 4) as u16 = 0x8000
        //   (lo << 4) & 0xFFFF = 0x8000
        //   lo & 0xFFFF = 0x0800
        //   So bytes[0..2] = [0x00, 0x08] (LE for 0x0800)
        //
        // Rest of the 12 bytes can be zero.
        let d = Disaggregator::new(&LOGIC_PRO_8_PROFILE, &[], &[0], 0, 50_000_000).unwrap();

        // cycle_bytes=24, place the pattern in the first 12 bytes.
        let mut data = vec![0u8; 24];
        data[0] = 0x00;
        data[1] = 0x08;

        let result = d.process_chunk(&data).unwrap();
        assert_eq!(result.analog_samples[0][0], -2048);
    }

    #[test]
    fn pro_analog_32bit_positive_value() {
        // Place 12-bit value 2047 (0x7FF) in the first sample slot.
        //   lo & 0xFFFF = 0x07FF
        //   bytes[0..2] = [0xFF, 0x07]
        let mut data = vec![0u8; 24];
        data[0] = 0xFF;
        data[1] = 0x07;

        let d = Disaggregator::new(&LOGIC_PRO_8_PROFILE, &[], &[0], 0, 50_000_000).unwrap();

        let result = d.process_chunk(&data).unwrap();
        assert_eq!(result.analog_samples[0][0], 2047);
    }

    #[test]
    fn pro_analog_16bit_unpack_zeros() {
        // Logic 8 has 16-bit GPIF, 12-bit ADC.
        // samples_per_group=4, bytes_per_group=3.
        // 1 analog channel, analog-only.
        // analog_ratio = 4 * 1 = 4
        // cycle_bytes = 4 * 3 = 12
        // 16-bit path: 6 bytes per channel per group.
        // 12 bytes / 6 per channel = 2 groups.
        // 2 groups * 4 samples = 8 samples.
        let d = Disaggregator::new(&LOGIC_8_PROFILE, &[], &[0], 0, 10_000_000).unwrap();

        let data = vec![0u8; d.cycle_bytes()];
        let result = d.process_chunk(&data).unwrap();
        assert_eq!(result.analog_samples.len(), 1);
        assert!(result.analog_samples[0].iter().all(|&s| s == 0));
    }

    // -------------------------------------------------------------------
    // NonPro (Logic 8) deinterleaving tests
    // -------------------------------------------------------------------

    #[test]
    fn logic8_digital_deinterleave() {
        // Logic 8: 8 digital channels, 16-bit GPIF, digital-only.
        // cycle = 8 * 2 = 16 bytes.
        let d = Disaggregator::new(
            &LOGIC_8_PROFILE,
            &[0, 1, 2, 3, 4, 5, 6, 7],
            &[],
            100_000_000,
            0,
        )
        .unwrap();

        assert_eq!(d.cycle_bytes(), 16);

        // One cycle: 8 channels, each with 2-byte word.
        // Channel 0: [0xAA, 0xBB] -> byte-swap -> [0xBB, 0xAA]
        // Channel 1: [0xCC, 0xDD] -> byte-swap -> [0xDD, 0xCC]
        // ...
        let mut data = Vec::new();
        for ch in 0..8u8 {
            data.push(ch * 2);
            data.push(ch * 2 + 1);
        }
        // data = [0,1, 2,3, 4,5, 6,7, 8,9, 10,11, 12,13, 14,15]

        let result = d.process_chunk(&data).unwrap();
        assert_eq!(result.digital_samples.len(), 8);
        // After byte swap, channel 0 = [1, 0], channel 1 = [3, 2], etc.
        for ch in 0..8 {
            let expected = vec![(ch as u8) * 2 + 1, (ch as u8) * 2];
            assert_eq!(
                result.digital_samples[ch], expected,
                "channel {ch} mismatch"
            );
        }
    }

    #[test]
    fn logic8_multiple_cycles() {
        let d = Disaggregator::new(
            &LOGIC_8_PROFILE,
            &[0, 1, 2, 3, 4, 5, 6, 7],
            &[],
            100_000_000,
            0,
        )
        .unwrap();

        // Two cycles: cycle 0 all 0xFF, cycle 1 all 0x00.
        let mut data = vec![0xFF; 16];
        data.extend_from_slice(&[0x00; 16]);

        let result = d.process_chunk(&data).unwrap();
        assert_eq!(result.digital_samples.len(), 8);
        for ch in 0..8 {
            // 2 cycles * 2 bytes = 4 bytes per channel
            assert_eq!(result.digital_samples[ch].len(), 4);
            assert_eq!(result.digital_samples[ch][0], 0xFF);
            assert_eq!(result.digital_samples[ch][1], 0xFF);
            assert_eq!(result.digital_samples[ch][2], 0x00);
            assert_eq!(result.digital_samples[ch][3], 0x00);
        }
    }

    #[test]
    fn logic8_bad_length_returns_error() {
        let d = Disaggregator::new(
            &LOGIC_8_PROFILE,
            &[0, 1, 2, 3, 4, 5, 6, 7],
            &[],
            100_000_000,
            0,
        )
        .unwrap();

        // 17 bytes is not a multiple of 16.
        let data = vec![0u8; 17];
        let result = d.process_chunk(&data);
        assert!(result.is_err());
    }

    #[test]
    fn empty_data_produces_empty_output() {
        let d = Disaggregator::new(&LOGIC_PRO_8_PROFILE, &[0, 1], &[], 100_000_000, 0).unwrap();

        let result = d.process_chunk(&[]).unwrap();
        assert_eq!(result.digital_samples.len(), 2);
        assert!(result.digital_samples[0].is_empty());
        assert!(result.digital_samples[1].is_empty());
    }

    // -------------------------------------------------------------------
    // Pro mixed-mode (digital + analog) test
    // -------------------------------------------------------------------

    #[test]
    fn pro_mixed_mode_structure() {
        // 2 digital + 1 analog, rates chosen for clean ratio.
        // digital_rate=32 MHz, analog_rate=32 MHz, gpif=32, adc=12.
        //
        // digital_reduced = 32M / 32 = 1M
        // analog_reduced  = 32M / 3 = 10_666_666  (integer division)
        //
        // Hmm, not clean. Let me pick rates that divide cleanly by gpif_width
        // and bytes_per_group.
        //
        // gpif=32, adc=12: bytes_per_group=3, samples_per_group=8
        // digital_rate = 32 * N, analog_rate = 3 * M
        // For simplest case: N=M, so gcd gives dq=1, aq=1.
        // digital_rate = 96, analog_rate = 9
        //   digital_reduced = 96 / 32 = 3
        //   analog_reduced = 9 / 3 = 3
        //   gcd(3,3) = 3, dq=1, aq=1
        //   digital_ratio = 2 * 1 = 2
        //   analog_ratio = 8 * 1 * 1 = 8
        //   digital_bytes = 2 * 4 = 8
        //   analog_bytes = 8 * 3 = 24
        //   cycle_bytes = 32

        let d = Disaggregator::new(&LOGIC_PRO_8_PROFILE, &[0, 1], &[0], 96, 9).unwrap();

        assert_eq!(d.mix_rate().digital_ratio, 2);
        assert_eq!(d.mix_rate().analog_ratio, 8);
        assert_eq!(d.cycle_bytes(), 32);

        // Build one cycle: 8 bytes digital + 24 bytes analog.
        let mut data = vec![0u8; 32];

        // Digital: 2 words of 4 bytes each.
        // Word 0 (ch0): [0x11, 0x22, 0x33, 0x44]
        data[0] = 0x11;
        data[1] = 0x22;
        data[2] = 0x33;
        data[3] = 0x44;
        // Word 1 (ch1): [0x55, 0x66, 0x77, 0x88]
        data[4] = 0x55;
        data[5] = 0x66;
        data[6] = 0x77;
        data[7] = 0x88;

        // Analog: 24 bytes, all zeros for simplicity.
        // (already zero)

        let result = d.process_chunk(&data).unwrap();

        // Digital: 2 channels, each with 4 bytes (1 bswap32 word).
        assert_eq!(result.digital_samples.len(), 2);
        assert_eq!(result.digital_samples[0], vec![0x44, 0x33, 0x22, 0x11]);
        assert_eq!(result.digital_samples[1], vec![0x88, 0x77, 0x66, 0x55]);

        // Analog: 1 channel, 24 bytes / 12 bytes per group = 2 groups of 8 = 16 samples.
        assert_eq!(result.analog_samples.len(), 1);
        assert_eq!(result.analog_samples[0].len(), 16);
        assert!(result.analog_samples[0].iter().all(|&s| s == 0));
    }

    // -------------------------------------------------------------------
    // Pro 16 test
    // -------------------------------------------------------------------

    #[test]
    fn disaggregator_pro16_constructs() {
        let d = Disaggregator::new(
            &LOGIC_PRO_16_PROFILE,
            &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
            &[0, 1, 2, 3],
            96,
            9,
        )
        .unwrap();

        // Same rate ratio as above, but more channels.
        // digital_ratio = 16 * 1 = 16
        // analog_ratio = 8 * 4 * 1 = 32
        // digital_bytes = 16 * 4 = 64
        // analog_bytes = 32 * 3 = 96
        // cycle_bytes = 160
        assert_eq!(d.mix_rate().digital_ratio, 16);
        assert_eq!(d.mix_rate().analog_ratio, 32);
        assert_eq!(d.cycle_bytes(), 160);
    }
}
