//! AFE auto-scale algorithm for automatic voltage range selection.
//!
//! Implements the two-phase auto-scale system reverse-engineered from Saleae
//! Logic 2 v2.4.40's `libgraph_server_shared.so`. See `re/notes/afe-autoscale.md`
//! for the full RE analysis.
//!
//! # Phases
//!
//! 1. **FullScaleKernel** ("horizontal"): Captures at the widest voltage range
//!    and classifies the signal via zero-crossing detection with hysteresis.
//!    Determines whether the signal is DC, AC, or too-fast.
//!
//! 2. **VerticalScaleKernel**: Measures actual min/max ADC values, detects
//!    saturation, and computes an optimal voltage range with 20% margin.
//!
//! # Constants
//!
//! All thresholds and constants are confirmed from decompilation:
//!
//! | Constant | Value | Source |
//! |----------|-------|--------|
//! | Hysteresis positive | +49 ADC codes | FullScaleKernel comparison |
//! | Hysteresis negative | -50 ADC codes | FullScaleKernel comparison |
//! | Min crossings (DC) | 6 | `local_6a8 < 6` |
//! | Max crossings (too fast) | 2600 | `local_6a8 < 0xa28` |
//! | Margin factor | 0.2 (20%) | Both kernels |
//! | Min margin (full-scale frac) | 1/128 | VerticalScaleKernel |
//! | Min margin (DC path) | 1.0 V | FullScaleKernel |
//! | Max iterations | 5 | Both kernels |
//! | Max scope channels | 4 | `std::array<T, 4>` usage |

use saleae_proto::calibration::VoltageTransform;
use tracing::debug;

// ---------------------------------------------------------------------------
// Constants from RE
// ---------------------------------------------------------------------------

/// Maximum number of scope channels for auto-scale (confirmed from binary).
pub const MAX_SCOPE_CHANNELS: usize = 4;

/// Maximum number of auto-scale iterations per phase.
pub const MAX_ITERATIONS: usize = 5;

/// Hysteresis threshold for positive-going transition (ADC codes).
const HYSTERESIS_POS: i16 = 49;

/// Hysteresis threshold for negative-going transition (ADC codes).
const HYSTERESIS_NEG: i16 = -50;

/// Below this crossing count, signal is classified as DC.
const CROSSING_THRESHOLD_DC: u32 = 6;

/// At or above this crossing count, signal is too fast for auto-scale.
const CROSSING_THRESHOLD_TOO_FAST: u32 = 2600;

/// Margin multiplier applied to signal amplitude (20%).
const MARGIN_FACTOR: f64 = 0.2;

/// Minimum margin as fraction of full-scale range (1/128).
const MIN_MARGIN_FULL_SCALE_FRAC: f64 = 1.0 / 128.0;

/// Minimum margin floor for the DC path in FullScaleKernel (volts).
const MIN_MARGIN_DC_FLOOR: f64 = 1.0;

/// Default ADC saturation minimum (12-bit signed).
const ADC_SATURATION_MIN: i16 = -2048;

/// Default ADC saturation maximum (12-bit signed).
const ADC_SATURATION_MAX: i16 = 2047;

// ---------------------------------------------------------------------------
// Signal classification
// ---------------------------------------------------------------------------

/// Signal type classification from FullScaleKernel zero-crossing analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalType {
    /// DC or very low frequency (fewer than 6 zero crossings).
    Dc,
    /// AC signal with measurable frequency (6 to 2599 crossings).
    Ac,
    /// Signal frequency too high for auto-scale (2600+ crossings).
    TooFast,
}

// ---------------------------------------------------------------------------
// Zero-crossing hysteresis state machine
// ---------------------------------------------------------------------------

/// State of the hysteresis-based zero-crossing detector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CrossingState {
    /// Initial / post-crossing state. Waiting for signal to exceed threshold.
    Undecided,
    /// Signal is in the negative region (below -50).
    Negative,
    /// Signal is in the positive region (above +49).
    Positive,
}

// ---------------------------------------------------------------------------
// FullScaleKernel
// ---------------------------------------------------------------------------

/// Phase 1: Full-scale zero-crossing detector.
///
/// Processes ADC samples at the widest voltage range and counts
/// zero crossings with hysteresis to classify the signal type.
///
/// The hysteresis prevents noise from triggering false crossings:
/// - Transition to Positive requires sample > +49
/// - Transition to Negative requires sample < -50
/// - A crossing is counted when the signal returns through zero
///   from either side.
#[derive(Debug, Clone)]
pub struct FullScaleKernel {
    /// Current hysteresis state.
    state: CrossingState,
    /// Total zero crossings counted.
    crossings: u32,
    /// Total samples processed.
    total_samples: u64,
}

impl FullScaleKernel {
    /// Create a new FullScaleKernel in the initial state.
    pub fn new() -> Self {
        Self {
            state: CrossingState::Undecided,
            crossings: 0,
            total_samples: 0,
        }
    }

    /// Reset the kernel to initial state.
    pub fn reset(&mut self) {
        self.state = CrossingState::Undecided;
        self.crossings = 0;
        self.total_samples = 0;
    }

    /// Process a batch of ADC samples and update crossing count.
    ///
    /// Samples are signed 12-bit values in the range [-2048, 2047].
    /// Can be called multiple times for streaming data.
    pub fn process(&mut self, samples: &[i16]) {
        for &sample in samples {
            match self.state {
                CrossingState::Undecided => {
                    if sample > HYSTERESIS_POS {
                        self.state = CrossingState::Positive;
                    } else if sample < HYSTERESIS_NEG {
                        self.state = CrossingState::Negative;
                    }
                }
                CrossingState::Negative => {
                    // Crossing occurs when signal returns to zero or above.
                    if sample >= 0 {
                        self.crossings += 1;
                        self.state = CrossingState::Undecided;
                    }
                }
                CrossingState::Positive => {
                    // Crossing occurs when signal goes to zero or below.
                    // Binary uses `sVar5 < 1` which is `<= 0`.
                    if sample <= 0 {
                        self.crossings += 1;
                        self.state = CrossingState::Undecided;
                    }
                }
            }
        }
        self.total_samples += samples.len() as u64;
    }

    /// Return the number of zero crossings detected so far.
    pub fn crossings(&self) -> u32 {
        self.crossings
    }

    /// Return the total number of samples processed.
    pub fn total_samples(&self) -> u64 {
        self.total_samples
    }

