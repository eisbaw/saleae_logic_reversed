//! Software trigger engine for Saleae Logic devices.
//!
//! All Saleae Logic devices (Pro 8, Pro 16, Logic 8, Logic 4) use software-only
//! triggers -- the FPGA streams all samples to the host, and trigger detection
//! runs entirely in the graph server on the host side. Only MSO devices have
//! partial FPGA-based analog trigger support (out of scope here).
//!
//! This module implements the digital trigger detection algorithms reconstructed
//! from `DigitalTriggerNode` in `libgraph_server_shared.so`:
//!
//! - **Edge triggers**: detect rising/falling/any-edge transitions on digital
//!   channels, with multi-channel AND logic (all channels must match on the
//!   same sample).
//! - **Pulse triggers**: measure pulse width (edge-to-edge duration) and trigger
//!   when the width falls within a configured [min, max] sample range.
//!
//! # References
//!
//! - `re/notes/trigger-engine.md` -- full RE analysis
//! - `EdgeTrigger_Detect` (ELF 0x01be1960) -- edge detection algorithm
//! - `PulseTrigger_Detect` (ELF 0x01be23c0) -- pulse detection state machine
//! - `ConfigReset` (ELF 0x01be12f0) -- trigger state initialization

/// Edge direction for edge trigger detection.
///
/// Matches the binary's encoding at `settings + 0x04`:
/// - 0 = Rising (match when digital value transitions to 1)
/// - 1 = Falling (match when digital value transitions to 0)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdgeType {
    /// Trigger on low-to-high transition.
    Rising,
    /// Trigger on high-to-low transition.
    Falling,
    /// Trigger on any transition (rising or falling).
    AnyEdge,
}

/// Pulse polarity for pulse trigger detection.
///
/// Determines whether we measure the duration of a high pulse or a low pulse.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PulsePolarity {
    /// Measure high pulses: rising edge starts, falling edge ends.
    ActiveHigh,
    /// Measure low pulses: falling edge starts, rising edge ends.
    ActiveLow,
}

/// A single trigger condition.
///
/// The trigger engine supports two condition types, matching the binary's
/// `CheckTrigger` dispatch (type 0 = edge, type 1 = pulse).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TriggerCondition {
    /// Trigger on a digital edge transition.
    Edge {
        /// Channel index to monitor.
        channel: u8,
        /// Which edge direction to match.
        edge_type: EdgeType,
    },
    /// Trigger on a pulse with duration within [min_samples, max_samples].
    ///
    /// The pulse width thresholds are computed from duration range and sample
    /// rate in `ConfigReset`: `min_samples = floor(duration_min * sample_rate)`,
    /// `max_samples = ceil(duration_max * sample_rate)`.
    Pulse {
        /// Channel index to monitor.
        channel: u8,
        /// Whether to measure high or low pulses.
        polarity: PulsePolarity,
        /// Minimum pulse width in samples (inclusive).
        min_samples: u64,
        /// Maximum pulse width in samples (inclusive).
        max_samples: u64,
    },
}

/// Configuration for the trigger engine.
///
/// Multiple conditions use AND logic -- all must be satisfied simultaneously
/// for the trigger to fire. This matches the binary's multi-channel pattern
/// matching where `FUN_01be3a10` checks all channels in the pattern.
#[derive(Clone, Debug)]
pub struct TriggerConfig {
    /// All conditions that must match (AND logic).
    pub conditions: Vec<TriggerCondition>,
    /// Number of samples to retain before the trigger point.
    pub pre_trigger_samples: u64,
}

/// Detects edge transitions on digital channels.
///
/// Tracks the previous value per channel to detect transitions. For
/// multi-channel edge conditions, all must fire on the same sample
/// (per the binary's pattern: build vector of channel states, scan
/// samples, check all channels at each transition).
#[derive(Clone, Debug)]
pub struct EdgeDetector {
    /// Previous value per channel (None = no previous sample seen).
    prev_values: Vec<Option<bool>>,
    /// Edge conditions to check.
    conditions: Vec<(u8, EdgeType)>,
}

impl EdgeDetector {
    /// Create a new edge detector for the given conditions.
    ///
    /// `max_channels` determines the size of the per-channel state array.
    pub fn new(conditions: Vec<(u8, EdgeType)>, max_channels: usize) -> Self {
        Self {
            prev_values: vec![None; max_channels],
            conditions,
        }
    }

