//! Analog sample processing pipeline.
//!
//! Converts raw ADC codes from the device into calibrated voltage samples
//! by chaining (matching the binary's processing order):
//!
//! 1. [`BiquadFilter`]: IIR biquad cascade on raw i16 ADC codes
//! 2. [`VoltageTransform`]: linear ADC code -> voltage conversion
//!
//! The binary (`libgraph_server_shared.so`) applies the IIR biquad filter
//! to raw i16 ADC samples first (converting to float, filtering, converting
//! back to i16 via truncation + saturation), then applies the voltage
//! transform in a separate graph node. Since the biquad filter is linear,
//! the order does not affect the final result for DC/steady-state signals,
//! but we match the binary's order for bit-exact consistency.
//!
//! The processor is stateful (the IIR filter has delay lines), so one
//! processor instance should be used per channel for the duration of a
//! capture session.
//!
//! # Example
//!
//! ```
//! use saleae_proto::analog::AnalogProcessor;
//! use saleae_proto::biquad::BiquadFilter;
//! use saleae_proto::calibration::{BiquadSection, VoltageTransform};
//!
//! // Identity filter (passthrough) for demonstration
//! let identity = BiquadSection { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0 };
//! let filter = BiquadFilter::new(&[identity]);
//!
//! // Uncalibrated Pro 8 transform (20V range, 12-bit signed ADC)
//! let transform = VoltageTransform::from_range(-10.0, 10.0, -2048, 2047);
//!
//! let mut proc = AnalogProcessor::new(transform, filter);
//! let raw = [0i16, 1024, -1024];
//! let voltages = proc.process(&raw);
//!
//! // ADC code 0 maps to roughly 0V (midpoint)
//! assert!((voltages[0]).abs() < 0.01);
//! ```

use crate::biquad::BiquadFilter;
use crate::calibration::VoltageTransform;

/// Analog sample processor: ADC codes -> calibrated voltages.
///
/// Applies IIR biquad filter to raw i16 ADC codes first (matching the
/// binary's processing order), then converts filtered codes to voltages
/// via a linear transform. The filter state is maintained across calls,
/// so samples must be fed in temporal order.
#[derive(Debug, Clone)]
pub struct AnalogProcessor {
    transform: VoltageTransform,
    filter: BiquadFilter,
}

impl AnalogProcessor {
    /// Create a new processor from a voltage transform and biquad filter.
    pub fn new(transform: VoltageTransform, filter: BiquadFilter) -> Self {
        Self { transform, filter }
    }

    /// Process a block of raw ADC samples into calibrated voltages.
    ///
    /// Each sample is first filtered as an i16 ADC code through the IIR
    /// biquad cascade (matching the binary's processing order), then
    /// converted to a voltage via the linear transform.
    ///
    /// Returns a new `Vec<f64>` of calibrated voltage values.
    pub fn process(&mut self, raw_samples: &[i16]) -> Vec<f64> {
        // Step 1: Filter raw i16 codes (binary-compatible order).
        let mut filtered = raw_samples.to_vec();
        self.filter.process_block_i16(&mut filtered);

        // Step 2: Convert filtered i16 codes to voltages.
        filtered
            .iter()
            .map(|&code| self.transform.apply(code))
            .collect()
    }

    /// Process a block of raw ADC samples into a pre-allocated output buffer.
    ///
    /// This avoids allocation when processing many blocks. The output
    /// buffer must be at least as long as the input.
    ///
    /// # Panics
    ///
    /// Panics if `output.len() < raw_samples.len()`.
    pub fn process_into(&mut self, raw_samples: &[i16], output: &mut [f64]) {
        assert!(
            output.len() >= raw_samples.len(),
            "output buffer too small: {} < {}",
            output.len(),
            raw_samples.len()
        );

        // Step 1: Filter raw i16 codes (binary-compatible order).
        let mut filtered = raw_samples.to_vec();
        self.filter.process_block_i16(&mut filtered);

        // Step 2: Convert filtered i16 codes to voltages.
        for (i, &code) in filtered.iter().enumerate() {
            output[i] = self.transform.apply(code);
        }
    }

    /// Reset the IIR filter state.
    ///
    /// Call this between non-contiguous signal blocks to avoid
    /// transient artifacts from stale delay lines.
    pub fn reset(&mut self) {
        self.filter.reset();
    }

    /// Access the underlying voltage transform.
    pub fn transform(&self) -> &VoltageTransform {
        &self.transform
    }

    /// Access the underlying biquad filter.
    pub fn filter(&self) -> &BiquadFilter {
        &self.filter
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calibration::{self, BiquadSection};

    fn identity_section() -> BiquadSection {
        BiquadSection {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
        }
    }

    fn reference_cal_path() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("re")
            .join("calibration-764051b51bd4d9f1.cal")
    }

    fn load_reference_cal() -> calibration::CalibrationData {
        let data = std::fs::read(reference_cal_path()).expect("read cal file");
        calibration::parse_calibration(&data).expect("parse cal")
    }

    // ---------------------------------------------------------------
    // Basic processor tests
    // ---------------------------------------------------------------

