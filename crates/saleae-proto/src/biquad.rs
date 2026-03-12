//! IIR biquad filter in Direct Form I.
//!
//! Implements a cascade of second-order sections (SOS) for calibration
//! filter application. Each section processes the signal as:
//!
//! ```text
//! y[n] = b0*x[n] + b1*x[n-1] + b2*x[n-2] - a1*y[n-1] - a2*y[n-2]
//! ```
//!
//! The cascade feeds the output of one section into the input of the next.
//! Section 0 is typically first-order (b2=0, a2=0) but is processed
//! identically since the zero terms are harmless.
//!
//! ## Binary implementation notes
//!
//! The Saleae binary (`libgraph_server_shared.so`) uses SIMD-optimized
//! implementations that convert the DF1 SOS coefficients into a companion
//! matrix (state-space) form. This allows processing multiple samples per
//! iteration without data dependencies between biquad stages.
//!
//! Two implementations exist:
//! - `SimdIirFilterPrototype`: SSE4.1, processes 4 samples per iteration
//! - `SimdPrototype_Static2By3IirFilter`: AVX/FMA, processes 8 samples
//!
//! Both use `float` (f32) precision internally. Our scalar implementation
//! uses `f64` for better accuracy and is mathematically equivalent.
//!
//! The binary operates on raw `i16` ADC samples: converts to float,
//! applies the filter matrix, converts back to `i32` via `cvttss2si`
//! (truncation toward zero), then packs to `i16` via `packssdw` (which
//! saturates rather than wraps). The ADC-to-voltage transform is applied
//! in a separate graph node, not in the filter.

use crate::calibration::BiquadSection;

/// Per-section delay line for Direct Form I.
#[derive(Debug, Clone)]
struct BiquadState {
    x1: f64, // x[n-1]
    x2: f64, // x[n-2]
    y1: f64, // y[n-1]
    y2: f64, // y[n-2]
}

impl BiquadState {
    fn new() -> Self {
        Self {
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }
}

/// Cascaded biquad filter (Direct Form I).
///
/// Processes samples through a chain of second-order sections.
/// The output of each section becomes the input of the next.
#[derive(Debug, Clone)]
pub struct BiquadFilter {
    sections: Vec<BiquadSection>,
    state: Vec<BiquadState>,
}

impl BiquadFilter {
    /// Create a new biquad filter from a slice of sections.
    ///
    /// The filter starts with zeroed delay lines (no transient).
    pub fn new(sections: &[BiquadSection]) -> Self {
        let state = vec![BiquadState::new(); sections.len()];
        Self {
            sections: sections.to_vec(),
            state,
        }
    }

    /// Process a single sample through the filter cascade.
    ///
    /// Each section applies the DF1 biquad equation:
    /// `y[n] = b0*x[n] + b1*x[n-1] + b2*x[n-2] - a1*y[n-1] - a2*y[n-2]`
    ///
    /// The output of section k becomes the input of section k+1.
    pub fn process_sample(&mut self, x: f64) -> f64 {
        let mut signal = x;
        for (section, state) in self.sections.iter().zip(self.state.iter_mut()) {
            let y = section.b0 * signal + section.b1 * state.x1 + section.b2 * state.x2
                - section.a1 * state.y1
                - section.a2 * state.y2;

            // Shift delay line
            state.x2 = state.x1;
            state.x1 = signal;
            state.y2 = state.y1;
            state.y1 = y;

            signal = y;
        }
        signal
    }

    /// Process a block of samples in-place.
    ///
    /// Each sample is processed through the full cascade before
    /// moving to the next sample. This maintains correct filter state.
    pub fn process_block(&mut self, samples: &mut [f64]) {
        for sample in samples.iter_mut() {
            *sample = self.process_sample(*sample);
        }
    }

    /// Reset all delay lines to zero.
    ///
    /// Call this between non-contiguous signal blocks to avoid
    /// transient artifacts from stale state.
    pub fn reset(&mut self) {
        for state in &mut self.state {
            *state = BiquadState::new();
        }
    }

    /// Return the number of sections in the cascade.
    pub fn num_sections(&self) -> usize {
        self.sections.len()
    }