    /// Check whether a single channel's current value matches an edge condition.
    ///
    /// Returns `true` if a matching edge transition occurred (compared to the
    /// previously seen value for this channel). The previous value is NOT updated
    /// by this call -- use `update` to advance state after checking all conditions.
    pub fn check(&self, channel: u8, current_value: bool, edge_type: EdgeType) -> bool {
        let ch = channel as usize;
        if ch >= self.prev_values.len() {
            return false;
        }
        let prev = match self.prev_values[ch] {
            Some(v) => v,
            None => return false, // No previous sample, cannot detect edge.
        };
        if prev == current_value {
            return false; // No transition.
        }
        match edge_type {
            // Rising: match when transitioning to 1 (prev=0, current=1).
            EdgeType::Rising => !prev && current_value,
            // Falling: match when transitioning to 0 (prev=1, current=0).
            EdgeType::Falling => prev && !current_value,
            // AnyEdge: any transition.
            EdgeType::AnyEdge => true,
        }
    }

    /// Check whether all edge conditions are satisfied by the given channel values.
    ///
    /// All conditions must match simultaneously (AND logic). Returns `true` only
    /// if every configured edge condition fires on this sample.
    pub fn check_all(&self, channel_values: &[bool]) -> bool {
        if self.conditions.is_empty() {
            return true; // No edge conditions = vacuously satisfied.
        }
        for &(ch, edge_type) in &self.conditions {
            let ch_idx = ch as usize;
            if ch_idx >= channel_values.len() {
                return false;
            }
            if !self.check(ch, channel_values[ch_idx], edge_type) {
                return false;
            }
        }
        true
    }

    /// Update the tracked previous values for all channels.
    ///
    /// Call this after processing each sample to advance state.
    pub fn update(&mut self, channel_values: &[bool]) {
        for (i, &val) in channel_values.iter().enumerate() {
            if i < self.prev_values.len() {
                self.prev_values[i] = Some(val);
            }
        }
    }

    /// Reset all tracked state.
    pub fn reset(&mut self) {
        for v in &mut self.prev_values {
            *v = None;
        }
    }
}

/// Internal state for the pulse detection state machine.
///
/// Mirrors the pulse trigger's state tracking in `PulseTrigger_Detect`:
/// tracks the start of the active pulse region and measures duration
/// until the opposite edge.
#[derive(Clone, Debug, PartialEq, Eq)]
enum PulseState {
    /// Waiting for the pulse to start (waiting for the leading edge).
    WaitingForStart,
    /// Pulse is active, measuring duration from `start_sample`.
    Measuring { start_sample: u64 },
    /// Trigger has fired. Terminal state until reset.
    Triggered,
}

/// Detects pulses with duration within a configured range.
///
/// State machine:
/// 1. `WaitingForStart`: watch for the leading edge (rising for ActiveHigh,
///    falling for ActiveLow).
/// 2. `Measuring`: pulse is active. Watch for the trailing edge. When it
///    arrives, compute width = current_sample - start_sample.
///    - If width is in [min_samples, max_samples]: trigger fires (at the end
///      of the pulse, matching the binary's behavior).
///    - If width is outside range: reject, go back to WaitingForStart.
/// 3. `Triggered`: terminal state, no further detection until reset.
#[derive(Clone, Debug)]
pub struct PulseDetector {
    /// Channel to monitor.
    channel: u8,
    /// Pulse polarity.
    polarity: PulsePolarity,
    /// Minimum pulse width in samples (inclusive).
    min_samples: u64,
    /// Maximum pulse width in samples (inclusive).
    max_samples: u64,
    /// Current state machine state.
    state: PulseState,
    /// Previous value on the monitored channel.
    prev_value: Option<bool>,
}

impl PulseDetector {
    /// Create a new pulse detector.
    pub fn new(channel: u8, polarity: PulsePolarity, min_samples: u64, max_samples: u64) -> Self {
        Self {
            channel,
            polarity,
            min_samples,
            max_samples,
            state: PulseState::WaitingForStart,
            prev_value: None,
        }
    }

