//! Capture comparison: compare Rust driver output against Saleae Logic 2 captures.
//!
//! Provides infrastructure for end-to-end validation by comparing sample data
//! from two capture sources (typically our Rust driver vs. Saleae Logic 2).
//!
//! # Supported input formats
//!
//! - `.sal` files (Saleae Logic 2 capture archives, via [`SalReader`])
//! - Binary per-channel files (`digital_N.bin`, `analog_N.bin`)
//! - CSV exports (`digital.csv`, `analog.csv`)
//!
//! # Comparison modes
//!
//! - **Digital**: Bit-exact comparison with match percentage and first-mismatch index.
//! - **Analog**: Tolerance-based comparison with statistics (max deviation, mean
//!   absolute error, match percentage within tolerance).
//!
//! [`SalReader`]: crate::sal::SalReader

use std::fmt;

// ---------------------------------------------------------------------------
// Digital comparison
// ---------------------------------------------------------------------------

/// Result of comparing two digital sample streams.
#[derive(Debug, Clone)]
pub struct DigitalCompareResult {
    /// Channel index being compared.
    pub channel: u8,
    /// Total samples compared (min of the two input lengths).
    pub samples_compared: u64,
    /// Number of samples that match exactly.
    pub matches: u64,
    /// Sample index of the first mismatch, if any.
    pub first_mismatch: Option<u64>,
    /// Length of reference stream (e.g., Saleae Logic 2).
    pub reference_length: u64,
    /// Length of candidate stream (e.g., our Rust driver).
    pub candidate_length: u64,
}

impl DigitalCompareResult {
    /// Match percentage (0.0 to 100.0).
    pub fn match_pct(&self) -> f64 {
        if self.samples_compared == 0 {
            return 100.0; // vacuously true
        }
        (self.matches as f64 / self.samples_compared as f64) * 100.0
    }

    /// Whether the two streams are bit-exact over the compared range.
    pub fn is_exact_match(&self) -> bool {
        self.matches == self.samples_compared && self.reference_length == self.candidate_length
    }
}

impl fmt::Display for DigitalCompareResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Digital ch{}: {}/{} match ({:.2}%)",
            self.channel,
            self.matches,
            self.samples_compared,
            self.match_pct()
        )?;
        if self.reference_length != self.candidate_length {
            write!(
                f,
                " [length mismatch: ref={}, cand={}]",
                self.reference_length, self.candidate_length
            )?;
        }
        if let Some(idx) = self.first_mismatch {
            write!(f, " [first mismatch at sample {}]", idx)?;
        }
        Ok(())
    }
}

/// Compare two digital sample streams bit-by-bit.
///
/// Compares up to `min(reference.len(), candidate.len())` samples.
/// Reports match statistics and the index of the first mismatch.
pub fn compare_digital(
    channel: u8,
    reference: &[bool],
    candidate: &[bool],
) -> DigitalCompareResult {
    let samples_compared = reference.len().min(candidate.len()) as u64;
    let mut matches = 0u64;
    let mut first_mismatch: Option<u64> = None;

    for i in 0..samples_compared as usize {
        if reference[i] == candidate[i] {
            matches += 1;
        } else if first_mismatch.is_none() {
            first_mismatch = Some(i as u64);
        }
    }

    DigitalCompareResult {
        channel,
        samples_compared,
        matches,
        first_mismatch,
        reference_length: reference.len() as u64,
        candidate_length: candidate.len() as u64,
    }
}

/// Compare two digital sample streams from transition-encoded representation.
///
/// Transition encoding stores sample indices where the signal toggles.
/// Initial state is assumed low (0) for both streams. This is the native
/// format in `.sal` digital channel data.
///
/// `total_samples` specifies the total capture length to expand transitions
/// into a full boolean array. If 0, uses the max transition index + 1.
pub fn compare_digital_transitions(
    channel: u8,
    reference_transitions: &[u64],
    candidate_transitions: &[u64],
    total_samples: u64,
) -> DigitalCompareResult {
    let ref_samples = expand_transitions(reference_transitions, total_samples);
    let cand_samples = expand_transitions(candidate_transitions, total_samples);
    compare_digital(channel, &ref_samples, &cand_samples)
}