    /// Process a block of raw ADC samples (i16), applying the filter in-place.
    ///
    /// This mirrors the binary's processing pipeline:
    /// 1. Convert i16 -> f64 (binary uses f32, we use f64 for accuracy)
    /// 2. Apply biquad cascade
    /// 3. Convert f64 -> i32 via truncation toward zero (matching SSE cvttss2si)
    /// 4. Saturate i32 -> i16 via clamping (matching SSE packssdw)
    ///
    /// The binary uses `cvttss2si` for float-to-int (truncation, not rounding)
    /// and `packssdw` for the i32-to-i16 pack (saturates to i16 range rather
    /// than wrapping).
    pub fn process_block_i16(&mut self, samples: &mut [i16]) {
        for sample in samples.iter_mut() {
            let x = *sample as f64;
            let y = self.process_sample(x);
            // Truncate toward zero (matching cvttss2si), then saturate to i16
            let yi = y as i32; // Rust `as i32` truncates toward zero, like cvttss2si
            *sample = yi.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calibration::BiquadSection;

    /// Create an identity (pass-through) biquad: b0=1, all others 0.
    fn identity_section() -> BiquadSection {
        BiquadSection {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
        }
    }

    /// Create a simple low-pass biquad section for testing.
    /// This is a very gentle low-pass that preserves DC gain = 1.0.
    fn lowpass_section() -> BiquadSection {
        // Simple averaging filter: y = 0.5*x + 0.5*x[n-1]
        // Transfer function: H(z) = 0.5 + 0.5*z^-1
        // DC gain: H(1) = 1.0
        BiquadSection {
            b0: 0.5,
            b1: 0.5,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
        }
    }

    #[test]
    fn identity_filter_passes_through() {
        let sections = [identity_section()];
        let mut filter = BiquadFilter::new(&sections);

        let input = [1.0, 2.0, 3.0, -1.0, 0.0, 5.5];
        for &x in &input {
            let y = filter.process_sample(x);
            assert!(
                (y - x).abs() < 1e-15,
                "identity filter: expected {}, got {}",
                x,
                y
            );
        }
    }

    #[test]
    fn identity_cascade_passes_through() {
        // Multiple identity sections should still pass through
        let sections = [identity_section(), identity_section(), identity_section()];
        let mut filter = BiquadFilter::new(&sections);

        for x in [1.0, -1.0, 0.5, 100.0] {
            let y = filter.process_sample(x);
            assert!((y - x).abs() < 1e-15);
        }
    }

    #[test]
    fn impulse_response_nonzero() {
        // Feed an impulse through a non-trivial filter and verify output
        let sections = [lowpass_section()];
        let mut filter = BiquadFilter::new(&sections);

        // Impulse: [1, 0, 0, ...]
        let y0 = filter.process_sample(1.0);
        let y1 = filter.process_sample(0.0);
        let y2 = filter.process_sample(0.0);

        // y[0] = 0.5*1 + 0.5*0 = 0.5
        assert!((y0 - 0.5).abs() < 1e-15, "y[0] = {}", y0);
        // y[1] = 0.5*0 + 0.5*1 = 0.5
        assert!((y1 - 0.5).abs() < 1e-15, "y[1] = {}", y1);
        // y[2] = 0.5*0 + 0.5*0 = 0.0
        assert!(y2.abs() < 1e-15, "y[2] = {}", y2);
    }

    #[test]
    fn dc_gain_converges() {
        // Feed constant 1.0 through a low-pass filter.
        // DC gain should be 1.0 (H(1) = 0.5 + 0.5 = 1.0).
        let sections = [lowpass_section()];
        let mut filter = BiquadFilter::new(&sections);

        let mut last = 0.0;
        for _ in 0..100 {
            last = filter.process_sample(1.0);
        }
        assert!(
            (last - 1.0).abs() < 1e-10,
            "DC gain did not converge to 1.0: got {}",
            last
        );
    }

    #[test]
    fn process_block_matches_sample_by_sample() {
        let sections = [lowpass_section(), identity_section()];

        // Process sample-by-sample
        let mut filter1 = BiquadFilter::new(&sections);
        let input = [1.0, 0.5, -0.3, 2.0, 0.0, -1.0, 0.7];
        let expected: Vec<f64> = input.iter().map(|&x| filter1.process_sample(x)).collect();

        // Process as block
        let mut filter2 = BiquadFilter::new(&sections);
        let mut block = input.to_vec();
        filter2.process_block(&mut block);

        for (i, (&exp, &got)) in expected.iter().zip(block.iter()).enumerate() {
            assert!(
                (exp - got).abs() < 1e-15,
                "sample {}: expected {}, got {}",
                i,
                exp,
                got
            );
        }
    }

    #[test]
    fn reset_clears_state() {
        let sections = [lowpass_section()];
        let mut filter = BiquadFilter::new(&sections);

        // Process some samples to fill state
        filter.process_sample(1.0);
        filter.process_sample(2.0);
        filter.process_sample(3.0);

        // Reset
        filter.reset();

        // After reset, processing should behave as if fresh
        let mut fresh = BiquadFilter::new(&sections);
        let y_reset = filter.process_sample(5.0);
        let y_fresh = fresh.process_sample(5.0);
        assert!(
            (y_reset - y_fresh).abs() < 1e-15,
            "after reset: {} vs fresh: {}",
            y_reset,
            y_fresh
        );
    }

    #[test]
    fn real_calibration_impulse_response() {
        // Use actual coefficients from channel 0 @ 50MHz to verify
        // the filter produces non-trivial output.
        let cal_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("re")
            .join("calibration-764051b51bd4d9f1.cal");

        let data = std::fs::read(&cal_path).expect("read cal file");
        let cal = crate::calibration::parse_calibration(&data).expect("parse cal");
        let set = cal.find_set(0, 50_000_000).expect("ch0 @ 50MHz");

        let mut filter = BiquadFilter::new(&set.sections);

        // Impulse response -- the cascade of 5 sections (including low-pass
        // stages with small b0) means y[0] = product of all b0's, which can
        // be small. We just verify it's non-zero.
        let y0 = filter.process_sample(1.0);
        assert!(y0.abs() > 1e-10, "impulse response y[0] is zero: {}", y0);

        // Continue processing zeros -- should produce non-zero tail
        let mut any_nonzero = false;
        for _ in 0..100 {
            let y = filter.process_sample(0.0);
            if y.abs() > 1e-10 {
                any_nonzero = true;
            }
        }
        assert!(any_nonzero, "impulse response tail is all zeros (not IIR)");
    }

    #[test]
    fn real_calibration_dc_converges() {
        // Feed DC (constant 1.0) through real calibration filter.
        // It should converge to some finite non-zero value.
        let cal_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("re")
            .join("calibration-764051b51bd4d9f1.cal");

        let data = std::fs::read(&cal_path).expect("read cal file");
        let cal = crate::calibration::parse_calibration(&data).expect("parse cal");
        let set = cal.find_set(0, 50_000_000).expect("ch0 @ 50MHz");

        let mut filter = BiquadFilter::new(&set.sections);

        // Feed 10000 samples of DC=1.0, should converge
        let mut last = 0.0;
        for _ in 0..10_000 {
            last = filter.process_sample(1.0);
        }

        assert!(last.is_finite(), "DC response is not finite: {}", last);
        assert!(
            last.abs() > 0.01,
            "DC response converged to near-zero: {}",
            last
        );

        // Check convergence: last few samples should be close
        let prev = filter.process_sample(1.0);
        assert!(
            (last - prev).abs() < 1e-6,
            "DC response has not converged: {} vs {}",
            last,
            prev
        );
    }

    #[test]
    fn process_block_i16_identity() {
        let sections = [identity_section()];
        let mut filter = BiquadFilter::new(&sections);

        let mut samples: Vec<i16> = vec![0, 100, -100, 1000, -1000, i16::MAX, i16::MIN];
        let original = samples.clone();
        filter.process_block_i16(&mut samples);

        for (i, (&orig, &got)) in original.iter().zip(samples.iter()).enumerate() {
            assert_eq!(orig, got, "sample {}: expected {}, got {}", i, orig, got);
        }
    }

    #[test]
    fn process_block_i16_saturation() {
        // A filter with gain > 1 should saturate rather than wrap
        let sections = [BiquadSection {
            b0: 2.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
        }];
        let mut filter = BiquadFilter::new(&sections);

        let mut samples: Vec<i16> = vec![i16::MAX, i16::MIN];
        filter.process_block_i16(&mut samples);

        // 32767 * 2 = 65534, saturated to 32767
        assert_eq!(samples[0], i16::MAX);
        // -32768 * 2 = -65536, saturated to -32768
        assert_eq!(samples[1], i16::MIN);
    }

    #[test]
    fn process_block_i16_truncates_toward_zero() {
        // The binary uses cvttss2si which truncates toward zero, NOT round-to-nearest.
        // A gain of 1.7 applied to input 1 should produce 1 (truncated from 1.7),
        // not 2 (rounded from 1.7).
        let sections = [BiquadSection {
            b0: 1.7,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
        }];
        let mut filter = BiquadFilter::new(&sections);

        let mut samples: Vec<i16> = vec![1, -1, 10, -10];
        filter.process_block_i16(&mut samples);

        // 1 * 1.7 = 1.7 -> truncated to 1 (not rounded to 2)
        assert_eq!(samples[0], 1, "positive should truncate toward zero");
        // -1 * 1.7 = -1.7 -> truncated to -1 (not rounded to -2)
        assert_eq!(samples[1], -1, "negative should truncate toward zero");
        // 10 * 1.7 = 17.0 -> exactly 17
        assert_eq!(samples[2], 17);
        // -10 * 1.7 = -17.0 -> exactly -17
        assert_eq!(samples[3], -17);
    }

    #[test]
    fn process_block_i16_real_calibration() {
        // Process a DC signal through real calibration filter
        let cal_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("re")
            .join("calibration-764051b51bd4d9f1.cal");

        let data = std::fs::read(&cal_path).expect("read cal file");
        let cal = crate::calibration::parse_calibration(&data).expect("parse cal");
        let set = cal.find_set(0, 50_000_000).expect("ch0 @ 50MHz");

        let mut filter = BiquadFilter::new(&set.sections);

        // Feed 1000 samples of ADC mid-scale (0) followed by some non-zero
        let mut samples = vec![0i16; 1000];
        filter.process_block_i16(&mut samples);
        // DC=0 through any linear filter should stay 0
        assert_eq!(samples[999], 0, "DC=0 should remain 0 after filtering");

        // Now feed some non-zero values and verify output is non-zero
        filter.reset();
        let mut samples = vec![1000i16; 1000];
        filter.process_block_i16(&mut samples);
        // After settling, output should be non-zero (filter has gain near 1)
        assert!(
            samples[999] != 0,
            "DC=1000 should produce non-zero output after settling"
        );
    }
}
