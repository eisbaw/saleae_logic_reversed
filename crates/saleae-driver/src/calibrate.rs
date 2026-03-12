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
use crate::error::{DriverError, DriverResult};

/// Per-channel calibrated voltage data.
#[derive(Debug, Clone)]
pub struct CalibratedChannel {
    /// The device channel index (0-based).
    pub index: u8,
    /// Calibrated voltage samples.
    pub voltages: Vec<f64>,
}

impl CalibratedChannel {
    /// Compute min, max, mean statistics. Returns None if empty.
    pub fn stats(&self) -> Option<(f64, f64, f64)> {
        if self.voltages.is_empty() {
            return None;
        }
        let min = self.voltages.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = self
            .voltages
            .iter()
            .cloned()
            .fold(f64::NEG_INFINITY, f64::max);
        let mean = self.voltages.iter().sum::<f64>() / self.voltages.len() as f64;
        Some((min, max, mean))
    }
}

/// Result of applying calibration to disaggregated analog data.
#[derive(Debug, Clone)]
pub struct CalibratedOutput {
    /// Per-channel calibrated data, in order of enabled channels.
    pub channels: Vec<CalibratedChannel>,
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
///   entry in `output.analog`, in order. Must have the same length as
///   `output.analog`.
/// - `sample_rate`: Capture sample rate in Hz, used to look up the correct
///   calibration coefficients
/// - `adc_min`, `adc_max`: ADC code range for voltage transform.
///   Logic Pro 8/16: (-2048, 2047). Logic 4: (0, 255).
pub fn apply_calibration(
    output: &DisaggregatorOutput,
    cal: &CalibrationData,
    analog_channel_indices: &[u8],
    sample_rate: u64,
    adc_min: i16,
    adc_max: i16,
) -> DriverResult<CalibratedOutput> {
    if analog_channel_indices.len() != output.analog.len() {
        return Err(DriverError::InvalidCaptureConfig {
            reason: format!(
                "analog_channel_indices length ({}) != output.analog length ({})",
                analog_channel_indices.len(),
                output.analog.len()
            ),
        });
    }

    if output.analog.is_empty() {
        return Ok(CalibratedOutput { channels: vec![] });
    }

    // Find the closest matching sample rate in the calibration data.
    let effective_rate = find_closest_cal_rate(cal, analog_channel_indices[0], sample_rate)
        .ok_or_else(|| DriverError::InvalidCaptureConfig {
            reason: format!(
                "no calibration data for channel {} at any sample rate",
                analog_channel_indices[0]
            ),
        })?;

    let mut channels = Vec::with_capacity(output.analog.len());

    for (i, raw_samples) in output.analog.iter().enumerate() {
        let ch = analog_channel_indices[i];

        let cal_set =
            cal.find_set(ch, effective_rate)
                .ok_or_else(|| DriverError::InvalidCaptureConfig {
                    reason: format!("no calibration set for channel {ch} at rate {effective_rate}"),
                })?;
        let vr = cal
            .find_voltage_range(ch)
            .ok_or_else(|| DriverError::InvalidCaptureConfig {
                reason: format!("no voltage range for channel {ch}"),
            })?;
        let transform =
            VoltageTransform::from_range(vr.min_voltage, vr.max_voltage, adc_min, adc_max);

        // Step 1: Filter raw i16 codes (binary-compatible order).
        let mut filtered = raw_samples.clone();
        let mut filter = BiquadFilter::new(&cal_set.sections);
        filter.process_block_i16(&mut filtered);

        // Step 2: Convert filtered i16 codes to voltages.
        let voltages: Vec<f64> = filtered.iter().map(|&code| transform.apply(code)).collect();

        channels.push(CalibratedChannel {
            index: ch,
            voltages,
        });
    }

    Ok(CalibratedOutput { channels })
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

    /// Logic Pro 8/16: 12-bit signed ADC range.
    const PRO_ADC_MIN: i16 = -2048;
    const PRO_ADC_MAX: i16 = 2047;

    #[test]
    fn calibrate_empty_analog() {
        let cal = load_reference_cal();
        let output = DisaggregatorOutput {
            digital: vec![],
            analog: vec![],
        };
        let result = apply_calibration(&output, &cal, &[], 12_500_000, PRO_ADC_MIN, PRO_ADC_MAX);
        assert!(result.is_ok());
        assert!(result.unwrap().channels.is_empty());
    }

    #[test]
    fn calibrate_single_channel_dc_zero() {
        let cal = load_reference_cal();

        let output = DisaggregatorOutput {
            digital: vec![],
            analog: vec![vec![0i16; 1000]],
        };

        let result = apply_calibration(&output, &cal, &[0], 12_500_000, PRO_ADC_MIN, PRO_ADC_MAX)
            .expect("calibration should succeed");

        assert_eq!(result.channels.len(), 1);
        assert_eq!(result.channels[0].index, 0);

        let last = result.channels[0].voltages.last().unwrap();
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

        let output = DisaggregatorOutput {
            digital: vec![],
            analog: vec![vec![1024i16; 1000], vec![-1024i16; 1000]],
        };

        let result =
            apply_calibration(&output, &cal, &[0, 1], 12_500_000, PRO_ADC_MIN, PRO_ADC_MAX)
                .expect("calibration should succeed");

        assert_eq!(result.channels.len(), 2);
        assert_eq!(result.channels[0].index, 0);
        assert_eq!(result.channels[1].index, 1);

        let v0 = *result.channels[0].voltages.last().unwrap();
        assert!(v0 > 0.0, "ch0 (+1024) should be positive, got {:.4}V", v0);

        let v1 = *result.channels[1].voltages.last().unwrap();
        assert!(v1 < 0.0, "ch1 (-1024) should be negative, got {:.4}V", v1);
    }

    #[test]
    fn calibrate_all_eight_channels() {
        let cal = load_reference_cal();

        let output = DisaggregatorOutput {
            digital: vec![],
            analog: vec![vec![0i16; 500]; 8],
        };

        let indices: Vec<u8> = (0..8).collect();
        let result = apply_calibration(
            &output,
            &cal,
            &indices,
            12_500_000,
            PRO_ADC_MIN,
            PRO_ADC_MAX,
        )
        .expect("calibration should succeed");

        assert_eq!(result.channels.len(), 8);
        for ch in &result.channels {
            assert_eq!(ch.voltages.len(), 500);
            let last = ch.voltages.last().unwrap();
            assert!(
                last.is_finite(),
                "ch{} should produce finite voltage, got {}",
                ch.index,
                last
            );
        }
    }

    #[test]
    fn calibrate_exact_rate_match() {
        let cal = load_reference_cal();
        let rate = find_closest_cal_rate(&cal, 0, 12_500_000);
        assert_eq!(rate, Some(12_500_000));
    }

    #[test]
    fn calibrate_closest_rate_fallback() {
        let cal = load_reference_cal();
        let rate = find_closest_cal_rate(&cal, 0, 10_000_000);
        assert_eq!(rate, Some(12_500_000));
    }

    #[test]
    fn calibrate_closest_rate_low() {
        let cal = load_reference_cal();
        let rate = find_closest_cal_rate(&cal, 0, 500_000);
        assert_eq!(rate, Some(781_250));
    }

    #[test]
    fn calibrate_missing_channel_returns_error() {
        let cal = load_reference_cal();
        let output = DisaggregatorOutput {
            digital: vec![],
            analog: vec![vec![0i16; 10]],
        };
        let result = apply_calibration(&output, &cal, &[99], 12_500_000, PRO_ADC_MIN, PRO_ADC_MAX);
        assert!(result.is_err());
    }

    #[test]
    fn calibrate_mismatched_indices_length() {
        let cal = load_reference_cal();
        let output = DisaggregatorOutput {
            digital: vec![],
            analog: vec![vec![0i16; 10], vec![0i16; 10]],
        };
        // 1 index for 2 channels = error
        let result = apply_calibration(&output, &cal, &[0], 12_500_000, PRO_ADC_MIN, PRO_ADC_MAX);
        assert!(result.is_err());
    }

    #[test]
    fn calibrate_voltage_range_reasonable() {
        let cal = load_reference_cal();

        let output = DisaggregatorOutput {
            digital: vec![],
            analog: vec![vec![2047i16; 1000]],
        };

        let result = apply_calibration(&output, &cal, &[0], 12_500_000, PRO_ADC_MIN, PRO_ADC_MAX)
            .expect("calibration should succeed");

        let last = *result.channels[0].voltages.last().unwrap();
        assert!(
            last > 8.0 && last < 12.0,
            "full-scale positive should be near +10V, got {:.4}V",
            last
        );
    }

    #[test]
    fn calibrated_channel_stats() {
        let ch = CalibratedChannel {
            index: 0,
            voltages: vec![1.0, 2.0, 3.0, -1.0],
        };
        let (min, max, mean) = ch.stats().unwrap();
        assert!((min - (-1.0)).abs() < 1e-10);
        assert!((max - 3.0).abs() < 1e-10);
        assert!((mean - 1.25).abs() < 1e-10);
    }
}
