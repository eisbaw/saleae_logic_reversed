//! Analog calibration pipeline: raw ADC codes -> calibrated voltages.
//!
//! Wires together the calibration parser, biquad filter, and voltage
//! transform to process disaggregated per-channel analog data.
//!
//! # Processing order
//!
//! The binary (`libgraph_server_shared.so`) applies the IIR biquad filter
//! to raw i16 ADC samples first (converting to float, filtering, converting
//! back to i16 via truncation + saturation), then applies the voltage
//! transform in a separate graph node. Since the biquad filter is linear,
//! the order does not affect the final result for DC/steady-state signals,
//! but we match the binary's order for consistency:
//!
//! ```text
//! raw i16 -> BiquadFilter::process_block_i16() -> filtered i16
//!         -> VoltageTransform::apply() -> calibrated f64 volts
//! ```

use saleae_proto::biquad::BiquadFilter;
use saleae_proto::calibration::{CalibrationData, VoltageTransform};

use crate::disaggregator::DisaggregatorOutput;

/// Result of applying calibration to disaggregated analog data.
///
/// Each channel gets a `Vec<f64>` of calibrated voltage samples.
/// The channel order matches the `DisaggregatorOutput::analog` order
/// (i.e., ordered by enabled channel index).
#[derive(Debug, Clone)]
pub struct CalibratedOutput {
    /// Per-channel calibrated voltage samples.
    pub channels: Vec<Vec<f64>>,
    /// Channel indices corresponding to each entry in `channels`.
    /// These are the original analog channel indices from the capture config.
    pub channel_indices: Vec<u8>,
}

impl CalibratedOutput {
    /// Print a summary of calibrated voltage statistics per channel.
    pub fn print_summary(&self) {
        for (i, voltages) in self.channels.iter().enumerate() {
            let ch = self.channel_indices[i];
            if voltages.is_empty() {
                println!("  CH{ch}: no samples");
                continue;
            }
            let min = voltages.iter().cloned().fold(f64::INFINITY, f64::min);
            let max = voltages.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            let mean = voltages.iter().sum::<f64>() / voltages.len() as f64;
            println!(
                "  CH{ch}: {} samples, min={min:.4}V max={max:.4}V mean={mean:.4}V",
                voltages.len(),
            );
        }
    }
}

/// Apply calibration to disaggregated analog data.
///
/// For each enabled analog channel:
/// 1. Look up the biquad filter coefficients for (channel, sample_rate)
/// 2. Look up the voltage range for the channel
/// 3. Apply the biquad filter to the raw i16 ADC codes (binary-compatible order)
/// 4. Convert filtered i16 codes to calibrated voltages
///
/// # Arguments
///
/// - `output`: Disaggregated data from the EP2 stream
/// - `cal`: Parsed calibration data from the .cal file
/// - `analog_channel_indices`: The actual channel numbers (0-based) for each
///   entry in `output.analog`, in order
/// - `sample_rate`: Capture sample rate in Hz, used to look up the correct
///   calibration coefficients
///
/// # Returns
///
/// `Some(CalibratedOutput)` if calibration data was available for all channels,
/// or `None` if any channel was missing calibration data.
pub fn apply_calibration(
    output: &DisaggregatorOutput,
    cal: &CalibrationData,
    analog_channel_indices: &[u8],
    sample_rate: u64,
) -> Option<CalibratedOutput> {
    if output.analog.is_empty() {
        return Some(CalibratedOutput {
            channels: vec![],
            channel_indices: vec![],
        });
    }

    // Find the closest matching sample rate in the calibration data.
    // The .cal file has specific rates (50M, 12.5M, 6.25M, 3.125M, 1.5625M, 781.25k).
    // Use exact match first, then fall back to closest.
    let effective_rate = find_closest_cal_rate(cal, analog_channel_indices[0], sample_rate)?;

    let mut channels = Vec::with_capacity(output.analog.len());
    let mut indices = Vec::with_capacity(output.analog.len());

    for (i, raw_samples) in output.analog.iter().enumerate() {
        let ch = analog_channel_indices[i];

        let cal_set = cal.find_set(ch, effective_rate)?;
        let vr = cal.find_voltage_range(ch)?;
        let transform = VoltageTransform::from_range(vr.min_voltage, vr.max_voltage, -2048, 2047);

        // Step 1: Filter raw i16 codes (binary-compatible order).
        let mut filtered = raw_samples.clone();
        let mut filter = BiquadFilter::new(&cal_set.sections);
        filter.process_block_i16(&mut filtered);

        // Step 2: Convert filtered i16 codes to voltages.
        let voltages: Vec<f64> = filtered.iter().map(|&code| transform.apply(code)).collect();

        channels.push(voltages);
        indices.push(ch);
    }

    Some(CalibratedOutput {
        channels,
        channel_indices: indices,
    })
}