    /// Process one sample. Returns `true` if the trigger fires on this sample.
    ///
    /// The trigger fires at the trailing edge of a qualifying pulse (matching
    /// the binary where the trigger timestamp is recorded at the end of the
    /// measured pulse).
    pub fn process(&mut self, sample_index: u64, current_value: bool) -> bool {
        if self.state == PulseState::Triggered {
            return false;
        }

        let prev = self.prev_value;
        self.prev_value = Some(current_value);

        let prev = match prev {
            Some(v) => v,
            None => return false,
        };

        if prev == current_value {
            // No transition -- but check if we're measuring and exceeded max.
            if let PulseState::Measuring { start_sample } = self.state {
                let width = sample_index - start_sample;
                if width > self.max_samples {
                    // Pulse too long, will never qualify. Reset to waiting.
                    self.state = PulseState::WaitingForStart;
                }
            }
            return false;
        }

        // A transition occurred.
        let is_rising = !prev && current_value;
        let is_leading_edge = match self.polarity {
            PulsePolarity::ActiveHigh => is_rising,
            PulsePolarity::ActiveLow => !is_rising,
        };

        match &self.state {
            PulseState::WaitingForStart => {
                if is_leading_edge {
                    self.state = PulseState::Measuring {
                        start_sample: sample_index,
                    };
                }
                false
            }
            PulseState::Measuring { start_sample } => {
                if !is_leading_edge {
                    // Trailing edge: measure the pulse.
                    let width = sample_index - start_sample;
                    if width >= self.min_samples && width <= self.max_samples {
                        self.state = PulseState::Triggered;
                        return true;
                    }
                    // Width out of range. Go back to waiting.
                    // Note: this edge could also be the start of a new pulse
                    // of the opposite polarity, but per the binary, we just
                    // return to WaitingForStart.
                    self.state = PulseState::WaitingForStart;
                } else {
                    // Another leading edge while measuring -- shouldn't happen
                    // for a single digital channel (would require same-edge
                    // twice without opposite edge). Reset measurement.
                    self.state = PulseState::Measuring {
                        start_sample: sample_index,
                    };
                }
                false
            }
            PulseState::Triggered => false,
        }
    }

    /// Whether the trigger has fired.
    pub fn is_triggered(&self) -> bool {
        self.state == PulseState::Triggered
    }

    /// Reset the detector to initial state for re-arm.
    pub fn reset(&mut self) {
        self.state = PulseState::WaitingForStart;
        self.prev_value = None;
    }
}

/// Top-level trigger engine combining edge and pulse conditions.
///
/// Processes sample-by-sample digital data and fires when all conditions
/// are simultaneously satisfied. The trigger is one-shot: once fired, it
/// remains in the triggered state until explicitly reset (matching the
/// binary's `triggered` flag at offset 0x180).
///
/// For AND logic across edge + pulse conditions:
/// - All edge conditions must fire on the current sample.
/// - All pulse conditions must have already triggered (pulse triggers fire
///   at the trailing edge, which may precede the current sample).
///
/// Note: mixed edge+pulse AND requires careful semantics. The binary's
/// multi-channel pattern check (`FUN_01be3a10`) validates all channels
/// simultaneously. For our implementation, we treat it as: the trigger
/// fires on the first sample where all edge conditions match AND all
/// pulse detectors are in the Triggered state.
pub struct TriggerEngine {
    config: TriggerConfig,
    edge_detector: EdgeDetector,
    pulse_detectors: Vec<PulseDetector>,
    triggered: bool,
    trigger_sample: Option<u64>,
}

impl TriggerEngine {
    /// Create a new trigger engine from the given configuration.
    pub fn new(config: TriggerConfig) -> Self {
        // Separate conditions into edge and pulse groups.
        let mut edge_conditions = Vec::new();
        let mut pulse_detectors = Vec::new();
        let mut max_channel: u8 = 0;

        for cond in &config.conditions {
            match cond {
                TriggerCondition::Edge { channel, edge_type } => {
                    edge_conditions.push((*channel, *edge_type));
                    max_channel = max_channel.max(*channel);
                }
                TriggerCondition::Pulse {
                    channel,
                    polarity,
                    min_samples,
                    max_samples,
                } => {
                    pulse_detectors.push(PulseDetector::new(
                        *channel,
                        *polarity,
                        *min_samples,
                        *max_samples,
                    ));
                    max_channel = max_channel.max(*channel);
                }
            }
        }

        let max_channels = max_channel as usize + 1;
        let edge_detector = EdgeDetector::new(edge_conditions, max_channels);

        Self {
            config,
            edge_detector,
            pulse_detectors,
            triggered: false,
            trigger_sample: None,
        }
    }

