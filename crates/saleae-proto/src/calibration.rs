//! Calibration data parser for Saleae Logic devices.
//!
//! Parses Boost.Serialization XML `.cal` files containing per-channel,
//! per-sample-rate IIR biquad filter coefficients and ADC-to-voltage
//! transform parameters.
//!
//! The calibration data compensates for analog front-end (AFE) distortion
//! using a cascade of 5 second-order sections (biquads) in Direct Form I.

use crate::error::{ProtoError, ProtoResult};
use quick_xml::events::Event;
use quick_xml::Reader;

/// Number of biquad sections per calibration set.
const NUM_SECTIONS: usize = 5;

/// Number of coefficients per section in the XML (b0, b1, b2, a0, a1, a2).
const COEFFS_PER_SECTION: usize = 6;

/// Total coefficient count per calibration set (5 * 6 = 30).
const TOTAL_COEFFS: usize = NUM_SECTIONS * COEFFS_PER_SECTION;

/// A single biquad section's coefficients.
///
/// Represents a second-order section (SOS) of the DF1 IIR filter.
/// a0 is always 1.0 (normalized) and is not stored.
#[derive(Debug, Clone)]
pub struct BiquadSection {
    pub b0: f64,
    pub b1: f64,
    pub b2: f64,
    // a0 is always 1.0, not stored
    pub a1: f64,
    pub a2: f64,
}

/// Per-channel, per-sample-rate calibration set.
///
/// Contains the IIR filter coefficients and metadata for one
/// (channel, sample_rate) combination.
#[derive(Debug, Clone)]
pub struct CalibrationSet {
    pub channel: u8,
    pub sample_rate: u64,
    pub sections: [BiquadSection; NUM_SECTIONS],
    pub group_delay: f64,
    pub impulse_length: u64,
}

/// Per-channel full-scale voltage range.
///
/// Defines the ADC-to-voltage mapping for a single channel.
#[derive(Debug, Clone)]
pub struct VoltageRange {
    pub channel: u8,
    pub min_voltage: f64,
    pub max_voltage: f64,
}

/// Linear ADC-to-voltage transform.
///
/// Converts raw ADC codes to calibrated voltages using:
/// `voltage = adc_code * scale + offset`
///
/// The transform is derived from the per-channel full-scale voltage
/// range and the device's ADC bit depth.
#[derive(Debug, Clone, Copy)]
pub struct VoltageTransform {
    /// Volts per ADC code.
    pub scale: f64,
    /// Voltage offset (volts at ADC code 0).
    pub offset: f64,
}

impl VoltageTransform {
    /// Compute the transform from a voltage range and ADC code range.
    ///
    /// For Logic Pro 8/16: `adc_min = -2048`, `adc_max = 2047` (12-bit signed).
    /// For Logic 4: `adc_min = 0`, `adc_max = 255` (8-bit unsigned).
    pub fn from_range(min_voltage: f64, max_voltage: f64, adc_min: i16, adc_max: i16) -> Self {
        let m = (max_voltage - min_voltage) / (adc_max as f64 - adc_min as f64);
        let b = min_voltage - adc_min as f64 * m;
        Self {
            scale: m,
            offset: b,
        }
    }

    /// Convert an ADC code to a voltage.
    #[inline]
    pub fn apply(&self, adc_code: i16) -> f64 {
        adc_code as f64 * self.scale + self.offset
    }
}

/// Complete calibration data for a device.
///
/// Contains all per-channel voltage ranges and per-(channel, sample_rate)
/// IIR filter coefficients parsed from a `.cal` file.
#[derive(Debug, Clone)]
pub struct CalibrationData {
    pub device_id: u64,
    pub device_type: u8,
    pub calibration_date: String,
    pub adc_gain: f64,
    pub voltage_ranges: Vec<VoltageRange>,
    pub calibration_sets: Vec<CalibrationSet>,
}

impl CalibrationData {
    /// Look up the calibration set for a given channel and sample rate.
    pub fn find_set(&self, channel: u8, sample_rate: u64) -> Option<&CalibrationSet> {
        self.calibration_sets
            .iter()
            .find(|s| s.channel == channel && s.sample_rate == sample_rate)
    }

    /// Look up the voltage range for a given channel.
    pub fn find_voltage_range(&self, channel: u8) -> Option<&VoltageRange> {
        self.voltage_ranges.iter().find(|v| v.channel == channel)
    }
}