/// Expand transition timestamps into a boolean sample array.
///
/// Initial state is low (false). Each transition toggles the state.
/// Used internally by [`crate::sal::SalReader::read_digital_samples`].
pub fn expand_transitions(transitions: &[u64], total_samples: u64) -> Vec<bool> {
    let length = if total_samples > 0 {
        total_samples as usize
    } else if let Some(&last) = transitions.last() {
        (last + 1) as usize
    } else {
        return Vec::new();
    };

    let mut samples = vec![false; length];
    let mut state = false;
    let mut prev_idx = 0usize;

    for &t in transitions {
        let t = t as usize;
        if t >= length {
            break;
        }
        // Fill from prev_idx to t with current state (already set for false)
        if state {
            for sample in &mut samples[prev_idx..t] {
                *sample = true;
            }
        }
        state = !state;
        prev_idx = t;
    }
    // Fill remaining with final state
    if state {
        for sample in &mut samples[prev_idx..] {
            *sample = true;
        }
    }

    samples
}

// ---------------------------------------------------------------------------
// Analog comparison
// ---------------------------------------------------------------------------

/// Result of comparing two analog sample streams.
#[derive(Debug, Clone)]
pub struct AnalogCompareResult {
    /// Channel index being compared.
    pub channel: u8,
    /// Total samples compared (min of the two input lengths).
    pub samples_compared: u64,
    /// Number of samples within tolerance.
    pub matches_within_tolerance: u64,
    /// Tolerance used (volts).
    pub tolerance_volts: f64,
    /// Maximum absolute deviation observed.
    pub max_deviation: f64,
    /// Mean absolute error.
    pub mean_absolute_error: f64,
    /// Root mean square error.
    pub rms_error: f64,
    /// Sample index of the maximum deviation.
    pub max_deviation_index: Option<u64>,
    /// Length of reference stream.
    pub reference_length: u64,
    /// Length of candidate stream.
    pub candidate_length: u64,
}

impl AnalogCompareResult {
    /// Match percentage within tolerance (0.0 to 100.0).
    pub fn match_pct(&self) -> f64 {
        if self.samples_compared == 0 {
            return 100.0;
        }
        (self.matches_within_tolerance as f64 / self.samples_compared as f64) * 100.0
    }
}

impl fmt::Display for AnalogCompareResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Analog ch{}: {}/{} within {:.3}V ({:.2}%)",
            self.channel,
            self.matches_within_tolerance,
            self.samples_compared,
            self.tolerance_volts,
            self.match_pct()
        )?;
        write!(
            f,
            " [max_dev={:.4}V, mae={:.4}V, rms={:.4}V]",
            self.max_deviation, self.mean_absolute_error, self.rms_error
        )?;
        if self.reference_length != self.candidate_length {
            write!(
                f,
                " [length mismatch: ref={}, cand={}]",
                self.reference_length, self.candidate_length
            )?;
        }
        Ok(())
    }
}

/// Compare two analog sample streams with tolerance.
///
/// Compares up to `min(reference.len(), candidate.len())` samples.
/// A sample is considered matching if `|ref - cand| <= tolerance_volts`.
pub fn compare_analog(
    channel: u8,
    reference: &[f64],
    candidate: &[f64],
    tolerance_volts: f64,
) -> AnalogCompareResult {
    let n = reference.len().min(candidate.len());
    let mut matches = 0u64;
    let mut max_dev = 0.0f64;
    let mut max_dev_idx: Option<u64> = None;
    let mut sum_abs_err = 0.0f64;
    let mut sum_sq_err = 0.0f64;

    for i in 0..n {
        let diff = (reference[i] - candidate[i]).abs();
        if diff <= tolerance_volts {
            matches += 1;
        }
        if diff > max_dev {
            max_dev = diff;
            max_dev_idx = Some(i as u64);
        }
        sum_abs_err += diff;
        sum_sq_err += diff * diff;
    }

    let mae = if n > 0 { sum_abs_err / n as f64 } else { 0.0 };
    let rms = if n > 0 {
        (sum_sq_err / n as f64).sqrt()
    } else {
        0.0
    };

    AnalogCompareResult {
        channel,
        samples_compared: n as u64,
        matches_within_tolerance: matches,
        tolerance_volts,
        max_deviation: max_dev,
        mean_absolute_error: mae,
        rms_error: rms,
        max_deviation_index: max_dev_idx,
        reference_length: reference.len() as u64,
        candidate_length: candidate.len() as u64,
    }
}

// ---------------------------------------------------------------------------
// Aggregate report
// ---------------------------------------------------------------------------

/// Aggregate comparison report across multiple channels.
#[derive(Debug, Clone)]
pub struct CompareReport {
    /// Digital channel comparison results.
    pub digital: Vec<DigitalCompareResult>,
    /// Analog channel comparison results.
    pub analog: Vec<AnalogCompareResult>,
}

impl CompareReport {
    /// Create an empty report.
    pub fn new() -> Self {
        Self {
            digital: Vec::new(),
            analog: Vec::new(),
        }
    }