    /// Process a single sample across all channels.
    ///
    /// Returns `Some(trigger_sample_index)` when the trigger fires, `None`
    /// otherwise. The returned sample index accounts for `pre_trigger_samples`:
    /// `trigger_sample_index = sample_index - pre_trigger_samples` (saturating
    /// at 0).
    ///
    /// Once triggered, subsequent calls return `None` until `reset()`.
    pub fn process_sample(
        &mut self,
        sample_index: u64,
        channel_values: &[bool],
    ) -> Option<u64> {
        if self.triggered {
            return None;
        }

        // Feed pulse detectors first (they need every sample for state tracking).
        for pd in &mut self.pulse_detectors {
            let ch = pd.channel as usize;
            if ch < channel_values.len() {
                pd.process(sample_index, channel_values[ch]);
            }
        }

        // Check edge conditions.
        let edges_match = self.edge_detector.check_all(channel_values);

        // Update edge detector state AFTER checking (so check uses prev vs current).
        self.edge_detector.update(channel_values);

        // Check pulse conditions: all must be in Triggered state.
        let pulses_match = self.pulse_detectors.iter().all(|pd| pd.is_triggered());

        // All conditions must be satisfied.
        if edges_match && pulses_match {
            self.triggered = true;
            let trigger_at = sample_index.saturating_sub(self.config.pre_trigger_samples);
            self.trigger_sample = Some(trigger_at);
            return Some(trigger_at);
        }

        None
    }

    /// Whether the trigger has fired.
    pub fn is_triggered(&self) -> bool {
        self.triggered
    }

    /// The sample index where the trigger fired (if any).
    pub fn trigger_sample(&self) -> Option<u64> {
        self.trigger_sample
    }

    /// The configured pre-trigger sample count.
    pub fn pre_trigger_samples(&self) -> u64 {
        self.config.pre_trigger_samples
    }