    /// Classify the signal based on accumulated crossing count.
    pub fn classify(&self) -> SignalType {
        if self.crossings >= CROSSING_THRESHOLD_TOO_FAST {
            SignalType::TooFast
        } else if self.crossings < CROSSING_THRESHOLD_DC {
            SignalType::Dc
        } else {
            SignalType::Ac
        }
    }

    /// Estimate the signal frequency in Hz.
    ///
    /// Only meaningful when `classify()` returns `SignalType::Ac`.
    /// Returns `None` if there are not enough crossings or samples.
    ///
    /// Formula from decompilation:
    /// `frequency = (crossings / 2) / (total_samples / sample_rate)`
    pub fn estimate_frequency(&self, sample_rate: u64) -> Option<f64> {
        if self.crossings < CROSSING_THRESHOLD_DC || self.total_samples == 0 || sample_rate == 0 {
            return None;
        }
        let half_cycles = self.crossings as f64 / 2.0;
        let duration_seconds = self.total_samples as f64 / sample_rate as f64;
        Some(half_cycles / duration_seconds)
    }
}

impl Default for FullScaleKernel {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// VerticalScaleKernel
// ---------------------------------------------------------------------------

/// Per-channel result from the VerticalScaleKernel.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ChannelScaleResult {
    /// Channel needs measurement (initial state).
    NeedsMeasurement,
    /// Measurement complete: min/max voltages and recommended range.
    Measured {
        /// Minimum voltage observed.
        min_voltage: f64,
        /// Maximum voltage observed.
        max_voltage: f64,
        /// Recommended range minimum (with margin).
        range_min: f64,
        /// Recommended range maximum (with margin).
        range_max: f64,
    },
    /// ADC is saturated -- signal exceeds the current range.
    Saturated,
}

/// Phase 2: Vertical scale min/max tracker with saturation detection.
///
/// Measures the actual signal amplitude per channel and computes the
/// optimal voltage range with 20% margin. Detects ADC saturation
/// (signal clipping at the rail) to trigger range expansion.
#[derive(Debug, Clone)]
pub struct VerticalScaleKernel {
    /// Per-channel tracked minimum ADC value.
    min_adc: [i16; MAX_SCOPE_CHANNELS],
    /// Per-channel tracked maximum ADC value.
    max_adc: [i16; MAX_SCOPE_CHANNELS],
    /// Whether each channel has received any samples.
    has_data: [bool; MAX_SCOPE_CHANNELS],
    /// Number of active channels.
    num_channels: usize,
    /// ADC saturation floor (typically -2048 for 12-bit).
    sat_min: i16,
    /// ADC saturation ceiling (typically 2047 for 12-bit).
    sat_max: i16,
}

impl VerticalScaleKernel {
    /// Create a new VerticalScaleKernel.
    ///
    /// `num_channels` must be <= `MAX_SCOPE_CHANNELS`.
    /// `adc_bits` determines the saturation thresholds.
    pub fn new(num_channels: usize, adc_bits: u8) -> Self {
        assert!(
            num_channels <= MAX_SCOPE_CHANNELS,
            "num_channels ({}) exceeds MAX_SCOPE_CHANNELS ({})",
            num_channels,
            MAX_SCOPE_CHANNELS,
        );

        let (sat_min, sat_max) = saturation_thresholds(adc_bits);

        Self {
            min_adc: [i16::MAX; MAX_SCOPE_CHANNELS],
            max_adc: [i16::MIN; MAX_SCOPE_CHANNELS],
            has_data: [false; MAX_SCOPE_CHANNELS],
            num_channels,
            sat_min,
            sat_max,
        }
    }

    /// Reset all channel state.
    pub fn reset(&mut self) {
        self.min_adc = [i16::MAX; MAX_SCOPE_CHANNELS];
        self.max_adc = [i16::MIN; MAX_SCOPE_CHANNELS];
        self.has_data = [false; MAX_SCOPE_CHANNELS];
    }

    /// Process ADC samples for a single channel.
    ///
    /// `channel` is the channel index (0-based, must be < num_channels).
    /// `samples` are signed ADC codes (12-bit: [-2048, 2047]).
    pub fn process_channel(&mut self, channel: usize, samples: &[i16]) {
        assert!(
            channel < self.num_channels,
            "channel {} out of range (num_channels={})",
            channel,
            self.num_channels,
        );

        if samples.is_empty() {
            return;
        }

        self.has_data[channel] = true;

        for &sample in samples {
            if sample < self.min_adc[channel] {
                self.min_adc[channel] = sample;
            }
            if sample > self.max_adc[channel] {
                self.max_adc[channel] = sample;
            }
        }
    }

    /// Check whether a channel's signal is saturated (clipping at the ADC rail).
    ///
    /// Saturation means the measured min equals the ADC floor or the measured
    /// max equals the ADC ceiling.
    pub fn is_saturated(&self, channel: usize) -> bool {
        if channel >= self.num_channels || !self.has_data[channel] {
            return false;
        }
        self.min_adc[channel] == self.sat_min || self.max_adc[channel] == self.sat_max
    }

    /// Compute the per-channel scale result.
    ///
    /// The `transform` converts ADC codes to voltages. The `full_scale_min` and
    /// `full_scale_max` define the current voltage range (for computing the
    /// minimum margin as 1/128 of full scale).
    ///
    /// Returns `ChannelScaleResult` for the given channel.
    pub fn compute_result(
        &self,
        channel: usize,
        transform: &VoltageTransform,
        full_scale_min: f64,
        full_scale_max: f64,
    ) -> ChannelScaleResult {
        if channel >= self.num_channels || !self.has_data[channel] {
            return ChannelScaleResult::NeedsMeasurement;
        }

        if self.is_saturated(channel) {
            return ChannelScaleResult::Saturated;
        }

        let min_voltage = transform.apply(self.min_adc[channel]);
        let max_voltage = transform.apply(self.max_adc[channel]);

        let signal_range = max_voltage - min_voltage;
        let range_margin = signal_range * MARGIN_FACTOR;
        let full_scale_margin = (full_scale_max - full_scale_min) * MIN_MARGIN_FULL_SCALE_FRAC;
        let margin = range_margin.max(full_scale_margin);

        let range_min = min_voltage - margin;
        let range_max = max_voltage + margin;

        debug!(
            channel,
            min_voltage,
            max_voltage,
            margin,
            range_min,
            range_max,
            "VerticalScaleKernel: computed range"
        );

        ChannelScaleResult::Measured {
            min_voltage,
            max_voltage,
            range_min,
            range_max,
        }
    }