    #[test]
    fn identity_filter_passthrough() {
        // With an identity filter, output should be pure voltage transform
        let transform = VoltageTransform::from_range(-10.0, 10.0, -2048, 2047);
        let filter = BiquadFilter::new(&[identity_section()]);
        let mut proc = AnalogProcessor::new(transform, filter);

        let raw = [0i16, 2047, -2048, 1024, -1024];
        let result = proc.process(&raw);

        for (i, &code) in raw.iter().enumerate() {
            let expected = transform.apply(code);
            assert!(
                (result[i] - expected).abs() < 1e-12,
                "sample {}: expected {}, got {}",
                i,
                expected,
                result[i]
            );
        }
    }

    #[test]
    fn process_into_matches_process() {
        let transform = VoltageTransform::from_range(-10.0, 10.0, -2048, 2047);
        let filter = BiquadFilter::new(&[identity_section()]);

        let raw = [0i16, 100, -100, 500, -500, 2047, -2048];

        // process() path
        let mut proc1 = AnalogProcessor::new(transform, filter.clone());
        let expected = proc1.process(&raw);

        // process_into() path
        let mut proc2 = AnalogProcessor::new(transform, filter);
        let mut output = vec![0.0; raw.len()];
        proc2.process_into(&raw, &mut output);

        for i in 0..raw.len() {
            assert!(
                (expected[i] - output[i]).abs() < 1e-12,
                "sample {}: process={}, process_into={}",
                i,
                expected[i],
                output[i]
            );
        }
    }

    #[test]
    fn reset_clears_filter_state() {
        let transform = VoltageTransform::from_range(-10.0, 10.0, -2048, 2047);
        let section = BiquadSection {
            b0: 0.5,
            b1: 0.5,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
        };
        let filter = BiquadFilter::new(&[section]);

        let mut proc = AnalogProcessor::new(transform, filter.clone());
        // Feed some samples to build up state
        proc.process(&[100, 200, 300]);

        // Reset
        proc.reset();

        // After reset, behavior should match a fresh processor
        let mut fresh = AnalogProcessor::new(transform, filter);
        let y_reset = proc.process(&[500]);
        let y_fresh = fresh.process(&[500]);
        assert!(
            (y_reset[0] - y_fresh[0]).abs() < 1e-12,
            "after reset: {} vs fresh: {}",
            y_reset[0],
            y_fresh[0]
        );
    }

    // ---------------------------------------------------------------
    // Real calibration tests
    // ---------------------------------------------------------------

    #[test]
    fn real_cal_channel0_zero_code() {
        // ADC code 0 through real calibration for channel 0 @ 50MHz.
        // With calibration, code 0 should map near the voltage midpoint.
        let cal = load_reference_cal();
        let vr = cal.find_voltage_range(0).expect("ch0 voltage range");
        let set = cal.find_set(0, 50_000_000).expect("ch0 @ 50MHz");

        let transform = VoltageTransform::from_range(vr.min_voltage, vr.max_voltage, -2048, 2047);
        let filter = BiquadFilter::new(&set.sections);
        let mut proc = AnalogProcessor::new(transform, filter);

        // Feed many zeros to let the filter settle
        let zeros = vec![0i16; 10_000];
        let result = proc.process(&zeros);

        // The last sample should be near the DC voltage for code 0.
        // With calibration filter, the DC gain should be close to 1.0,
        // so the output should converge to transform.apply(0).
        let raw_v0 = transform.apply(0);
        let filtered_v0 = result[result.len() - 1];

        assert!(
            filtered_v0.is_finite(),
            "filtered voltage is not finite: {}",
            filtered_v0
        );
        // The IIR filter should converge close to the input DC value.
        // Allow 0.5V tolerance since the calibration filter has some gain.
        assert!(
            (filtered_v0 - raw_v0).abs() < 0.5,
            "code 0: raw={:.6} filtered={:.6} (delta={:.6})",
            raw_v0,
            filtered_v0,
            (filtered_v0 - raw_v0).abs()
        );
    }

    #[test]
    fn real_cal_channel0_full_scale() {
        // Process full-scale positive and negative ADC codes.
        let cal = load_reference_cal();
        let vr = cal.find_voltage_range(0).expect("ch0 voltage range");
        let set = cal.find_set(0, 50_000_000).expect("ch0 @ 50MHz");

        let transform = VoltageTransform::from_range(vr.min_voltage, vr.max_voltage, -2048, 2047);
        let filter = BiquadFilter::new(&set.sections);
        let mut proc = AnalogProcessor::new(transform, filter);

        // Feed constant max code to let filter settle
        let max_codes = vec![2047i16; 10_000];
        let result = proc.process(&max_codes);
        let v_max = result[result.len() - 1];

        assert!(v_max.is_finite());
        // Should be near max_voltage (within 1V for the IIR distortion)
        assert!(
            (v_max - vr.max_voltage).abs() < 1.0,
            "max code: expected ~{:.3}, got {:.3}",
            vr.max_voltage,
            v_max
        );
    }