    /// Reset the engine for re-arm. Clears all internal state.
    pub fn reset(&mut self) {
        self.triggered = false;
        self.trigger_sample = None;
        self.edge_detector.reset();
        for pd in &mut self.pulse_detectors {
            pd.reset();
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------
    // EdgeDetector unit tests
    // -------------------------------------------------------------------

    #[test]
    fn edge_detector_rising_edge() {
        let mut det = EdgeDetector::new(vec![(0, EdgeType::Rising)], 1);

        // First sample: no previous value, cannot detect edge.
        det.update(&[false]);
        assert!(!det.check_all(&[false])); // no transition
        det.update(&[false]);

        // Rising edge: false -> true
        assert!(det.check_all(&[true]));
        det.update(&[true]);

        // No transition: true -> true
        assert!(!det.check_all(&[true]));
        det.update(&[true]);

        // Falling edge: true -> false (should NOT match rising)
        assert!(!det.check_all(&[false]));
    }

    #[test]
    fn edge_detector_falling_edge() {
        let mut det = EdgeDetector::new(vec![(0, EdgeType::Falling)], 1);

        det.update(&[true]);
        // Falling: true -> false
        assert!(det.check_all(&[false]));
        det.update(&[false]);

        // Rising: false -> true (should NOT match falling)
        assert!(!det.check_all(&[true]));
    }

    #[test]
    fn edge_detector_any_edge() {
        let mut det = EdgeDetector::new(vec![(0, EdgeType::AnyEdge)], 1);

        det.update(&[false]);
        // Rising
        assert!(det.check_all(&[true]));
        det.update(&[true]);
        // Falling
        assert!(det.check_all(&[false]));
        det.update(&[false]);
        // No change
        assert!(!det.check_all(&[false]));
    }

    #[test]
    fn edge_detector_multi_channel_and() {
        // ch0 rising AND ch1 falling must fire on the same sample.
        let mut det = EdgeDetector::new(
            vec![(0, EdgeType::Rising), (1, EdgeType::Falling)],
            2,
        );

        // Initial state.
        det.update(&[false, true]);

        // ch0 rises, ch1 falls simultaneously.
        assert!(det.check_all(&[true, false]));
        det.update(&[true, false]);

        // Only ch0 rises, ch1 stays low -- ch1 has no falling edge.
        det.update(&[false, false]);
        assert!(!det.check_all(&[true, false]));
    }

    #[test]
    fn edge_detector_multi_channel_partial_mismatch() {
        // ch0 rising AND ch1 falling
        let mut det = EdgeDetector::new(
            vec![(0, EdgeType::Rising), (1, EdgeType::Falling)],
            2,
        );

        det.update(&[false, true]);

        // Only ch0 rises, ch1 stays high -> not all match.
        assert!(!det.check_all(&[true, true]));
    }

    // -------------------------------------------------------------------
    // PulseDetector unit tests
    // -------------------------------------------------------------------

    #[test]
    fn pulse_detector_active_high_in_range() {
        let mut pd = PulseDetector::new(0, PulsePolarity::ActiveHigh, 3, 5);

        // Feed: low for a while, then high pulse of width 4, then low.
        assert!(!pd.process(0, false));
        assert!(!pd.process(1, false));
        // Rising edge at sample 2 -> start measuring.
        assert!(!pd.process(2, true));
        assert!(!pd.process(3, true));
        assert!(!pd.process(4, true));
        assert!(!pd.process(5, true));
        // Falling edge at sample 6 -> width = 6 - 2 = 4 (in [3,5]).
        assert!(pd.process(6, false));
        assert!(pd.is_triggered());
    }

    #[test]
    fn pulse_detector_active_high_too_short() {
        let mut pd = PulseDetector::new(0, PulsePolarity::ActiveHigh, 5, 10);

        assert!(!pd.process(0, false));
        // Rising at 1.
        assert!(!pd.process(1, true));
        assert!(!pd.process(2, true));
        // Falling at 3 -> width = 3 - 1 = 2 (< min 5).
        assert!(!pd.process(3, false));
        assert!(!pd.is_triggered());
    }

    #[test]
    fn pulse_detector_active_high_too_long() {
        let mut pd = PulseDetector::new(0, PulsePolarity::ActiveHigh, 2, 4);

        assert!(!pd.process(0, false));
        // Rising at 1.
        assert!(!pd.process(1, true));
        assert!(!pd.process(2, true));
        assert!(!pd.process(3, true));
        assert!(!pd.process(4, true));
        assert!(!pd.process(5, true));
        // Falling at 6 -> width = 6 - 1 = 5 (> max 4).
        assert!(!pd.process(6, false));
        assert!(!pd.is_triggered());
    }

    #[test]
    fn pulse_detector_active_low() {
        let mut pd = PulseDetector::new(0, PulsePolarity::ActiveLow, 2, 4);

        assert!(!pd.process(0, true));
        // Falling at 1 -> start measuring low pulse.
        assert!(!pd.process(1, false));
        assert!(!pd.process(2, false));
        assert!(!pd.process(3, false));
        // Rising at 4 -> width = 4 - 1 = 3 (in [2,4]).
        assert!(pd.process(4, true));
        assert!(pd.is_triggered());
    }

    #[test]
    fn pulse_detector_rejects_then_accepts() {
        let mut pd = PulseDetector::new(0, PulsePolarity::ActiveHigh, 3, 5);

        assert!(!pd.process(0, false));
        // First pulse: too short (width = 1).
        assert!(!pd.process(1, true));
        assert!(!pd.process(2, false));
        assert!(!pd.is_triggered());

        // Second pulse: width = 4 (in range).
        assert!(!pd.process(3, true));
        assert!(!pd.process(4, true));
        assert!(!pd.process(5, true));
        assert!(!pd.process(6, true));
        assert!(pd.process(7, false)); // width = 7 - 3 = 4
        assert!(pd.is_triggered());
    }

    #[test]
    fn pulse_detector_reset() {
        let mut pd = PulseDetector::new(0, PulsePolarity::ActiveHigh, 2, 5);

        assert!(!pd.process(0, false));
        assert!(!pd.process(1, true));
        assert!(!pd.process(2, true));
        assert!(!pd.process(3, true));
        assert!(pd.process(4, false)); // width = 3, triggers.
        assert!(pd.is_triggered());

        pd.reset();
        assert!(!pd.is_triggered());

        // Should be able to trigger again after reset.
        assert!(!pd.process(10, false));
        assert!(!pd.process(11, true));
        assert!(!pd.process(12, true));
        assert!(!pd.process(13, true));
        assert!(pd.process(14, false));
        assert!(pd.is_triggered());
    }

    // -------------------------------------------------------------------
    // TriggerEngine tests
    // -------------------------------------------------------------------

    #[test]
    fn engine_edge_rising_ch0() {
        let config = TriggerConfig {
            conditions: vec![TriggerCondition::Edge {
                channel: 0,
                edge_type: EdgeType::Rising,
            }],
            pre_trigger_samples: 0,
        };
        let mut engine = TriggerEngine::new(config);

        assert_eq!(engine.process_sample(0, &[false]), None);
        assert_eq!(engine.process_sample(1, &[false]), None);
        // Rising edge at sample 2.
        assert_eq!(engine.process_sample(2, &[true]), Some(2));
        assert!(engine.is_triggered());

        // One-shot: no more triggers.
        assert_eq!(engine.process_sample(3, &[false]), None);
        assert_eq!(engine.process_sample(4, &[true]), None);
    }

    #[test]
    fn engine_edge_multi_channel_and() {
        // ch0 rising AND ch1 falling on the same sample.
        let config = TriggerConfig {
            conditions: vec![
                TriggerCondition::Edge {
                    channel: 0,
                    edge_type: EdgeType::Rising,
                },
                TriggerCondition::Edge {
                    channel: 1,
                    edge_type: EdgeType::Falling,
                },
            ],
            pre_trigger_samples: 0,
        };
        let mut engine = TriggerEngine::new(config);

        // Initial values.
        assert_eq!(engine.process_sample(0, &[false, true]), None);

        // Only ch0 rises, ch1 stays high -> no trigger.
        assert_eq!(engine.process_sample(1, &[true, true]), None);

        // Reset ch0 to low (need another rising edge later).
        assert_eq!(engine.process_sample(2, &[false, true]), None);

        // Both conditions met: ch0 rises (false->true), ch1 falls (true->false).
        assert_eq!(engine.process_sample(3, &[true, false]), Some(3));
        assert!(engine.is_triggered());
    }

    #[test]
    fn engine_pulse_in_range() {
        let config = TriggerConfig {
            conditions: vec![TriggerCondition::Pulse {
                channel: 0,
                polarity: PulsePolarity::ActiveHigh,
                min_samples: 3,
                max_samples: 5,
            }],
            pre_trigger_samples: 0,
        };
        let mut engine = TriggerEngine::new(config);

        // No edge conditions -> edges_match is vacuously true.
        // Feed a qualifying pulse.
        assert_eq!(engine.process_sample(0, &[false]), None);
        assert_eq!(engine.process_sample(1, &[true]), None); // rising edge
        assert_eq!(engine.process_sample(2, &[true]), None);
        assert_eq!(engine.process_sample(3, &[true]), None);
        assert_eq!(engine.process_sample(4, &[true]), None);
        // Falling edge at 5 -> width = 5 - 1 = 4 (in [3,5]).
        // Pulse detector triggers. Edge detector has no conditions (vacuous).
        assert_eq!(engine.process_sample(5, &[false]), Some(5));
        assert!(engine.is_triggered());
    }

    #[test]
    fn engine_pulse_reject_too_short_and_too_long() {
        let config = TriggerConfig {
            conditions: vec![TriggerCondition::Pulse {
                channel: 0,
                polarity: PulsePolarity::ActiveHigh,
                min_samples: 3,
                max_samples: 5,
            }],
            pre_trigger_samples: 0,
        };
        let mut engine = TriggerEngine::new(config);

        // Pulse too short (width = 1).
        assert_eq!(engine.process_sample(0, &[false]), None);
        assert_eq!(engine.process_sample(1, &[true]), None);
        assert_eq!(engine.process_sample(2, &[false]), None);
        assert!(!engine.is_triggered());

        // Pulse too long (width = 7).
        assert_eq!(engine.process_sample(3, &[true]), None);
        assert_eq!(engine.process_sample(4, &[true]), None);
        assert_eq!(engine.process_sample(5, &[true]), None);
        assert_eq!(engine.process_sample(6, &[true]), None);
        assert_eq!(engine.process_sample(7, &[true]), None);
        assert_eq!(engine.process_sample(8, &[true]), None);
        assert_eq!(engine.process_sample(9, &[true]), None);
        assert_eq!(engine.process_sample(10, &[false]), None);
        assert!(!engine.is_triggered());

        // Pulse in range (width = 4).
        assert_eq!(engine.process_sample(11, &[true]), None);
        assert_eq!(engine.process_sample(12, &[true]), None);
        assert_eq!(engine.process_sample(13, &[true]), None);
        assert_eq!(engine.process_sample(14, &[true]), None);
        assert_eq!(engine.process_sample(15, &[false]), Some(15));
        assert!(engine.is_triggered());
    }

    #[test]
    fn engine_mixed_edge_and_pulse() {
        // Edge on ch0 (rising) AND pulse on ch1 (active-high, width 2-4).
        // The pulse must trigger first (it fires at the trailing edge),
        // then the edge condition must fire on the same or later sample.
        let config = TriggerConfig {
            conditions: vec![
                TriggerCondition::Edge {
                    channel: 0,
                    edge_type: EdgeType::Rising,
                },
                TriggerCondition::Pulse {
                    channel: 1,
                    polarity: PulsePolarity::ActiveHigh,
                    min_samples: 2,
                    max_samples: 4,
                },
            ],
            pre_trigger_samples: 0,
        };
        let mut engine = TriggerEngine::new(config);

        // Set up initial values.
        assert_eq!(engine.process_sample(0, &[false, false]), None);

        // Start a qualifying pulse on ch1.
        assert_eq!(engine.process_sample(1, &[false, true]), None);
        assert_eq!(engine.process_sample(2, &[false, true]), None);
        assert_eq!(engine.process_sample(3, &[false, true]), None);
        // End pulse on ch1 (width=3) AND rise ch0 simultaneously.
        assert_eq!(engine.process_sample(4, &[true, false]), Some(4));
        assert!(engine.is_triggered());
    }

    #[test]
    fn engine_pre_trigger_samples() {
        let config = TriggerConfig {
            conditions: vec![TriggerCondition::Edge {
                channel: 0,
                edge_type: EdgeType::Rising,
            }],
            pre_trigger_samples: 100,
        };
        let mut engine = TriggerEngine::new(config);

        assert_eq!(engine.process_sample(0, &[false]), None);
        // Trigger fires at sample 500. Pre-trigger = 100.
        // Returned index = 500 - 100 = 400.
        for i in 1..500 {
            assert_eq!(engine.process_sample(i, &[false]), None);
        }
        assert_eq!(engine.process_sample(500, &[true]), Some(400));
        assert_eq!(engine.pre_trigger_samples(), 100);
    }

    #[test]
    fn engine_pre_trigger_saturates_at_zero() {
        let config = TriggerConfig {
            conditions: vec![TriggerCondition::Edge {
                channel: 0,
                edge_type: EdgeType::Rising,
            }],
            pre_trigger_samples: 1000,
        };
        let mut engine = TriggerEngine::new(config);

        assert_eq!(engine.process_sample(0, &[false]), None);
        // Trigger at sample 5, pre_trigger = 1000. Saturates to 0.
        assert_eq!(engine.process_sample(5, &[true]), Some(0));
    }

    #[test]
    fn engine_reset_and_rearm() {
        let config = TriggerConfig {
            conditions: vec![TriggerCondition::Edge {
                channel: 0,
                edge_type: EdgeType::Rising,
            }],
            pre_trigger_samples: 0,
        };
        let mut engine = TriggerEngine::new(config);

        assert_eq!(engine.process_sample(0, &[false]), None);
        assert_eq!(engine.process_sample(1, &[true]), Some(1));
        assert!(engine.is_triggered());

        engine.reset();
        assert!(!engine.is_triggered());
        assert_eq!(engine.trigger_sample(), None);

        // Can trigger again after reset.
        assert_eq!(engine.process_sample(10, &[false]), None);
        assert_eq!(engine.process_sample(11, &[true]), Some(11));
        assert!(engine.is_triggered());
    }

    #[test]
    fn engine_no_conditions_triggers_immediately() {
        // With no conditions, everything is vacuously true.
        // The first sample should trigger (all zero conditions match).
        let config = TriggerConfig {
            conditions: vec![],
            pre_trigger_samples: 0,
        };
        let mut engine = TriggerEngine::new(config);
        assert_eq!(engine.process_sample(0, &[false]), Some(0));
        assert!(engine.is_triggered());
    }

    #[test]
    fn engine_trigger_sample_accessor() {
        let config = TriggerConfig {
            conditions: vec![TriggerCondition::Edge {
                channel: 0,
                edge_type: EdgeType::Falling,
            }],
            pre_trigger_samples: 10,
        };
        let mut engine = TriggerEngine::new(config);

        assert_eq!(engine.trigger_sample(), None);

        assert_eq!(engine.process_sample(0, &[true]), None);
        assert_eq!(engine.process_sample(50, &[false]), Some(40));
        assert_eq!(engine.trigger_sample(), Some(40));
    }
}