    /// Return the raw min/max ADC values for a channel.
    ///
    /// Returns `None` if the channel has no data.
    pub fn adc_range(&self, channel: usize) -> Option<(i16, i16)> {
        if channel >= self.num_channels || !self.has_data[channel] {
            return None;
        }
        Some((self.min_adc[channel], self.max_adc[channel]))
    }
}

/// Compute ADC saturation thresholds from bit depth.
///
/// Returns `(min, max)` as signed values:
/// - 12-bit: (-2048, 2047)
/// - 8-bit: (-128, 127) -- though Logic 4 uses unsigned 0-255, this is
///   the signed interpretation.
fn saturation_thresholds(adc_bits: u8) -> (i16, i16) {
    match adc_bits {
        12 => (ADC_SATURATION_MIN, ADC_SATURATION_MAX),
        8 => (-128, 127),
        _ => {
            // Generic: 2's complement range for n bits.
            let half = 1i16 << (adc_bits - 1);
            (-half, half - 1)
        }
    }
}

// ---------------------------------------------------------------------------
// AutoScaler: combines both kernels
// ---------------------------------------------------------------------------

/// Per-channel auto-scale state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelState {
    /// Waiting for Phase 1 (full-scale measurement).
    WaitingFullScale,
    /// Phase 1 complete, waiting for Phase 2 (vertical measurement).
    WaitingVertical,
    /// Auto-scale complete for this channel.
    Done,
    /// Error: signal too fast for auto-scale.
    TooFast,
    /// Error: saturated after max iterations.
    SaturatedError,
}

/// Result of a complete auto-scale operation for one channel.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AutoScaleResult {
    /// Channel index.
    pub channel: usize,
    /// Final state.
    pub state: ChannelState,
    /// Recommended minimum voltage (only valid if state == Done).
    pub min_voltage: f64,
    /// Recommended maximum voltage (only valid if state == Done).
    pub max_voltage: f64,
}

/// Orchestrator that runs both FullScaleKernel and VerticalScaleKernel
/// for up to `MAX_SCOPE_CHANNELS` channels.
///
/// Mirrors the `AutoScaleRunner::OnWaveform` logic from the binary.
///
/// # Usage
///
/// ```ignore
/// let mut scaler = AutoScaler::new(2, 12);
///
/// // Phase 1: feed full-scale samples (all channels at widest range)
/// scaler.process_full_scale(0, &adc_samples_ch0);
/// scaler.process_full_scale(1, &adc_samples_ch1);
///
/// // Classify signals
/// scaler.finish_full_scale();
///
/// // Phase 2: feed samples at current range for measurement
/// scaler.process_vertical(0, &adc_samples_ch0);
/// scaler.process_vertical(1, &adc_samples_ch1);
///
/// // Get results
/// let results = scaler.compute_results(&transforms, &full_scale_ranges);
/// ```
#[derive(Debug, Clone)]
pub struct AutoScaler {
    /// Phase 1 kernel (shared across channels since it classifies
    /// based on crossing count, not per-channel).
    full_scale: Vec<FullScaleKernel>,
    /// Phase 2 kernel.
    vertical: VerticalScaleKernel,
    /// Per-channel state.
    channel_states: [ChannelState; MAX_SCOPE_CHANNELS],
    /// Number of active channels.
    num_channels: usize,
    /// Current iteration count.
    iteration: usize,
}

impl AutoScaler {
    /// Create a new AutoScaler.
    ///
    /// `num_channels` must be <= `MAX_SCOPE_CHANNELS`.
    /// `adc_bits` determines saturation thresholds.
    pub fn new(num_channels: usize, adc_bits: u8) -> Self {
        assert!(
            num_channels <= MAX_SCOPE_CHANNELS,
            "num_channels ({}) exceeds MAX_SCOPE_CHANNELS ({})",
            num_channels,
            MAX_SCOPE_CHANNELS,
        );

        let full_scale = (0..num_channels).map(|_| FullScaleKernel::new()).collect();

        Self {
            full_scale,
            vertical: VerticalScaleKernel::new(num_channels, adc_bits),
            channel_states: [ChannelState::WaitingFullScale; MAX_SCOPE_CHANNELS],
            num_channels,
            iteration: 0,
        }
    }

    /// Number of active channels.
    pub fn num_channels(&self) -> usize {
        self.num_channels
    }

    /// Current iteration count.
    pub fn iteration(&self) -> usize {
        self.iteration
    }

    /// Get the state of a channel.
    pub fn channel_state(&self, channel: usize) -> ChannelState {
        self.channel_states[channel]
    }

    /// Process Phase 1 (full-scale) samples for one channel.
    ///
    /// Feed ADC samples captured at the widest voltage range.
    pub fn process_full_scale(&mut self, channel: usize, samples: &[i16]) {
        assert!(
            channel < self.num_channels,
            "channel {} out of range (num_channels={})",
            channel,
            self.num_channels,
        );
        self.full_scale[channel].process(samples);
    }

    /// Finish Phase 1: classify all channels and transition their state.
    ///
    /// Call this after all full-scale samples have been processed.
    pub fn finish_full_scale(&mut self) {
        for ch in 0..self.num_channels {
            if self.channel_states[ch] != ChannelState::WaitingFullScale {
                continue;
            }

            let signal_type = self.full_scale[ch].classify();
            debug!(
                channel = ch,
                crossings = self.full_scale[ch].crossings(),
                signal_type = ?signal_type,
                "FullScaleKernel classification"
            );

            match signal_type {
                SignalType::TooFast => {
                    self.channel_states[ch] = ChannelState::TooFast;
                }
                SignalType::Dc | SignalType::Ac => {
                    self.channel_states[ch] = ChannelState::WaitingVertical;
                }
            }
        }
    }

    /// Process Phase 2 (vertical scale) samples for one channel.
    ///
    /// Feed ADC samples captured at the current voltage range.
    pub fn process_vertical(&mut self, channel: usize, samples: &[i16]) {
        assert!(
            channel < self.num_channels,
            "channel {} out of range (num_channels={})",
            channel,
            self.num_channels,
        );
        self.vertical.process_channel(channel, samples);
    }