/// Find the closest calibration sample rate for a given channel.
///
/// Returns `None` if the channel has no calibration data at all.
fn find_closest_cal_rate(cal: &CalibrationData, channel: u8, target_rate: u64) -> Option<u64> {
    // Try exact match first.
    if cal.find_set(channel, target_rate).is_some() {
        return Some(target_rate);
    }

    // Collect all available rates for this channel.
    let rates: Vec<u64> = cal
        .calibration_sets
        .iter()
        .filter(|s| s.channel == channel)
        .map(|s| s.sample_rate)
        .collect();

    if rates.is_empty() {
        return None;
    }

    // Find closest by absolute difference.
    let closest = rates
        .iter()
        .min_by_key(|&&r| {
            if r >= target_rate {
                r - target_rate
            } else {
                target_rate - r
            }
        })
        .copied()?;

    tracing::warn!(
        target_rate,
        closest_rate = closest,
        "no exact calibration match for sample rate, using closest"
    );

    Some(closest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::disaggregator::DisaggregatorOutput;
    use saleae_proto::calibration;
    use std::path::PathBuf;

    fn reference_cal_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("re")
            .join("calibration-764051b51bd4d9f1.cal")
    }

    fn load_reference_cal() -> CalibrationData {
        let data = std::fs::read(reference_cal_path()).expect("read cal file");
        calibration::parse_calibration(&data).expect("parse cal")
    }

    #[test]
    fn calibrate_empty_analog() {
        let cal = load_reference_cal();
        let output = DisaggregatorOutput {
            digital: vec![],
            analog: vec![],
        };
        let result = apply_calibration(&output, &cal, &[], 12_500_000);
        assert!(result.is_some());
        assert!(result.unwrap().channels.is_empty());
    }

    #[test]
    fn calibrate_single_channel_dc_zero() {
        let cal = load_reference_cal();

        // 1000 samples of ADC code 0 on channel 0
        let output = DisaggregatorOutput {
            digital: vec![],
            analog: vec![vec![0i16; 1000]],
        };

        let result =
            apply_calibration(&output, &cal, &[0], 12_500_000).expect("calibration should succeed");

        assert_eq!(result.channels.len(), 1);
        assert_eq!(result.channel_indices, vec![0]);

        // After filter settles, DC=0 should map near 0V (midpoint).
        // The voltage transform maps code 0 to ~-0.098V for channel 0.
        let last = result.channels[0].last().unwrap();
        assert!(
            last.is_finite(),
            "calibrated voltage should be finite, got {}",
            last
        );
        assert!(last.abs() < 1.0, "DC=0 should be near 0V, got {:.4}V", last);
    }

    #[test]
    fn calibrate_two_channels_opposite_codes() {
        let cal = load_reference_cal();

        // Channel 0: constant +1024, Channel 1: constant -1024
        let output = DisaggregatorOutput {
            digital: vec![],
            analog: vec![vec![1024i16; 1000], vec![-1024i16; 1000]],
        };

        let result = apply_calibration(&output, &cal, &[0, 1], 12_500_000)
            .expect("calibration should succeed");

        assert_eq!(result.channels.len(), 2);
        assert_eq!(result.channel_indices, vec![0, 1]);

        // Channel 0 (+1024) should settle to positive voltage
        let v0 = *result.channels[0].last().unwrap();
        assert!(v0 > 0.0, "ch0 (+1024) should be positive, got {:.4}V", v0);

        // Channel 1 (-1024) should settle to negative voltage
        let v1 = *result.channels[1].last().unwrap();
        assert!(v1 < 0.0, "ch1 (-1024) should be negative, got {:.4}V", v1);
    }

    #[test]
    fn calibrate_all_eight_channels() {
        let cal = load_reference_cal();

        // All 8 channels with constant 0
        let output = DisaggregatorOutput {
            digital: vec![],
            analog: vec![vec![0i16; 500]; 8],
        };

        let indices: Vec<u8> = (0..8).collect();
        let result = apply_calibration(&output, &cal, &indices, 12_500_000)
            .expect("calibration should succeed");

        assert_eq!(result.channels.len(), 8);
        for (i, voltages) in result.channels.iter().enumerate() {
            assert_eq!(voltages.len(), 500);
            let last = voltages.last().unwrap();
            assert!(
                last.is_finite(),
                "ch{} should produce finite voltage, got {}",
                i,
                last
            );
        }
    }

    #[test]
    fn calibrate_exact_rate_match() {
        let cal = load_reference_cal();
        // 12.5 MHz is an exact match
        let rate = find_closest_cal_rate(&cal, 0, 12_500_000);
        assert_eq!(rate, Some(12_500_000));
    }

    #[test]
    fn calibrate_closest_rate_fallback() {
        let cal = load_reference_cal();
        // 10 MHz is not an exact match, should fall back to 12.5 MHz (closest)
        let rate = find_closest_cal_rate(&cal, 0, 10_000_000);
        assert_eq!(rate, Some(12_500_000));
    }

    #[test]
    fn calibrate_closest_rate_low() {
        let cal = load_reference_cal();
        // 500 kHz is below all rates, should pick 781250 (closest)
        let rate = find_closest_cal_rate(&cal, 0, 500_000);
        assert_eq!(rate, Some(781_250));
    }

    #[test]
    fn calibrate_missing_channel_returns_none() {
        let cal = load_reference_cal();
        // Channel 99 doesn't exist
        let output = DisaggregatorOutput {
            digital: vec![],
            analog: vec![vec![0i16; 10]],
        };
        let result = apply_calibration(&output, &cal, &[99], 12_500_000);
        assert!(result.is_none());
    }

    #[test]
    fn calibrate_voltage_range_reasonable() {
        let cal = load_reference_cal();

        // Full-scale positive ADC code through calibration
        let output = DisaggregatorOutput {
            digital: vec![],
            analog: vec![vec![2047i16; 1000]],
        };

        let result =
            apply_calibration(&output, &cal, &[0], 12_500_000).expect("calibration should succeed");

        let last = *result.channels[0].last().unwrap();
        // Should be near +10V (the max voltage for channel 0 is ~10.245V)
        assert!(
            last > 8.0 && last < 12.0,
            "full-scale positive should be near +10V, got {:.4}V",
            last
        );
    }
}