    /// Whether all digital comparisons are exact matches.
    pub fn all_digital_exact(&self) -> bool {
        self.digital.iter().all(|r| r.is_exact_match())
    }

    /// Whether all analog comparisons meet tolerance.
    pub fn all_analog_within_tolerance(&self) -> bool {
        self.analog
            .iter()
            .all(|r| r.matches_within_tolerance == r.samples_compared)
    }

    /// Overall pass: all digital exact AND all analog within tolerance.
    pub fn passed(&self) -> bool {
        self.all_digital_exact() && self.all_analog_within_tolerance()
    }
}

impl Default for CompareReport {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for CompareReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "=== Capture Comparison Report ===")?;

        if !self.digital.is_empty() {
            writeln!(f, "\nDigital channels:")?;
            for r in &self.digital {
                writeln!(f, "  {r}")?;
            }
        }

        if !self.analog.is_empty() {
            writeln!(f, "\nAnalog channels:")?;
            for r in &self.analog {
                writeln!(f, "  {r}")?;
            }
        }

        let status = if self.passed() { "PASS" } else { "FAIL" };
        writeln!(f, "\nOverall: {status}")?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------
    // expand_transitions
    // -------------------------------------------------------------------

    #[test]
    fn expand_transitions_empty() {
        let result = expand_transitions(&[], 0);
        assert!(result.is_empty());
    }

    #[test]
    fn expand_transitions_with_total() {
        let result = expand_transitions(&[], 8);
        assert_eq!(result, vec![false; 8]);
    }

    #[test]
    fn expand_transitions_single_toggle() {
        // Low for 4 samples, then high for 4 samples
        let result = expand_transitions(&[4], 8);
        assert_eq!(
            result,
            vec![false, false, false, false, true, true, true, true]
        );
    }

    #[test]
    fn expand_transitions_two_toggles() {
        // Low[0..2], High[2..5], Low[5..8]
        let result = expand_transitions(&[2, 5], 8);
        assert_eq!(
            result,
            vec![false, false, true, true, true, false, false, false]
        );
    }

    #[test]
    fn expand_transitions_square_wave() {
        // Toggle every 2 samples: L,L,H,H,L,L,H,H
        let result = expand_transitions(&[2, 4, 6, 8], 10);
        assert_eq!(
            result,
            vec![false, false, true, true, false, false, true, true, false, false]
        );
    }

    #[test]
    fn expand_transitions_auto_length() {
        let result = expand_transitions(&[3, 7], 0);
        // Length = 7 + 1 = 8
        assert_eq!(result.len(), 8);
        // Low[0..3], High[3..7], Low[7]
        assert_eq!(
            result,
            vec![false, false, false, true, true, true, true, false]
        );
    }

    // -------------------------------------------------------------------
    // compare_digital
    // -------------------------------------------------------------------

    #[test]
    fn digital_exact_match() {
        let data = vec![true, false, true, true, false];
        let result = compare_digital(0, &data, &data);
        assert!(result.is_exact_match());
        assert_eq!(result.match_pct(), 100.0);
        assert_eq!(result.first_mismatch, None);
    }

    #[test]
    fn digital_total_mismatch() {
        let reference = vec![true, true, true, true];
        let candidate = vec![false, false, false, false];
        let result = compare_digital(0, &reference, &candidate);
        assert_eq!(result.matches, 0);
        assert_eq!(result.match_pct(), 0.0);
        assert_eq!(result.first_mismatch, Some(0));
    }

    #[test]
    fn digital_partial_mismatch() {
        let reference = vec![true, false, true, false, true];
        let candidate = vec![true, false, false, false, true];
        let result = compare_digital(0, &reference, &candidate);
        assert_eq!(result.matches, 4);
        assert_eq!(result.samples_compared, 5);
        assert_eq!(result.first_mismatch, Some(2));
    }

    #[test]
    fn digital_different_lengths() {
        let reference = vec![true, false, true];
        let candidate = vec![true, false, true, false, true];
        let result = compare_digital(0, &reference, &candidate);
        assert_eq!(result.samples_compared, 3);
        assert_eq!(result.matches, 3);
        assert!(!result.is_exact_match()); // lengths differ
        assert_eq!(result.reference_length, 3);
        assert_eq!(result.candidate_length, 5);
    }

    #[test]
    fn digital_empty() {
        let result = compare_digital(0, &[], &[]);
        assert!(result.is_exact_match());
        assert_eq!(result.match_pct(), 100.0);
    }

    // -------------------------------------------------------------------
    // compare_digital_transitions
    // -------------------------------------------------------------------

    #[test]
    fn transitions_identical() {
        let transitions = vec![100, 200, 300];
        let result = compare_digital_transitions(0, &transitions, &transitions, 500);
        assert!(result.is_exact_match());
    }

    #[test]
    fn transitions_different() {
        let ref_t = vec![100, 200];
        let cand_t = vec![100, 250]; // high phase extends 50 samples longer
        let result = compare_digital_transitions(0, &ref_t, &cand_t, 400);
        assert!(!result.is_exact_match());
        // Samples 200..250 differ: ref is low, candidate is high
        assert_eq!(result.first_mismatch, Some(200));
    }

    // -------------------------------------------------------------------
    // compare_analog
    // -------------------------------------------------------------------

    #[test]
    fn analog_exact_match() {
        let data = vec![1.0, 2.0, 3.0, -1.5];
        let result = compare_analog(0, &data, &data, 0.0);
        assert_eq!(result.match_pct(), 100.0);
        assert_eq!(result.max_deviation, 0.0);
        assert_eq!(result.mean_absolute_error, 0.0);
    }

    #[test]
    fn analog_within_tolerance() {
        let reference = vec![1.0, 2.0, 3.0];
        let candidate = vec![1.01, 2.02, 2.98];
        let result = compare_analog(0, &reference, &candidate, 0.05);
        assert_eq!(result.matches_within_tolerance, 3);
        assert_eq!(result.match_pct(), 100.0);
    }

    #[test]
    fn analog_partial_tolerance() {
        let reference = vec![1.0, 2.0, 3.0];
        let candidate = vec![1.01, 2.2, 2.98]; // 2.2 is 0.2 from 2.0
        let result = compare_analog(0, &reference, &candidate, 0.05);
        assert_eq!(result.matches_within_tolerance, 2); // 1st and 3rd match
        assert!(result.max_deviation > 0.19 && result.max_deviation < 0.21);
    }

    #[test]
    fn analog_statistics() {
        let reference = vec![0.0, 0.0, 0.0, 0.0];
        let candidate = vec![0.1, 0.2, 0.3, 0.4];
        let result = compare_analog(0, &reference, &candidate, 1.0);

        // MAE = (0.1 + 0.2 + 0.3 + 0.4) / 4 = 0.25
        assert!((result.mean_absolute_error - 0.25).abs() < 1e-10);

        // Max deviation = 0.4
        assert!((result.max_deviation - 0.4).abs() < 1e-10);
        assert_eq!(result.max_deviation_index, Some(3));

        // RMS = sqrt((0.01 + 0.04 + 0.09 + 0.16) / 4) = sqrt(0.075)
        let expected_rms = (0.075f64).sqrt();
        assert!((result.rms_error - expected_rms).abs() < 1e-10);
    }

    #[test]
    fn analog_empty() {
        let result = compare_analog(0, &[], &[], 0.1);
        assert_eq!(result.match_pct(), 100.0);
        assert_eq!(result.samples_compared, 0);
    }

    #[test]
    fn analog_different_lengths() {
        let reference = vec![1.0, 2.0, 3.0];
        let candidate = vec![1.0, 2.0];
        let result = compare_analog(0, &reference, &candidate, 0.01);
        assert_eq!(result.samples_compared, 2);
        assert_eq!(result.reference_length, 3);
        assert_eq!(result.candidate_length, 2);
    }

    // -------------------------------------------------------------------
    // CompareReport
    // -------------------------------------------------------------------

    #[test]
    fn report_pass() {
        let mut report = CompareReport::new();
        let data = vec![true, false, true];
        report.digital.push(compare_digital(0, &data, &data));
        let adata = vec![1.0, 2.0];
        report.analog.push(compare_analog(0, &adata, &adata, 0.01));
        assert!(report.passed());
    }

    #[test]
    fn report_fail_digital() {
        let mut report = CompareReport::new();
        let reference = vec![true, false];
        let candidate = vec![false, true];
        report
            .digital
            .push(compare_digital(0, &reference, &candidate));
        assert!(!report.passed());
    }

    #[test]
    fn report_fail_analog() {
        let mut report = CompareReport::new();
        let reference = vec![1.0, 2.0];
        let candidate = vec![1.0, 3.0]; // off by 1.0
        report
            .analog
            .push(compare_analog(0, &reference, &candidate, 0.01));
        assert!(!report.passed());
    }

    #[test]
    fn report_display() {
        let mut report = CompareReport::new();
        let data = vec![true, false, true];
        report.digital.push(compare_digital(0, &data, &data));
        let s = format!("{report}");
        assert!(s.contains("PASS"));
        assert!(s.contains("Digital ch0"));
    }
}