    /// Reset the vertical kernel for a new iteration.
    ///
    /// Called when iterating after saturation detection. Increments the
    /// iteration counter.
    pub fn reset_vertical(&mut self) {
        self.vertical.reset();
        self.iteration += 1;
    }

    /// Whether the maximum iteration count has been reached.
    pub fn max_iterations_reached(&self) -> bool {
        self.iteration >= MAX_ITERATIONS
    }

    /// Compute results for all channels.
    ///
    /// `transforms` provides the ADC-to-voltage conversion for each channel.
    /// `full_scale_ranges` provides the current (min, max) voltage range
    /// for each channel (used for minimum margin calculation).
    ///
    /// Both slices must have length >= `num_channels`.
    pub fn compute_results(
        &mut self,
        transforms: &[VoltageTransform],
        full_scale_ranges: &[(f64, f64)],
    ) -> Vec<AutoScaleResult> {
        assert!(
            transforms.len() >= self.num_channels,
            "transforms length ({}) < num_channels ({})",
            transforms.len(),
            self.num_channels,
        );
        assert!(
            full_scale_ranges.len() >= self.num_channels,
            "full_scale_ranges length ({}) < num_channels ({})",
            full_scale_ranges.len(),
            self.num_channels,
        );

        let mut results = Vec::with_capacity(self.num_channels);

        for ch in 0..self.num_channels {
            let result = match self.channel_states[ch] {
                ChannelState::WaitingVertical => {
                    let (fs_min, fs_max) = full_scale_ranges[ch];
                    let scale_result =
                        self.vertical
                            .compute_result(ch, &transforms[ch], fs_min, fs_max);

                    match scale_result {
                        ChannelScaleResult::Measured {
                            range_min,
                            range_max,
                            ..
                        } => {
                            self.channel_states[ch] = ChannelState::Done;
                            AutoScaleResult {
                                channel: ch,
                                state: ChannelState::Done,
                                min_voltage: range_min,
                                max_voltage: range_max,
                            }
                        }
                        ChannelScaleResult::Saturated => {
                            if self.max_iterations_reached() {
                                self.channel_states[ch] = ChannelState::SaturatedError;
                                AutoScaleResult {
                                    channel: ch,
                                    state: ChannelState::SaturatedError,
                                    min_voltage: full_scale_ranges[ch].0,
                                    max_voltage: full_scale_ranges[ch].1,
                                }
                            } else {
                                // Stay in WaitingVertical for retry.
                                AutoScaleResult {
                                    channel: ch,
                                    state: ChannelState::WaitingVertical,
                                    min_voltage: full_scale_ranges[ch].0,
                                    max_voltage: full_scale_ranges[ch].1,
                                }
                            }
                        }
                        ChannelScaleResult::NeedsMeasurement => AutoScaleResult {
                            channel: ch,
                            state: ChannelState::WaitingVertical,
                            min_voltage: full_scale_ranges[ch].0,
                            max_voltage: full_scale_ranges[ch].1,
                        },
                    }
                }
                state => AutoScaleResult {
                    channel: ch,
                    state,
                    min_voltage: full_scale_ranges.get(ch).map_or(0.0, |r| r.0),
                    max_voltage: full_scale_ranges.get(ch).map_or(0.0, |r| r.1),
                },
            };

            results.push(result);
        }

        results
    }

    /// Access the FullScaleKernel for a specific channel.
    pub fn full_scale_kernel(&self, channel: usize) -> &FullScaleKernel {
        &self.full_scale[channel]
    }

    /// Access the VerticalScaleKernel.
    pub fn vertical_kernel(&self) -> &VerticalScaleKernel {
        &self.vertical
    }
}

// ---------------------------------------------------------------------------
// DC path helper (FullScaleKernel range adjustment)
// ---------------------------------------------------------------------------