    #[test]
    fn real_cal_all_channels_finite() {
        // Verify all 8 channels produce finite output with real calibration
        let cal = load_reference_cal();

        for ch in 0..8u8 {
            let vr = cal
                .find_voltage_range(ch)
                .unwrap_or_else(|| panic!("missing voltage range for ch{}", ch));
            let set = cal
                .find_set(ch, 50_000_000)
                .unwrap_or_else(|| panic!("missing cal set for ch{} @ 50MHz", ch));

            let transform =
                VoltageTransform::from_range(vr.min_voltage, vr.max_voltage, -2048, 2047);
            let filter = BiquadFilter::new(&set.sections);
            let mut proc = AnalogProcessor::new(transform, filter);

            // Feed a ramp of ADC codes
            let ramp: Vec<i16> = (-2048..=2047).collect();
            let result = proc.process(&ramp);

            for (i, &v) in result.iter().enumerate() {
                assert!(
                    v.is_finite(),
                    "ch{} sample {}: voltage is not finite ({})",
                    ch,
                    i,
                    v
                );
            }
        }
    }

    #[test]
    fn full_pipeline_disaggregate_transform_filter() {
        // End-to-end: raw interleaved bytes -> disaggregate -> transform -> filter
        use crate::capture_data::disaggregate_analog;

        let cal = load_reference_cal();

        // Build 2-channel interleaved data: ch0=0, ch1=1024 (repeated 100 times)
        let num_channels = 2;
        let num_samples = 100;
        let mut raw_bytes = Vec::with_capacity(num_channels * num_samples * 2);
        for _ in 0..num_samples {
            raw_bytes.extend_from_slice(&0i16.to_le_bytes());
            raw_bytes.extend_from_slice(&1024i16.to_le_bytes());
        }

        // Disaggregate
        let per_channel = disaggregate_analog(&raw_bytes, num_channels);
        assert_eq!(per_channel.len(), 2);
        assert_eq!(per_channel[0].len(), num_samples);
        assert_eq!(per_channel[1].len(), num_samples);

        // All ch0 samples should be 0, all ch1 should be 1024
        assert!(per_channel[0].iter().all(|&s| s == 0));
        assert!(per_channel[1].iter().all(|&s| s == 1024));

        // Create processors and process
        let mut processors: Vec<AnalogProcessor> = (0..2u8)
            .map(|ch| {
                let vr = cal.find_voltage_range(ch).unwrap();
                let set = cal.find_set(ch, 50_000_000).unwrap();
                let transform =
                    VoltageTransform::from_range(vr.min_voltage, vr.max_voltage, -2048, 2047);
                let filter = BiquadFilter::new(&set.sections);
                AnalogProcessor::new(transform, filter)
            })
            .collect();

        let v0 = processors[0].process(&per_channel[0]);
        let v1 = processors[1].process(&per_channel[1]);

        // Verify all outputs are finite
        assert!(v0.iter().all(|v| v.is_finite()));
        assert!(v1.iter().all(|v| v.is_finite()));

        // After settling, ch0 (code 0) should be near midpoint (~0V)
        // and ch1 (code 1024) should be positive
        let v0_last = v0[v0.len() - 1];
        let v1_last = v1[v1.len() - 1];

        assert!(
            v0_last.abs() < 1.0,
            "ch0 (code 0) should be near 0V, got {:.3}",
            v0_last
        );
        assert!(
            v1_last > 0.0,
            "ch1 (code 1024) should be positive, got {:.3}",
            v1_last
        );
    }

    #[test]
    fn voltage_transform_channel0_values() {
        // Verify specific ADC-to-voltage transform values from the task spec.
        // Channel 0: min=-10.444522, max=10.245802
        // m = (10.245802 - (-10.444522)) / 4095 = 20.690324 / 4095 ~ 0.005052
        // b = -10.444522 + 2048 * 0.005052 ~ -0.098077
        let cal = load_reference_cal();
        let vr = cal.find_voltage_range(0).unwrap();
        let transform = VoltageTransform::from_range(vr.min_voltage, vr.max_voltage, -2048, 2047);

        // Scale should be ~0.005052
        let expected_scale = (vr.max_voltage - vr.min_voltage) / 4095.0;
        assert!(
            (transform.scale - expected_scale).abs() < 1e-6,
            "scale: expected {:.6}, got {:.6}",
            expected_scale,
            transform.scale
        );

        // Offset should be ~-0.098
        let expected_offset = vr.min_voltage - (-2048.0) * expected_scale;
        assert!(
            (transform.offset - expected_offset).abs() < 1e-6,
            "offset: expected {:.6}, got {:.6}",
            expected_offset,
            transform.offset
        );

        // Code -2048 -> min_voltage
        let v_min = transform.apply(-2048);
        assert!(
            (v_min - vr.min_voltage).abs() < 0.01,
            "code -2048: expected {:.3}, got {:.3}",
            vr.min_voltage,
            v_min
        );

        // Code 2047 -> max_voltage
        let v_max = transform.apply(2047);
        assert!(
            (v_max - vr.max_voltage).abs() < 0.01,
            "code 2047: expected {:.3}, got {:.3}",
            vr.max_voltage,
            v_max
        );
    }
}