/// Parse a `.cal` file (Boost.Serialization XML) into [`CalibrationData`].
///
/// The parser processes the XML element-by-element using quick-xml's
/// event-based reader. It handles the Boost.Serialization attributes
/// (class_id, tracking_level, etc.) by ignoring them and extracting
/// only the data elements.
pub fn parse_calibration(xml_data: &[u8]) -> ProtoResult<CalibrationData> {
    let mut reader = Reader::from_reader(xml_data);
    reader.config_mut().trim_text(true);

    let mut device_id: Option<u64> = None;
    let mut device_type: Option<u8> = None;
    let mut calibration_date: Option<String> = None;
    let mut adc_gain: Option<f64> = None;
    let mut calibration_sets: Vec<CalibrationSet> = Vec::new();
    let mut voltage_ranges: Vec<VoltageRange> = Vec::new();

    // Parsing state machine
    let mut buf = Vec::new();
    let mut tag_stack: Vec<String> = Vec::new();

    // Current item being parsed
    let mut in_calibration_data = false;
    let mut in_voltage_ranges = false;
    let mut in_item = false;

    // Per-item state (calibration set)
    let mut item_channel: Option<u8> = None;
    let mut item_sample_rate: Option<u64> = None;
    let mut item_coeffs: Vec<f64> = Vec::new();
    let mut item_group_delay: Option<f64> = None;
    let mut item_impulse_length: Option<u64> = None;
    let mut in_sos_coefficients = false;
    let mut in_fir_taps = false;

    // Per-item state (voltage range)
    let mut vr_channel: Option<u8> = None;
    let mut vr_min: Option<f64> = None;
    let mut vr_max: Option<f64> = None;

    // Track nesting depth for mDeviceId to distinguish the outer wrapper
    // from the per-item channel mDeviceId.
    let mut device_id_depth: usize = 0;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                tag_stack.push(name.clone());

                match name.as_str() {
                    "mCalibrationData" => {
                        in_calibration_data = true;
                        in_voltage_ranges = false;
                    }
                    "mFullScaleVoltageRanges" => {
                        in_calibration_data = false;
                        in_voltage_ranges = true;
                    }
                    "item" if in_calibration_data && !in_sos_coefficients && !in_fir_taps => {
                        in_item = true;
                        item_channel = None;
                        item_sample_rate = None;
                        item_coeffs = Vec::new();
                        item_group_delay = None;
                        item_impulse_length = None;
                    }
                    "item" if in_voltage_ranges && !in_sos_coefficients && !in_fir_taps => {
                        in_item = true;
                        vr_channel = None;
                        vr_min = None;
                        vr_max = None;
                    }
                    "mDf1SosCoefficients" => {
                        in_sos_coefficients = true;
                        in_fir_taps = false;
                    }
                    "mFirTaps" => {
                        in_fir_taps = true;
                    }
                    "mDeviceId" => {
                        device_id_depth += 1;
                    }
                    _ => {}
                }
            }
            Ok(Event::End(ref e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();

                match name.as_str() {
                    "mDf1SosCoefficients" => {
                        in_sos_coefficients = false;
                    }
                    "mFirTaps" => {
                        in_fir_taps = false;
                    }
                    "mDeviceId" => {
                        device_id_depth = device_id_depth.saturating_sub(1);
                    }
                    "item"
                        if in_item
                            && in_calibration_data
                            && !in_sos_coefficients
                            && !in_fir_taps =>
                    {
                        // Finalize a calibration set item
                        let channel = item_channel.ok_or_else(|| {
                            ProtoError::parse("calibration item missing mChannelIndex")
                        })?;
                        let sample_rate = item_sample_rate.ok_or_else(|| {
                            ProtoError::parse("calibration item missing mSampleRate")
                        })?;
                        let group_delay = item_group_delay.ok_or_else(|| {
                            ProtoError::parse("calibration item missing mGroupDelay")
                        })?;
                        let impulse_length = item_impulse_length.ok_or_else(|| {
                            ProtoError::parse("calibration item missing mImpulseLength")
                        })?;

                        if item_coeffs.len() != TOTAL_COEFFS {
                            return Err(ProtoError::parse(format!(
                                "expected {} coefficients, got {}",
                                TOTAL_COEFFS,
                                item_coeffs.len()
                            )));
                        }

                        let sections = parse_sections(&item_coeffs)?;
                        calibration_sets.push(CalibrationSet {
                            channel,
                            sample_rate,
                            sections,
                            group_delay,
                            impulse_length,
                        });
                        in_item = false;
                    }
                    "item" if in_item && in_voltage_ranges && !in_sos_coefficients && !in_fir_taps => {
                        let channel = vr_channel.ok_or_else(|| {
                            ProtoError::parse("voltage range item missing mChannelIndex")
                        })?;
                        let min_voltage = vr_min.ok_or_else(|| {
                            ProtoError::parse("voltage range item missing mFullScaleMinVoltage")
                        })?;
                        let max_voltage = vr_max.ok_or_else(|| {
                            ProtoError::parse("voltage range item missing mFullScaleMaxVoltage")
                        })?;
                        voltage_ranges.push(VoltageRange {
                            channel,
                            min_voltage,
                            max_voltage,
                        });
                        in_item = false;
                    }
                    "mCalibrationData" => {
                        in_calibration_data = false;
                    }
                    "mFullScaleVoltageRanges" => {
                        in_voltage_ranges = false;
                    }
                    _ => {}
                }

                if !tag_stack.is_empty() {
                    tag_stack.pop();
                }
            }
            Ok(Event::Text(ref e)) => {
                let text = e.unescape().map_err(|err| {
                    ProtoError::parse(format!("XML text unescape error: {}", err))
                })?;
                let text = text.trim();
                if text.is_empty() {
                    buf.clear();
                    continue;
                }

                let current_tag = tag_stack.last().map(|s| s.as_str()).unwrap_or("");

                match current_tag {
                    "mDeviceId" if device_id_depth == 1 && device_id.is_none() => {
                        // This is the outer device ID (in the mDeviceId wrapper element).
                        // The inner mDeviceId elements (within mChannel) have depth > 1.
                        device_id = Some(text.parse::<u64>().map_err(|e| {
                            ProtoError::parse(format!("invalid device_id '{}': {}", text, e))
                        })?);
                    }
                    "mDeviceType" if device_type.is_none() => {
                        device_type = Some(text.parse::<u8>().map_err(|e| {
                            ProtoError::parse(format!("invalid device_type '{}': {}", text, e))
                        })?);
                    }
                    "mCalibrationDate" if calibration_date.is_none() => {
                        calibration_date = Some(text.to_string());
                    }
                    "mAdcGain" if adc_gain.is_none() => {
                        adc_gain = Some(text.parse::<f64>().map_err(|e| {
                            ProtoError::parse(format!("invalid adc_gain '{}': {}", text, e))
                        })?);
                    }
                    "mChannelIndex" if in_item && in_calibration_data => {
                        item_channel = Some(text.parse::<u8>().map_err(|e| {
                            ProtoError::parse(format!("invalid channel index '{}': {}", text, e))
                        })?);
                    }
                    "mChannelIndex" if in_item && in_voltage_ranges => {
                        vr_channel = Some(text.parse::<u8>().map_err(|e| {
                            ProtoError::parse(format!("invalid channel index '{}': {}", text, e))
                        })?);
                    }
                    "mSampleRate" if in_item && in_calibration_data => {
                        item_sample_rate = Some(text.parse::<u64>().map_err(|e| {
                            ProtoError::parse(format!("invalid sample rate '{}': {}", text, e))
                        })?);
                    }
                    "item" if in_sos_coefficients && !in_fir_taps => {
                        let val = text.parse::<f64>().map_err(|e| {
                            ProtoError::parse(format!("invalid coefficient '{}': {}", text, e))
                        })?;
                        item_coeffs.push(val);
                    }
                    "mGroupDelay" if in_item && in_calibration_data => {
                        item_group_delay = Some(text.parse::<f64>().map_err(|e| {
                            ProtoError::parse(format!("invalid group delay '{}': {}", text, e))
                        })?);
                    }
                    "mImpulseLength" if in_item && in_calibration_data => {
                        item_impulse_length = Some(text.parse::<u64>().map_err(|e| {
                            ProtoError::parse(format!(
                                "invalid impulse length '{}': {}",
                                text, e
                            ))
                        })?);
                    }
                    "mFullScaleMinVoltage" if in_item && in_voltage_ranges => {
                        vr_min = Some(text.parse::<f64>().map_err(|e| {
                            ProtoError::parse(format!("invalid min voltage '{}': {}", text, e))
                        })?);
                    }
                    "mFullScaleMaxVoltage" if in_item && in_voltage_ranges => {
                        vr_max = Some(text.parse::<f64>().map_err(|e| {
                            ProtoError::parse(format!("invalid max voltage '{}': {}", text, e))
                        })?);
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {} // Ignore PI, comments, CData, etc.
            Err(e) => {
                return Err(ProtoError::parse(format!(
                    "XML parse error at position {}: {}",
                    reader.error_position(),
                    e
                )));
            }
        }
        buf.clear();
    }

    let device_id =
        device_id.ok_or_else(|| ProtoError::parse("missing mDeviceId in calibration file"))?;
    let device_type =
        device_type.ok_or_else(|| ProtoError::parse("missing mDeviceType in calibration file"))?;
    let calibration_date = calibration_date
        .ok_or_else(|| ProtoError::parse("missing mCalibrationDate in calibration file"))?;
    let adc_gain =
        adc_gain.ok_or_else(|| ProtoError::parse("missing mAdcGain in calibration file"))?;

    if calibration_sets.is_empty() {
        return Err(ProtoError::parse("no calibration sets found"));
    }
    if voltage_ranges.is_empty() {
        return Err(ProtoError::parse("no voltage ranges found"));
    }

    Ok(CalibrationData {
        device_id,
        device_type,
        calibration_date,
        adc_gain,
        voltage_ranges,
        calibration_sets,
    })
}

/// Parse 30 raw coefficient values into 5 biquad sections.
///
/// Layout per section: b0, b1, b2, a0, a1, a2
/// a0 must be 1.0 (normalized); it is verified and discarded.
fn parse_sections(coeffs: &[f64]) -> ProtoResult<[BiquadSection; NUM_SECTIONS]> {
    assert_eq!(coeffs.len(), TOTAL_COEFFS);

    let mut sections: [BiquadSection; NUM_SECTIONS] = std::array::from_fn(|_| BiquadSection {
        b0: 0.0,
        b1: 0.0,
        b2: 0.0,
        a1: 0.0,
        a2: 0.0,
    });

    for (i, section) in sections.iter_mut().enumerate() {
        let base = i * COEFFS_PER_SECTION;
        let b0 = coeffs[base];
        let b1 = coeffs[base + 1];
        let b2 = coeffs[base + 2];
        let a0 = coeffs[base + 3];
        let a1 = coeffs[base + 4];
        let a2 = coeffs[base + 5];

        // a0 must always be 1.0 (normalized SOS form).
        if (a0 - 1.0).abs() > 1e-10 {
            return Err(ProtoError::parse(format!(
                "section {} has a0 = {} (expected 1.0)",
                i, a0
            )));
        }

        *section = BiquadSection {
            b0,
            b1,
            b2,
            a1,
            a2,
        };
    }

    Ok(sections)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Path to the reference .cal file for testing.
    fn reference_cal_path() -> PathBuf {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        PathBuf::from(manifest_dir)
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("re")
            .join("calibration-764051b51bd4d9f1.cal")
    }

    fn load_reference_cal() -> CalibrationData {
        let path = reference_cal_path();
        let data = std::fs::read(&path).unwrap_or_else(|e| {
            panic!(
                "Failed to read reference .cal file at {}: {}",
                path.display(),
                e
            )
        });
        parse_calibration(&data).expect("Failed to parse reference .cal file")
    }

    #[test]
    fn parse_reference_device_id() {
        let cal = load_reference_cal();
        assert_eq!(cal.device_id, 8520900333282843121);
    }

    #[test]
    fn parse_reference_device_type() {
        let cal = load_reference_cal();
        assert_eq!(cal.device_type, 6); // LogicPro8
    }

    #[test]
    fn parse_reference_calibration_date() {
        let cal = load_reference_cal();
        assert_eq!(cal.calibration_date, "9/13/2019 3:48:10 AM");
    }

    #[test]
    fn parse_reference_adc_gain() {
        let cal = load_reference_cal();
        assert!((cal.adc_gain - 0.0).abs() < 1e-10);
    }

    #[test]
    fn parse_reference_calibration_set_count() {
        let cal = load_reference_cal();
        assert_eq!(cal.calibration_sets.len(), 48);
    }

    #[test]
    fn parse_reference_voltage_range_count() {
        let cal = load_reference_cal();
        assert_eq!(cal.voltage_ranges.len(), 8);
    }

    #[test]
    fn parse_reference_channel0_voltage_range() {
        let cal = load_reference_cal();
        let vr = cal.find_voltage_range(0).expect("channel 0 voltage range");
        // Values from doc/calibration/calibration-format.md
        assert!((vr.min_voltage - (-10.444522)).abs() < 1e-4);
        assert!((vr.max_voltage - 10.245802).abs() < 1e-4);
    }

    #[test]
    fn parse_reference_all_voltage_ranges() {
        let cal = load_reference_cal();
        // Expected values from doc/calibration/calibration-format.md
        let expected = [
            (0, -10.444522, 10.245802),
            (1, -10.424817, 10.313864),
            (2, -10.480572, 10.256573),
            (3, -10.432200, 10.372678),
            (4, -10.367207, 10.491253),
            (5, -10.324269, 10.317355),
            (6, -10.424414, 10.231961),
            (7, -10.427620, 10.405477),
        ];
        for (ch, exp_min, exp_max) in expected {
            let vr = cal
                .find_voltage_range(ch)
                .unwrap_or_else(|| panic!("missing voltage range for channel {}", ch));
            assert!(
                (vr.min_voltage - exp_min).abs() < 1e-3,
                "channel {} min: expected {}, got {}",
                ch,
                exp_min,
                vr.min_voltage
            );
            assert!(
                (vr.max_voltage - exp_max).abs() < 1e-3,
                "channel {} max: expected {}, got {}",
                ch,
                exp_max,
                vr.max_voltage
            );
        }
    }

    #[test]
    fn parse_reference_first_item_channel0_50mhz() {
        let cal = load_reference_cal();
        let set = cal
            .find_set(0, 50_000_000)
            .expect("channel 0 @ 50MHz calibration set");
        assert_eq!(set.channel, 0);
        assert_eq!(set.sample_rate, 50_000_000);
        assert!((set.group_delay - 3.0).abs() < 1e-10);
        assert_eq!(set.impulse_length, 3381);

        // Section 0 coefficients from reference file
        let s0 = &set.sections[0];
        assert!((s0.b0 - 1.1470743525205651).abs() < 1e-12);
        assert!((s0.b1 - (-1.1446541192031381)).abs() < 1e-12);
        assert!((s0.b2 - 0.0).abs() < 1e-12);
        assert!((s0.a1 - (-0.99757976668257287)).abs() < 1e-12);
        assert!((s0.a2 - 0.0).abs() < 1e-12);
    }

    #[test]
    fn section0_is_first_order() {
        // Section 0 always has b2=0 and a2=0 (first-order)
        let cal = load_reference_cal();
        for set in &cal.calibration_sets {
            let s0 = &set.sections[0];
            assert!(
                s0.b2.abs() < 1e-15,
                "channel {} @ {} Hz: section 0 b2 = {} (expected 0)",
                set.channel,
                set.sample_rate,
                s0.b2
            );
            assert!(
                s0.a2.abs() < 1e-15,
                "channel {} @ {} Hz: section 0 a2 = {} (expected 0)",
                set.channel,
                set.sample_rate,
                s0.a2
            );
        }
    }

    #[test]
    fn sections_1_4_shared_across_channels_at_same_rate() {
        let cal = load_reference_cal();
        // Group by sample rate
        let mut rates: Vec<u64> = cal
            .calibration_sets
            .iter()
            .map(|s| s.sample_rate)
            .collect();
        rates.sort();
        rates.dedup();

        for rate in rates {
            let sets: Vec<&CalibrationSet> =
                cal.calibration_sets.iter().filter(|s| s.sample_rate == rate).collect();
            if sets.len() < 2 {
                continue;
            }
            let reference = &sets[0].sections;
            for set in &sets[1..] {
                for section_idx in 1..NUM_SECTIONS {
                    let ref_s = &reference[section_idx];
                    let cur_s = &set.sections[section_idx];
                    assert!(
                        (ref_s.b0 - cur_s.b0).abs() < 1e-15
                            && (ref_s.b1 - cur_s.b1).abs() < 1e-15
                            && (ref_s.b2 - cur_s.b2).abs() < 1e-15
                            && (ref_s.a1 - cur_s.a1).abs() < 1e-15
                            && (ref_s.a2 - cur_s.a2).abs() < 1e-15,
                        "sections 1-4 differ between channel {} and channel {} at rate {}",
                        sets[0].channel,
                        set.channel,
                        rate
                    );
                }
            }
        }
    }

    #[test]
    fn all_a0_are_one() {
        // This is verified during parsing via parse_sections, but double-check
        // by verifying the parse succeeded on all 48 items.
        let cal = load_reference_cal();
        assert_eq!(cal.calibration_sets.len(), 48);
    }

    #[test]
    fn channels_and_rates_coverage() {
        let cal = load_reference_cal();
        // 8 channels, 6 sample rates each = 48
        let mut seen = std::collections::HashSet::new();
        for set in &cal.calibration_sets {
            seen.insert((set.channel, set.sample_rate));
        }
        assert_eq!(seen.len(), 48);

        // Verify all 8 channels present
        let channels: std::collections::HashSet<u8> =
            cal.calibration_sets.iter().map(|s| s.channel).collect();
        assert_eq!(channels.len(), 8);
        for ch in 0..8u8 {
            assert!(channels.contains(&ch), "missing channel {}", ch);
        }

        // Verify 6 sample rates
        let rates: std::collections::HashSet<u64> =
            cal.calibration_sets.iter().map(|s| s.sample_rate).collect();
        assert_eq!(rates.len(), 6);
        let expected_rates = [50_000_000, 12_500_000, 6_250_000, 3_125_000, 1_562_500, 781_250];
        for r in expected_rates {
            assert!(rates.contains(&r), "missing sample rate {}", r);
        }
    }

    // -- VoltageTransform tests --

    #[test]
    fn voltage_transform_channel0_midpoint() {
        let cal = load_reference_cal();
        let vr = cal.find_voltage_range(0).unwrap();
        let tx = VoltageTransform::from_range(vr.min_voltage, vr.max_voltage, -2048, 2047);

        // ADC code 0 should be near the midpoint of the voltage range.
        // Midpoint = (min + max) / 2, but the offset is min - (-2048) * scale.
        // For symmetric ADC range, code 0 maps to approximately (min + max) / 2.
        let v0 = tx.apply(0);
        let midpoint = (vr.min_voltage + vr.max_voltage) / 2.0;
        // The ADC range is -2048..2047 (not symmetric: 4095 codes), so code 0
        // is slightly above true midpoint. Allow ~0.01V tolerance.
        assert!(
            (v0 - midpoint).abs() < 0.01,
            "code 0 -> {} V, expected ~{} V",
            v0,
            midpoint
        );
    }

    #[test]
    fn voltage_transform_channel0_max() {
        let cal = load_reference_cal();
        let vr = cal.find_voltage_range(0).unwrap();
        let tx = VoltageTransform::from_range(vr.min_voltage, vr.max_voltage, -2048, 2047);

        let v_max = tx.apply(2047);
        assert!(
            (v_max - vr.max_voltage).abs() < 0.01,
            "code 2047 -> {} V, expected ~{} V",
            v_max,
            vr.max_voltage
        );
    }

    #[test]
    fn voltage_transform_channel0_min() {
        let cal = load_reference_cal();
        let vr = cal.find_voltage_range(0).unwrap();
        let tx = VoltageTransform::from_range(vr.min_voltage, vr.max_voltage, -2048, 2047);

        let v_min = tx.apply(-2048);
        assert!(
            (v_min - vr.min_voltage).abs() < 0.01,
            "code -2048 -> {} V, expected ~{} V",
            v_min,
            vr.min_voltage
        );
    }

    #[test]
    fn voltage_transform_scale_positive() {
        // Scale should always be positive (higher ADC code = higher voltage)
        let cal = load_reference_cal();
        for vr in &cal.voltage_ranges {
            let tx = VoltageTransform::from_range(vr.min_voltage, vr.max_voltage, -2048, 2047);
            assert!(
                tx.scale > 0.0,
                "channel {} has negative scale {}",
                vr.channel,
                tx.scale
            );
        }
    }

    #[test]
    fn voltage_transform_logic4_unsigned() {
        // Logic 4: 8-bit unsigned ADC, 0-5V range
        let tx = VoltageTransform::from_range(0.0, 5.0, 0, 255);
        assert!((tx.apply(0) - 0.0).abs() < 1e-10, "code 0 = {} V", tx.apply(0));
        assert!((tx.apply(255) - 5.0).abs() < 1e-10, "code 255 = {} V", tx.apply(255));
        assert!(
            (tx.apply(128) - 2.5098).abs() < 0.01,
            "code 128 = {} V",
            tx.apply(128)
        );
    }

    #[test]
    fn parse_minimal_xml() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes" ?>
<!DOCTYPE boost_serialization>
<boost_serialization signature="serialization::archive" version="10">
<DeviceCalibrationSet class_id="0" tracking_level="0" version="1">
  <px class_id="1" tracking_level="1" version="21" object_id="_0">
    <mDeviceId class_id="2" tracking_level="0" version="21">
      <mDeviceId>12345</mDeviceId>
      <mDeviceType>6</mDeviceType>
    </mDeviceId>
    <mCalibrationDate>1/1/2020</mCalibrationDate>
    <mCalibrationData class_id="3" tracking_level="0" version="0">
      <count>1</count>
      <item_version>21</item_version>
      <item class_id="4" tracking_level="0" version="21">
        <mChannel class_id="5" tracking_level="0" version="21">
          <mDeviceId>12345</mDeviceId>
          <mChannelIndex>0</mChannelIndex>
          <mDataType>0</mDataType>
        </mChannel>
        <mSampleRate>1000000</mSampleRate>
        <mDf1SosCoefficients>
          <count>30</count>
          <item_version>0</item_version>
          <item>1.0</item><item>0.0</item><item>0.0</item><item>1</item><item>0.0</item><item>0.0</item>
          <item>1.0</item><item>0.0</item><item>0.0</item><item>1</item><item>0.0</item><item>0.0</item>
          <item>1.0</item><item>0.0</item><item>0.0</item><item>1</item><item>0.0</item><item>0.0</item>
          <item>1.0</item><item>0.0</item><item>0.0</item><item>1</item><item>0.0</item><item>0.0</item>
          <item>1.0</item><item>0.0</item><item>0.0</item><item>1</item><item>0.0</item><item>0.0</item>
        </mDf1SosCoefficients>
        <mFirTaps>
          <count>30</count>
          <item_version>0</item_version>
          <item>1.0</item><item>0.0</item><item>0.0</item><item>1</item><item>0.0</item><item>0.0</item>
          <item>1.0</item><item>0.0</item><item>0.0</item><item>1</item><item>0.0</item><item>0.0</item>
          <item>1.0</item><item>0.0</item><item>0.0</item><item>1</item><item>0.0</item><item>0.0</item>
          <item>1.0</item><item>0.0</item><item>0.0</item><item>1</item><item>0.0</item><item>0.0</item>
          <item>1.0</item><item>0.0</item><item>0.0</item><item>1</item><item>0.0</item><item>0.0</item>
        </mFirTaps>
        <mGroupDelay>0</mGroupDelay>
        <mImpulseLength>1</mImpulseLength>
      </item>
    </mCalibrationData>
    <mFullScaleVoltageRanges class_id="7" tracking_level="0" version="0">
      <count>1</count>
      <item_version>21</item_version>
      <item class_id="8" tracking_level="0" version="21">
        <mChannel>
          <mDeviceId>12345</mDeviceId>
          <mChannelIndex>0</mChannelIndex>
          <mDataType>0</mDataType>
        </mChannel>
        <mFullScaleMinVoltage>-10.0</mFullScaleMinVoltage>
        <mFullScaleMaxVoltage>10.0</mFullScaleMaxVoltage>
      </item>
    </mFullScaleVoltageRanges>
    <mAdcGain>0</mAdcGain>
  </px>
</DeviceCalibrationSet>
</boost_serialization>"#;

        let cal = parse_calibration(xml.as_bytes()).expect("parse minimal XML");
        assert_eq!(cal.device_id, 12345);
        assert_eq!(cal.device_type, 6);
        assert_eq!(cal.calibration_sets.len(), 1);
        assert_eq!(cal.voltage_ranges.len(), 1);

        let set = &cal.calibration_sets[0];
        // All-unity filter: each section b0=1, rest=0
        for section in &set.sections {
            assert!((section.b0 - 1.0).abs() < 1e-15);
            assert!(section.b1.abs() < 1e-15);
            assert!(section.b2.abs() < 1e-15);
            assert!(section.a1.abs() < 1e-15);
            assert!(section.a2.abs() < 1e-15);
        }
    }
}