/// Compute the voltage range for the DC path in FullScaleKernel.
///
/// When the signal is DC or very slow (< 6 crossings), the FullScaleKernel
/// adjusts the range based on the measured min/max voltages with a margin.
///
/// The margin is `max(signal_range * 0.2, 1.0)` volts. The margin is applied
/// asymmetrically: `0.2 * margin` on each side. This matches the decompiled
/// behavior where `margin * 0.2` is added to each boundary.
pub fn compute_dc_range(min_voltage: f64, max_voltage: f64) -> (f64, f64) {
    let signal_range = max_voltage - min_voltage;
    let margin = (signal_range * MARGIN_FACTOR).max(MIN_MARGIN_DC_FLOOR);
    let asymmetric_margin = margin * MARGIN_FACTOR; // 0.2 * margin per side
    (
        min_voltage - asymmetric_margin,
        max_voltage + asymmetric_margin,
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // FullScaleKernel tests
    // -----------------------------------------------------------------------

    #[test]
    fn full_scale_new_starts_at_zero() {
        let k = FullScaleKernel::new();
        assert_eq!(k.crossings(), 0);
        assert_eq!(k.total_samples(), 0);
        assert_eq!(k.classify(), SignalType::Dc);
    }

    #[test]
    fn full_scale_dc_signal() {
        let mut k = FullScaleKernel::new();
        // Constant value well within hysteresis band -- no crossings.
        let samples: Vec<i16> = vec![10; 1000];
        k.process(&samples);
        assert_eq!(k.crossings(), 0);
        assert_eq!(k.classify(), SignalType::Dc);
        assert_eq!(k.total_samples(), 1000);
    }

    #[test]
    fn full_scale_dc_at_zero() {
        let mut k = FullScaleKernel::new();
        let samples: Vec<i16> = vec![0; 500];
        k.process(&samples);
        assert_eq!(k.crossings(), 0);
        assert_eq!(k.classify(), SignalType::Dc);
    }

    #[test]
    fn full_scale_slowly_varying_dc() {
        let mut k = FullScaleKernel::new();
        // Signal varies slowly between -30 and +30 (within hysteresis band).
        let mut samples = Vec::new();
        for i in 0..1000 {
            let val = ((i as f64 / 100.0).sin() * 30.0) as i16;
            samples.push(val);
        }
        k.process(&samples);
        // The signal stays within [-30, +30], which is inside the hysteresis
        // band of [-50, +49]. So no crossings should be detected because
        // the state never leaves Undecided.
        assert_eq!(k.crossings(), 0);
        assert_eq!(k.classify(), SignalType::Dc);
    }

    #[test]
    fn full_scale_sine_wave_ac() {
        let mut k = FullScaleKernel::new();
        // Generate a sine wave with amplitude 500 (well above hysteresis).
        // Each full cycle has 2 crossings.
        let num_cycles = 50;
        let samples_per_cycle = 100;
        let mut samples = Vec::new();
        for i in 0..(num_cycles * samples_per_cycle) {
            let phase = 2.0 * std::f64::consts::PI * i as f64 / samples_per_cycle as f64;
            let val = (phase.sin() * 500.0) as i16;
            samples.push(val);
        }
        k.process(&samples);

        // Expect approximately num_cycles * 2 crossings.
        // Due to discrete sampling the count may vary by a small amount.
        let crossings = k.crossings();
        assert!(
            (90..=110).contains(&crossings),
            "expected ~100 crossings, got {}",
            crossings
        );
        assert_eq!(k.classify(), SignalType::Ac);
    }

    #[test]
    fn full_scale_square_wave_ac() {
        let mut k = FullScaleKernel::new();
        // Square wave: alternating +500, -500. Each transition is a crossing.
        let half_period = 10;
        let num_cycles = 50;
        let mut samples = Vec::new();
        for cycle in 0..num_cycles {
            for _ in 0..half_period {
                samples.push(if cycle % 2 == 0 { 500i16 } else { -500 });
            }
        }
        k.process(&samples);

        // Each transition from +500 to -500 or vice versa should produce
        // crossings. The exact count depends on the state machine behavior.
        let crossings = k.crossings();
        assert!(
            (6..CROSSING_THRESHOLD_TOO_FAST).contains(&crossings),
            "expected AC-range crossings, got {}",
            crossings
        );
        assert_eq!(k.classify(), SignalType::Ac);
    }

    #[test]
    fn full_scale_too_fast() {
        let mut k = FullScaleKernel::new();
        // Very fast alternation: +100, -100 every sample.
        // Each pair generates a crossing when returning to undecided.
        let mut samples = Vec::new();
        for i in 0..10000 {
            samples.push(if i % 2 == 0 { 100i16 } else { -100 });
        }
        k.process(&samples);
        assert!(
            k.crossings() >= CROSSING_THRESHOLD_TOO_FAST,
            "expected >= {} crossings, got {}",
            CROSSING_THRESHOLD_TOO_FAST,
            k.crossings()
        );
        assert_eq!(k.classify(), SignalType::TooFast);
    }

    #[test]
    fn full_scale_hysteresis_prevents_noise_crossings() {
        let mut k = FullScaleKernel::new();
        // Signal oscillates between -10 and +10: within the hysteresis band.
        // Should produce zero crossings because the state never leaves Undecided.
        let mut samples = Vec::new();
        for i in 0..1000 {
            samples.push(if i % 2 == 0 { 10i16 } else { -10 });
        }
        k.process(&samples);
        assert_eq!(k.crossings(), 0);
        assert_eq!(k.classify(), SignalType::Dc);
    }

    #[test]
    fn full_scale_exact_threshold_values() {
        let mut k = FullScaleKernel::new();
        // +49 should NOT trigger Positive (need > +49).
        // -50 should NOT trigger Negative (need < -50).
        let samples = vec![49, -50, 49, -50, 49, -50];
        k.process(&samples);
        assert_eq!(
            k.crossings(),
            0,
            "boundary values should not trigger crossings"
        );

        // +50 should trigger Positive, -51 should trigger Negative.
        let mut k2 = FullScaleKernel::new();
        let samples2 = vec![50, -51, 50, -51];
        k2.process(&samples2);
        // 50 -> Positive, -51 -> crossing (back through zero? No, -51 is negative).
        // Actually: 50 -> Positive, -51 -> crossing (<=0) -> Undecided, then
        // -51 < -50 -> Negative, then 50 -> crossing (>=0) -> Undecided.
        // Wait, let me trace through:
        // sample 50: Undecided, 50 > 49 -> Positive
        // sample -51: Positive, -51 <= 0 -> crossing++, Undecided
        //             Undecided, -51 < -50 -> Negative
        // sample 50: Negative, 50 >= 0 -> crossing++, Undecided
        //            Undecided, 50 > 49 -> Positive
        // sample -51: Positive, -51 <= 0 -> crossing++, Undecided
        //             Undecided, -51 < -50 -> Negative
        // Total: 3 crossings
        // But wait -- the state machine transitions happen one sample at a time.
        // After a crossing, we go to Undecided and then check the SAME sample
        // again? No -- we process each sample once.
        // Let me re-trace:
        // state=Undecided
        //   sample=50: 50 > 49 -> state=Positive
        //   sample=-51: -51 <= 0 -> crossings=1, state=Undecided
        //   sample=50: 50 > 49 -> state=Positive
        //   sample=-51: -51 <= 0 -> crossings=2, state=Undecided
        // Total: 2 crossings (not 3, because going Undecided->Negative requires
        // < -50 check, but we already moved to next sample)
        assert_eq!(k2.crossings(), 2);
    }

    #[test]
    fn full_scale_empty_input() {
        let mut k = FullScaleKernel::new();
        k.process(&[]);
        assert_eq!(k.crossings(), 0);
        assert_eq!(k.total_samples(), 0);
        assert_eq!(k.classify(), SignalType::Dc);
    }

    #[test]
    fn full_scale_single_sample() {
        let mut k = FullScaleKernel::new();
        k.process(&[1000]);
        assert_eq!(k.crossings(), 0);
        assert_eq!(k.total_samples(), 1);
    }

    #[test]
    fn full_scale_streaming() {
        // Verify that streaming (multiple process calls) gives same result
        // as single call.
        let samples: Vec<i16> = (0..1000)
            .map(|i| {
                let phase = 2.0 * std::f64::consts::PI * i as f64 / 50.0;
                (phase.sin() * 500.0) as i16
            })
            .collect();

        let mut k_single = FullScaleKernel::new();
        k_single.process(&samples);

        let mut k_stream = FullScaleKernel::new();
        for chunk in samples.chunks(100) {
            k_stream.process(chunk);
        }

        assert_eq!(k_single.crossings(), k_stream.crossings());
        assert_eq!(k_single.total_samples(), k_stream.total_samples());
    }

    #[test]
    fn full_scale_reset() {
        let mut k = FullScaleKernel::new();
        let samples: Vec<i16> = vec![500, -500, 500, -500];
        k.process(&samples);
        assert!(k.crossings() > 0);

        k.reset();
        assert_eq!(k.crossings(), 0);
        assert_eq!(k.total_samples(), 0);
    }

    #[test]
    fn full_scale_frequency_estimate() {
        let mut k = FullScaleKernel::new();
        // 100 Hz sine wave at 10000 samples/sec for 1 second.
        let sample_rate: u64 = 10_000;
        let frequency = 100.0;
        let num_samples = sample_rate as usize;
        let mut samples = Vec::with_capacity(num_samples);
        for i in 0..num_samples {
            let phase = 2.0 * std::f64::consts::PI * frequency * i as f64 / sample_rate as f64;
            samples.push((phase.sin() * 500.0) as i16);
        }
        k.process(&samples);

        let est = k.estimate_frequency(sample_rate).unwrap();
        // Should be close to 100 Hz.
        assert!(
            (est - 100.0).abs() < 5.0,
            "estimated frequency {est} Hz, expected ~100 Hz"
        );
    }

    #[test]
    fn full_scale_frequency_dc_returns_none() {
        let mut k = FullScaleKernel::new();
        k.process(&[0; 1000]);
        assert!(k.estimate_frequency(10_000).is_none());
    }

    #[test]
    fn full_scale_frequency_no_samples_returns_none() {
        let k = FullScaleKernel::new();
        assert!(k.estimate_frequency(10_000).is_none());
    }

    #[test]
    fn full_scale_frequency_zero_rate_returns_none() {
        let mut k = FullScaleKernel::new();
        let samples: Vec<i16> = (0..100)
            .map(|i| if i % 2 == 0 { 500 } else { -500 })
            .collect();
        k.process(&samples);
        assert!(k.estimate_frequency(0).is_none());
    }

    // -----------------------------------------------------------------------
    // VerticalScaleKernel tests
    // -----------------------------------------------------------------------

    fn identity_transform() -> VoltageTransform {
        // 1:1 mapping: voltage = adc_code * 1.0 + 0.0
        VoltageTransform {
            scale: 1.0,
            offset: 0.0,
        }
    }

    fn pro8_transform() -> VoltageTransform {
        // Logic Pro 8 at +/-5V: maps [-2048, 2047] to [-5.0, +5.0]
        VoltageTransform::from_range(-5.0, 5.0, -2048, 2047)
    }

    #[test]
    fn vertical_new() {
        let k = VerticalScaleKernel::new(2, 12);
        assert_eq!(k.num_channels, 2);
        assert!(!k.has_data[0]);
        assert!(!k.has_data[1]);
    }

    #[test]
    fn vertical_process_tracks_min_max() {
        let mut k = VerticalScaleKernel::new(1, 12);
        k.process_channel(0, &[100, -200, 50, 300, -100]);
        let (min, max) = k.adc_range(0).unwrap();
        assert_eq!(min, -200);
        assert_eq!(max, 300);
    }

    #[test]
    fn vertical_process_streaming() {
        let mut k = VerticalScaleKernel::new(1, 12);
        k.process_channel(0, &[100, 200]);
        k.process_channel(0, &[-300, 50]);
        let (min, max) = k.adc_range(0).unwrap();
        assert_eq!(min, -300);
        assert_eq!(max, 200);
    }

    #[test]
    fn vertical_empty_samples() {
        let mut k = VerticalScaleKernel::new(1, 12);
        k.process_channel(0, &[]);
        assert!(k.adc_range(0).is_none());
        assert!(!k.has_data[0]);
    }

    #[test]
    fn vertical_saturation_at_min() {
        let mut k = VerticalScaleKernel::new(1, 12);
        k.process_channel(0, &[-2048, 0, 100]);
        assert!(k.is_saturated(0));
    }

    #[test]
    fn vertical_saturation_at_max() {
        let mut k = VerticalScaleKernel::new(1, 12);
        k.process_channel(0, &[0, 2047, 100]);
        assert!(k.is_saturated(0));
    }

    #[test]
    fn vertical_no_saturation() {
        let mut k = VerticalScaleKernel::new(1, 12);
        k.process_channel(0, &[-2047, 2046, 0]);
        assert!(!k.is_saturated(0));
    }

    #[test]
    fn vertical_saturation_no_data() {
        let k = VerticalScaleKernel::new(1, 12);
        assert!(!k.is_saturated(0));
    }

    #[test]
    fn vertical_saturation_8bit() {
        let mut k = VerticalScaleKernel::new(1, 8);
        k.process_channel(0, &[-128, 0]);
        assert!(k.is_saturated(0));
    }

    #[test]
    fn vertical_compute_result_measured() {
        let mut k = VerticalScaleKernel::new(1, 12);
        // ADC range [-500, 500] with identity transform.
        k.process_channel(0, &[-500, 0, 500]);

        let transform = identity_transform();
        let result = k.compute_result(0, &transform, -2048.0, 2047.0);

        match result {
            ChannelScaleResult::Measured {
                min_voltage,
                max_voltage,
                range_min,
                range_max,
            } => {
                assert_eq!(min_voltage, -500.0);
                assert_eq!(max_voltage, 500.0);
                // Signal range = 1000, margin = max(1000*0.2, 4095/128)
                // 1000*0.2 = 200, 4095/128 = 31.99
                // margin = 200
                assert!((range_min - (-500.0 - 200.0)).abs() < 0.01);
                assert!((range_max - (500.0 + 200.0)).abs() < 0.01);
            }
            other => panic!("expected Measured, got {:?}", other),
        }
    }

    #[test]
    fn vertical_compute_result_small_signal_uses_min_margin() {
        let mut k = VerticalScaleKernel::new(1, 12);
        // Very small signal: ADC range [0, 1]
        k.process_channel(0, &[0, 1]);

        let transform = identity_transform();
        // Full scale = [-2048, 2047], range = 4095
        let result = k.compute_result(0, &transform, -2048.0, 2047.0);

        match result {
            ChannelScaleResult::Measured {
                range_min,
                range_max,
                ..
            } => {
                // Signal range = 1, margin from signal = 1*0.2 = 0.2
                // Min margin from full scale = 4095 * (1/128) = 31.99
                // So margin = 31.99 (full-scale minimum dominates)
                let expected_margin = 4095.0 * MIN_MARGIN_FULL_SCALE_FRAC;
                assert!(
                    (range_min - (0.0 - expected_margin)).abs() < 0.1,
                    "range_min={}, expected={}",
                    range_min,
                    0.0 - expected_margin
                );
                assert!(
                    (range_max - (1.0 + expected_margin)).abs() < 0.1,
                    "range_max={}, expected={}",
                    range_max,
                    1.0 + expected_margin
                );
            }
            other => panic!("expected Measured, got {:?}", other),
        }
    }

    #[test]
    fn vertical_compute_result_saturated() {
        let mut k = VerticalScaleKernel::new(1, 12);
        k.process_channel(0, &[-2048, 2047]);

        let transform = identity_transform();
        let result = k.compute_result(0, &transform, -2048.0, 2047.0);
        assert_eq!(result, ChannelScaleResult::Saturated);
    }

    #[test]
    fn vertical_compute_result_no_data() {
        let k = VerticalScaleKernel::new(1, 12);
        let transform = identity_transform();
        let result = k.compute_result(0, &transform, -2048.0, 2047.0);
        assert_eq!(result, ChannelScaleResult::NeedsMeasurement);
    }

    #[test]
    fn vertical_compute_with_pro8_transform() {
        let mut k = VerticalScaleKernel::new(1, 12);
        // Signal at roughly +/-1V on a +/-5V range.
        // +/-1V maps to roughly +/-409 ADC codes (1V / 5V * 2048)
        k.process_channel(0, &[-409, 409]);

        let transform = pro8_transform();
        let result = k.compute_result(0, &transform, -5.0, 5.0);

        match result {
            ChannelScaleResult::Measured {
                min_voltage,
                max_voltage,
                range_min,
                range_max,
            } => {
                // Should be approximately +/-1V
                assert!(
                    (min_voltage - (-1.0)).abs() < 0.05,
                    "min_voltage={}, expected ~-1.0",
                    min_voltage
                );
                assert!(
                    (max_voltage - 1.0).abs() < 0.05,
                    "max_voltage={}, expected ~1.0",
                    max_voltage
                );
                // Margin: signal_range ~2V, 2*0.2=0.4V
                // Full scale margin: 10V / 128 = 0.078V
                // So 0.4V margin is used.
                assert!(range_min < min_voltage);
                assert!(range_max > max_voltage);
            }
            other => panic!("expected Measured, got {:?}", other),
        }
    }

    #[test]
    fn vertical_reset() {
        let mut k = VerticalScaleKernel::new(2, 12);
        k.process_channel(0, &[100, 200]);
        k.process_channel(1, &[-100, -200]);
        assert!(k.has_data[0]);
        assert!(k.has_data[1]);

        k.reset();
        assert!(!k.has_data[0]);
        assert!(!k.has_data[1]);
        assert!(k.adc_range(0).is_none());
        assert!(k.adc_range(1).is_none());
    }

    #[test]
    fn vertical_multi_channel() {
        let mut k = VerticalScaleKernel::new(4, 12);
        k.process_channel(0, &[100, -100]);
        k.process_channel(1, &[200, -200]);
        k.process_channel(2, &[300, -300]);
        k.process_channel(3, &[400, -400]);

        assert_eq!(k.adc_range(0), Some((-100, 100)));
        assert_eq!(k.adc_range(1), Some((-200, 200)));
        assert_eq!(k.adc_range(2), Some((-300, 300)));
        assert_eq!(k.adc_range(3), Some((-400, 400)));
    }

    // -----------------------------------------------------------------------
    // AutoScaler tests
    // -----------------------------------------------------------------------

    #[test]
    fn autoscaler_dc_signal() {
        let mut scaler = AutoScaler::new(1, 12);

        // Phase 1: DC signal (constant).
        scaler.process_full_scale(0, &[10; 1000]);
        scaler.finish_full_scale();
        assert_eq!(scaler.channel_state(0), ChannelState::WaitingVertical);

        // Phase 2: measure the signal.
        scaler.process_vertical(0, &[10; 1000]);

        let transform = identity_transform();
        let results = scaler.compute_results(&[transform], &[(-2048.0, 2047.0)]);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].state, ChannelState::Done);
    }

    #[test]
    fn autoscaler_too_fast_signal() {
        let mut scaler = AutoScaler::new(1, 12);

        // Phase 1: very fast alternation.
        let samples: Vec<i16> = (0..10000)
            .map(|i| if i % 2 == 0 { 100 } else { -100 })
            .collect();
        scaler.process_full_scale(0, &samples);
        scaler.finish_full_scale();
        assert_eq!(scaler.channel_state(0), ChannelState::TooFast);
    }

    #[test]
    fn autoscaler_saturated_then_retry() {
        let mut scaler = AutoScaler::new(1, 12);

        // Phase 1: AC signal.
        let samples: Vec<i16> = (0..5000)
            .map(|i| {
                let phase = 2.0 * std::f64::consts::PI * i as f64 / 100.0;
                (phase.sin() * 500.0) as i16
            })
            .collect();
        scaler.process_full_scale(0, &samples);
        scaler.finish_full_scale();
        assert_eq!(scaler.channel_state(0), ChannelState::WaitingVertical);

        // Phase 2: saturated signal.
        scaler.process_vertical(0, &[-2048, 2047]);
        let transform = identity_transform();
        let results = scaler.compute_results(&[transform], &[(-2048.0, 2047.0)]);
        // Should still be WaitingVertical (retry).
        assert_eq!(results[0].state, ChannelState::WaitingVertical);
        assert!(!scaler.max_iterations_reached());

        // Reset vertical and retry.
        scaler.reset_vertical();
        scaler.process_vertical(0, &[-500, 500]);
        let results = scaler.compute_results(&[transform], &[(-2048.0, 2047.0)]);
        assert_eq!(results[0].state, ChannelState::Done);
    }

    #[test]
    fn autoscaler_saturated_exceeds_max_iterations() {
        let mut scaler = AutoScaler::new(1, 12);

        // Phase 1.
        let samples: Vec<i16> = (0..5000)
            .map(|i| {
                let phase = 2.0 * std::f64::consts::PI * i as f64 / 100.0;
                (phase.sin() * 500.0) as i16
            })
            .collect();
        scaler.process_full_scale(0, &samples);
        scaler.finish_full_scale();

        let transform = identity_transform();

        // Keep saturating until max iterations.
        for _ in 0..MAX_ITERATIONS {
            scaler.process_vertical(0, &[-2048, 2047]);
            let results = scaler.compute_results(&[transform], &[(-2048.0, 2047.0)]);
            if results[0].state == ChannelState::SaturatedError {
                return; // Test passes.
            }
            scaler.reset_vertical();
        }

        // After MAX_ITERATIONS resets, should be saturated error.
        scaler.process_vertical(0, &[-2048, 2047]);
        let results = scaler.compute_results(&[transform], &[(-2048.0, 2047.0)]);
        assert_eq!(results[0].state, ChannelState::SaturatedError);
    }

    #[test]
    fn autoscaler_multi_channel() {
        let mut scaler = AutoScaler::new(2, 12);

        // Phase 1: ch0 = DC, ch1 = AC.
        scaler.process_full_scale(0, &[10; 1000]);
        let ac_samples: Vec<i16> = (0..5000)
            .map(|i| {
                let phase = 2.0 * std::f64::consts::PI * i as f64 / 100.0;
                (phase.sin() * 500.0) as i16
            })
            .collect();
        scaler.process_full_scale(1, &ac_samples);
        scaler.finish_full_scale();
        assert_eq!(scaler.channel_state(0), ChannelState::WaitingVertical);
        assert_eq!(scaler.channel_state(1), ChannelState::WaitingVertical);

        // Phase 2.
        scaler.process_vertical(0, &[10; 100]);
        scaler.process_vertical(1, &[-500, 500]);

        let transform = identity_transform();
        let results = scaler.compute_results(
            &[transform, transform],
            &[(-2048.0, 2047.0), (-2048.0, 2047.0)],
        );
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].state, ChannelState::Done);
        assert_eq!(results[1].state, ChannelState::Done);
    }

    // -----------------------------------------------------------------------
    // DC path helper tests
    // -----------------------------------------------------------------------

    #[test]
    fn dc_range_small_signal() {
        // Signal at 0V to 0.1V.
        let (min, max) = compute_dc_range(0.0, 0.1);
        // signal_range = 0.1, margin = max(0.02, 1.0) = 1.0
        // asymmetric = 0.2 * 1.0 = 0.2
        assert!((min - (-0.2)).abs() < 0.001);
        assert!((max - 0.3).abs() < 0.001);
    }

    #[test]
    fn dc_range_large_signal() {
        // Signal at -3V to +3V.
        let (min, max) = compute_dc_range(-3.0, 3.0);
        // signal_range = 6.0, margin = max(1.2, 1.0) = 1.2
        // asymmetric = 0.2 * 1.2 = 0.24
        assert!((min - (-3.24)).abs() < 0.001);
        assert!((max - 3.24).abs() < 0.001);
    }

    #[test]
    fn dc_range_zero_signal() {
        // DC at exactly 0V.
        let (min, max) = compute_dc_range(0.0, 0.0);
        // signal_range = 0, margin = max(0, 1.0) = 1.0
        // asymmetric = 0.2 * 1.0 = 0.2
        assert!((min - (-0.2)).abs() < 0.001);
        assert!((max - 0.2).abs() < 0.001);
    }

    // -----------------------------------------------------------------------
    // Saturation threshold tests
    // -----------------------------------------------------------------------

    #[test]
    fn saturation_thresholds_12bit() {
        let (min, max) = saturation_thresholds(12);
        assert_eq!(min, -2048);
        assert_eq!(max, 2047);
    }

    #[test]
    fn saturation_thresholds_8bit() {
        let (min, max) = saturation_thresholds(8);
        assert_eq!(min, -128);
        assert_eq!(max, 127);
    }

    #[test]
    fn saturation_thresholds_10bit() {
        let (min, max) = saturation_thresholds(10);
        assert_eq!(min, -512);
        assert_eq!(max, 511);
    }

    // -----------------------------------------------------------------------
    // Edge cases
    // -----------------------------------------------------------------------

    #[test]
    fn vertical_single_sample() {
        let mut k = VerticalScaleKernel::new(1, 12);
        k.process_channel(0, &[42]);
        let (min, max) = k.adc_range(0).unwrap();
        assert_eq!(min, 42);
        assert_eq!(max, 42);
    }

    #[test]
    fn vertical_all_zeros() {
        let mut k = VerticalScaleKernel::new(1, 12);
        k.process_channel(0, &[0; 1000]);
        let (min, max) = k.adc_range(0).unwrap();
        assert_eq!(min, 0);
        assert_eq!(max, 0);
        assert!(!k.is_saturated(0));
    }

    #[test]
    fn full_scale_all_zeros() {
        let mut k = FullScaleKernel::new();
        k.process(&[0; 1000]);
        assert_eq!(k.crossings(), 0);
        assert_eq!(k.classify(), SignalType::Dc);
    }

    #[test]
    fn autoscaler_zero_channels_is_valid() {
        // Edge case: zero channels. The AutoScaler should accept this.
        let scaler = AutoScaler::new(0, 12);
        assert_eq!(scaler.num_channels(), 0);
    }

    #[test]
    #[should_panic(expected = "exceeds MAX_SCOPE_CHANNELS")]
    fn autoscaler_too_many_channels_panics() {
        let _ = AutoScaler::new(5, 12);
    }

    #[test]
    #[should_panic(expected = "exceeds MAX_SCOPE_CHANNELS")]
    fn vertical_too_many_channels_panics() {
        let _ = VerticalScaleKernel::new(5, 12);
    }

    // -----------------------------------------------------------------------
    // SignalType / ChannelState derive coverage
    // -----------------------------------------------------------------------

    #[test]
    fn signal_type_eq() {
        assert_eq!(SignalType::Dc, SignalType::Dc);
        assert_ne!(SignalType::Dc, SignalType::Ac);
        assert_ne!(SignalType::Ac, SignalType::TooFast);
    }

    #[test]
    fn channel_state_eq() {
        assert_eq!(ChannelState::Done, ChannelState::Done);
        assert_ne!(ChannelState::Done, ChannelState::TooFast);
    }

    #[test]
    fn channel_scale_result_eq() {
        assert_eq!(
            ChannelScaleResult::NeedsMeasurement,
            ChannelScaleResult::NeedsMeasurement
        );
        assert_eq!(ChannelScaleResult::Saturated, ChannelScaleResult::Saturated);
        assert_ne!(
            ChannelScaleResult::NeedsMeasurement,
            ChannelScaleResult::Saturated
        );
    }
}
