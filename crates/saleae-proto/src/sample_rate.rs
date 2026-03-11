//! Sample rate options and validation for Saleae Logic analyzers.
//!
//! Derived from reverse engineering of `libgraph_server_shared.so` v2.4.40.
//! See `re/notes/sample-rate-options.md` for struct layout, per-device rate
//! arrays, and performance tier documentation.
//!
//! # Architecture
//!
//! The Saleae firmware uses a three-tier performance system:
//!
//! - **DigitalOnly**: Legacy devices (Logic, Logic16) -- digital channels only.
//! - **AnalogBasic**: Logic 4, Logic 8 -- pre-computed rate option lists.
//! - **AnalogAdvanced**: Logic Pro 8, Logic Pro 16 -- bandwidth-allocated
//!   rate groups with 5 tiers (100%, 80%, 60%, 40%, 20% of max USB bandwidth).
//!
//! Source: `DeviceSettings.cpp`, `LogicStudentDevice.cpp`,
//! `LogicGraduateDevice.cpp`, `LogicProfessionalDevice.cpp`.

use crate::device::DeviceType;
use crate::error::{ProtoError, ProtoResult};

// ---------------------------------------------------------------------------
// Performance tier
// ---------------------------------------------------------------------------

/// Performance tier that determines how sample rate options are computed.
///
/// From `GetPerformanceInterfaceType()` at ELF VA 0x1c49e40, which uses a
/// bitmask-indexed lookup table to map device types to tiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PerformanceTier {
    /// Digital channels only. Used by legacy devices (Logic, Logic16) and
    /// any device with no analog channels enabled.
    DigitalOnly,

    /// Basic analog support with pre-computed rate options.
    /// Used by Logic 4 (type 3), Logic 8 (type 4).
    AnalogBasic,

    /// Advanced analog with bandwidth-allocated rate groups (5 tiers).
    /// Used by Logic Pro 8 (type 5), Logic Pro 16 (type 6/7).
    AnalogAdvanced,
}

impl std::fmt::Display for PerformanceTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PerformanceTier::DigitalOnly => f.write_str("DigitalOnly"),
            PerformanceTier::AnalogBasic => f.write_str("AnalogBasic"),
            PerformanceTier::AnalogAdvanced => f.write_str("AnalogAdvanced"),
        }
    }
}

// ---------------------------------------------------------------------------
// SampleRateOption
// ---------------------------------------------------------------------------

/// A valid sample rate configuration for a device.
///
/// Maps to the 24-byte `SampleRateOption` struct in the binary (see
/// `re/notes/sample-rate-options.md` for field layout). We simplify the
/// representation here: the binary has separate `hasDigital`/`hasAnalog`
/// booleans and separate physical/output analog rates, but for the Rust
/// driver we use 0 to indicate "not applicable" and track the tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SampleRateOption {
    /// Digital sample rate in Hz. 0 if this option is analog-only.
    pub digital_rate: u64,

    /// Analog sample rate in Hz (output rate after decimation).
    /// 0 if this option is digital-only.
    pub analog_rate: u64,

    /// Which performance tier generated this option.
    pub tier: PerformanceTier,
}

impl SampleRateOption {
    /// Create a digital-only rate option.
    pub const fn digital_only(rate: u64) -> Self {
        Self {
            digital_rate: rate,
            analog_rate: 0,
            tier: PerformanceTier::DigitalOnly,
        }
    }

    /// Create an analog-only rate option with the given tier.
    pub const fn analog_only(rate: u64, tier: PerformanceTier) -> Self {
        Self {
            digital_rate: 0,
            analog_rate: rate,
            tier,
        }
    }

    /// Create a combined digital+analog rate option.
    pub const fn combined(digital: u64, analog: u64, tier: PerformanceTier) -> Self {
        Self {
            digital_rate: digital,
            analog_rate: analog,
            tier,
        }
    }

    /// Whether this option includes a digital rate.
    pub const fn has_digital(&self) -> bool {
        self.digital_rate > 0
    }

    /// Whether this option includes an analog rate.
    pub const fn has_analog(&self) -> bool {
        self.analog_rate > 0
    }
}

impl std::fmt::Display for SampleRateOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (self.has_digital(), self.has_analog()) {
            (true, true) => write!(
                f,
                "digital {} Hz + analog {} Hz ({})",
                self.digital_rate, self.analog_rate, self.tier
            ),
            (true, false) => write!(f, "digital {} Hz ({})", self.digital_rate, self.tier),
            (false, true) => write!(f, "analog {} Hz ({})", self.analog_rate, self.tier),
            (false, false) => write!(f, "no rate ({})", self.tier),
        }
    }
}

// ---------------------------------------------------------------------------
// Per-device rate arrays (from .rodata extraction)
// ---------------------------------------------------------------------------

/// Digital sample rates for Logic Pro 8 / Logic Pro 16 (USB 3.0).
///
/// 15 entries from `.rodata` at ELF VA 0x471a28. Confirmed at three
/// locations in the binary (0x471a28, 0x472220, 0x4742b0).
///
/// Source: `LogicProfessionalDevice.cpp`
pub static PRO_DIGITAL_RATES: &[u64] = &[
    500_000_000,  // 500 MHz
    250_000_000,  // 250 MHz
    125_000_000,  // 125 MHz
    100_000_000,  // 100 MHz
    50_000_000,   // 50 MHz
    25_000_000,   // 25 MHz
    20_000_000,   // 20 MHz
    12_500_000,   // 12.5 MHz
    10_000_000,   // 10 MHz
    6_250_000,    // 6.25 MHz
    5_000_000,    // 5 MHz
    4_000_000,    // 4 MHz
    2_500_000,    // 2.5 MHz
    2_000_000,    // 2 MHz
    1_000_000,    // 1 MHz
];

/// Analog physical sample rates for Logic Pro 8 / Logic Pro 16 (USB 3.0).
///
/// 11 entries from `.rodata` at ELF VA 0x471a64. These are the ADC physical
/// rates before host-side decimation.
///
/// The second copy at 0x472260 omits 100 MHz (USB 2.0 bandwidth limit).
///
/// Source: `LogicProfessionalDevice.cpp`
pub static PRO_ANALOG_RATES: &[u64] = &[
    100_000_000,  // 100 MHz (USB 3.0 only)
    50_000_000,   // 50 MHz
    40_000_000,   // 40 MHz
    25_000_000,   // 25 MHz
    20_000_000,   // 20 MHz
    10_000_000,   // 10 MHz
    8_000_000,    // 8 MHz
    5_000_000,    // 5 MHz
    4_000_000,    // 4 MHz
    2_000_000,    // 2 MHz
    1_000_000,    // 1 MHz
];

/// Analog rates for Logic Pro at USB 2.0 (omits 100 MHz).
///
/// 10 entries from `.rodata` at ELF VA 0x472260.
pub static PRO_ANALOG_RATES_USB2: &[u64] = &[
    50_000_000,   // 50 MHz
    40_000_000,   // 40 MHz
    25_000_000,   // 25 MHz
    20_000_000,   // 20 MHz
    10_000_000,   // 10 MHz
    8_000_000,    // 8 MHz
    5_000_000,    // 5 MHz
    4_000_000,    // 4 MHz
    2_000_000,    // 2 MHz
    1_000_000,    // 1 MHz
];

/// Digital sample rates for Logic 8.
///
/// Runtime-populated from `.bss` globals (DAT_0307aa30/38). The rates must
/// be factors of 200 MHz (divisibility check against DAT_0044e3b8).
///
/// Source: `LogicGraduateDevice.cpp`, cross-referenced with JS bundle defaults.
pub static LOGIC8_DIGITAL_RATES: &[u64] = &[
    200_000_000,  // 200 MHz (USB 3.0 only, exceeds USB 2.0 max of 100 MHz)
    100_000_000,  // 100 MHz
    50_000_000,   // 50 MHz
    25_000_000,   // 25 MHz
    20_000_000,   // 20 MHz
    10_000_000,   // 10 MHz
    5_000_000,    // 5 MHz
    2_500_000,    // 2.5 MHz
    1_250_000,    // 1.25 MHz
];

/// Analog sample rates for Logic 8.
///
/// Runtime-populated from `.bss` globals (DAT_0307aa48/50). The rates must
/// be factors of 20 MHz (divisibility check against DAT_0044e278).
///
/// Source: `LogicGraduateDevice.cpp`, 12-bit ADC (AD9637/ADS528x).
pub static LOGIC8_ANALOG_RATES: &[u64] = &[
    20_000_000,   // 20 MHz
    10_000_000,   // 10 MHz
    5_000_000,    // 5 MHz
];

/// Logic 4 sample rate options.
///
/// 10 fixed entries from `LogicStudentDevice.cpp` at ELF VA 0x1c56310.
/// The Logic 4 has a fixed 6 MHz ADC and 12 MHz digital clock. All
/// analog options share the same 6 MHz physical rate; output rates are
/// achieved through host-side decimation.
///
/// Note: channel config is 4 channels (1 digital-capable + 3 analog-only
/// + digital-only variants). 8-bit ADC (HMCAD1100).
pub static LOGIC4_OPTIONS: &[SampleRateOption] = &[
    // Combined digital + analog options (6 entries)
    SampleRateOption::combined(12_000_000, 6_000_000, PerformanceTier::AnalogBasic),
    SampleRateOption::combined(12_000_000, 125_000, PerformanceTier::AnalogBasic),
    SampleRateOption::combined(12_000_000, 5_000, PerformanceTier::AnalogBasic),
    SampleRateOption::combined(12_000_000, 1_000, PerformanceTier::AnalogBasic),
    SampleRateOption::combined(12_000_000, 100, PerformanceTier::AnalogBasic),
    SampleRateOption::combined(12_000_000, 10, PerformanceTier::AnalogBasic),
    // Digital-only options (4 entries)
    SampleRateOption { digital_rate: 12_000_000, analog_rate: 0, tier: PerformanceTier::AnalogBasic },
    SampleRateOption { digital_rate: 6_000_000, analog_rate: 0, tier: PerformanceTier::AnalogBasic },
    SampleRateOption { digital_rate: 3_000_000, analog_rate: 0, tier: PerformanceTier::AnalogBasic },
    SampleRateOption { digital_rate: 1_000_000, analog_rate: 0, tier: PerformanceTier::AnalogBasic },
];

// ---------------------------------------------------------------------------
// Per-device rate queries
// ---------------------------------------------------------------------------

/// Return the performance tier for a device type.
///
/// From `GetPerformanceInterfaceType()` bitmask table:
/// - Logic, Logic16 -> DigitalOnly (legacy)
/// - Logic 4, Logic 8 -> AnalogBasic
/// - Logic Pro 8, Logic Pro 16 -> AnalogAdvanced
pub fn performance_tier(device_type: DeviceType) -> PerformanceTier {
    match device_type {
        DeviceType::LogicOriginal | DeviceType::Logic16 => PerformanceTier::DigitalOnly,
        DeviceType::Logic4 | DeviceType::Logic8 => PerformanceTier::AnalogBasic,
        DeviceType::LogicPro8 | DeviceType::LogicPro16 => PerformanceTier::AnalogAdvanced,
    }
}

/// Return the available digital sample rates for a device type.
///
/// Returns the raw rate array (sorted descending) for the given device.
/// Legacy devices return an empty slice (they are not supported by this
/// driver).
pub fn digital_rates(device_type: DeviceType) -> &'static [u64] {
    match device_type {
        DeviceType::LogicOriginal | DeviceType::Logic16 => &[],
        DeviceType::Logic4 => &[12_000_000, 6_000_000, 3_000_000, 1_000_000],
        DeviceType::Logic8 => LOGIC8_DIGITAL_RATES,
        DeviceType::LogicPro8 | DeviceType::LogicPro16 => PRO_DIGITAL_RATES,
    }
}

/// Return the available analog sample rates for a device type.
///
/// Returns the raw rate array (sorted descending). Logic 4 has a single
/// fixed ADC rate of 6 MHz (decimation happens host-side). Legacy devices
/// return an empty slice.
pub fn analog_rates(device_type: DeviceType) -> &'static [u64] {
    match device_type {
        DeviceType::LogicOriginal | DeviceType::Logic16 => &[],
        DeviceType::Logic4 => &[6_000_000],
        DeviceType::Logic8 => LOGIC8_ANALOG_RATES,
        DeviceType::LogicPro8 | DeviceType::LogicPro16 => PRO_ANALOG_RATES,
    }
}

/// Return all available sample rate options for a device type.
///
/// For Logic 4, this returns the fixed option table from the binary.
/// For Logic 8 and Pro devices, this builds options from the rate arrays
/// (digital-only + analog-only combinations). The full bandwidth-allocated
/// combinations for AnalogAdvanced require channel count and USB speed
/// context that this function does not have -- use the rate arrays directly
/// for that.
pub fn available_rates(device_type: DeviceType) -> Vec<SampleRateOption> {
    match device_type {
        DeviceType::LogicOriginal | DeviceType::Logic16 => Vec::new(),
        DeviceType::Logic4 => LOGIC4_OPTIONS.to_vec(),
        _ => {
            let tier = performance_tier(device_type);
            let drates = digital_rates(device_type);
            let arates = analog_rates(device_type);

            let mut options = Vec::with_capacity(drates.len() + arates.len());

            // Digital-only options
            for &rate in drates {
                options.push(SampleRateOption {
                    digital_rate: rate,
                    analog_rate: 0,
                    tier,
                });
            }

            // Analog-only options
            for &rate in arates {
                options.push(SampleRateOption {
                    digital_rate: 0,
                    analog_rate: rate,
                    tier,
                });
            }

            options
        }
    }
}

// ---------------------------------------------------------------------------
// Selection algorithm
// ---------------------------------------------------------------------------

/// Find the closest rate in a sorted (descending) array to the requested rate.
///
/// Implements the `SelectClosestIndex` algorithm from `ComputationUtilities.cpp`
/// (ELF VA 0x1c46ab0). The binary uses a std::map to sort by absolute
/// difference; we use a simpler linear scan since the arrays are small
/// (< 20 elements).
///
/// # Panics
///
/// Panics if `available` is empty (mirrors the binary's assertion:
/// "selections can not be empty").
pub fn select_closest_rate(available: &[u64], requested: u64) -> u64 {
    assert!(!available.is_empty(), "selections can not be empty");

    available
        .iter()
        .copied()
        .min_by_key(|&rate| rate.abs_diff(requested))
        .unwrap()
}

/// Find the index of the closest rate in a sorted array.
///
/// Same as [`select_closest_rate`] but returns the index.
///
/// # Panics
///
/// Panics if `available` is empty.
pub fn select_closest_index(available: &[u64], requested: u64) -> usize {
    assert!(!available.is_empty(), "selections can not be empty");

    available
        .iter()
        .enumerate()
        .min_by_key(|&(_, &rate)| rate.abs_diff(requested))
        .map(|(idx, _)| idx)
        .unwrap()
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/// Validate a digital/analog rate combination for a device type.
///
/// Checks that the requested rates exist in the device's available rate
/// tables. For Logic 4, validates against the fixed option table. For other
/// devices, validates digital and analog rates independently against their
/// respective rate arrays.
///
/// Returns the matching `SampleRateOption` on success, or a `ProtoError`
/// describing why the combination is invalid.
pub fn validate_rate(
    device_type: DeviceType,
    digital_rate: u64,
    analog_rate: u64,
) -> ProtoResult<SampleRateOption> {
    let tier = performance_tier(device_type);

    // Logic 4 has a fixed option table -- must match exactly.
    if device_type == DeviceType::Logic4 {
        for opt in LOGIC4_OPTIONS {
            if opt.digital_rate == digital_rate && opt.analog_rate == analog_rate {
                return Ok(*opt);
            }
        }
        return Err(ProtoError::InvalidSampleRate {
            reason: format!(
                "no Logic4 option matches digital={digital_rate} Hz, analog={analog_rate} Hz"
            ),
        });
    }

    // Legacy devices are not supported.
    if matches!(device_type, DeviceType::LogicOriginal | DeviceType::Logic16) {
        return Err(ProtoError::InvalidSampleRate {
            reason: format!(
                "{} is a legacy device, not supported by this driver",
                device_type
            ),
        });
    }

    // For Logic 8 and Pro devices, validate each rate independently.
    let drates = digital_rates(device_type);
    let arates = analog_rates(device_type);

    // At least one rate must be nonzero.
    if digital_rate == 0 && analog_rate == 0 {
        return Err(ProtoError::InvalidSampleRate {
            reason: "both digital and analog rates are zero".into(),
        });
    }

    // Validate digital rate if specified.
    if digital_rate > 0 && !drates.contains(&digital_rate) {
        return Err(ProtoError::InvalidSampleRate {
            reason: format!(
                "digital rate {digital_rate} Hz is not available for {device_type}; \
                 valid rates: {drates:?}"
            ),
        });
    }

    // Validate analog rate if specified.
    if analog_rate > 0 && !arates.contains(&analog_rate) {
        return Err(ProtoError::InvalidSampleRate {
            reason: format!(
                "analog rate {analog_rate} Hz is not available for {device_type}; \
                 valid rates: {arates:?}"
            ),
        });
    }

    Ok(SampleRateOption {
        digital_rate,
        analog_rate,
        tier,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Rate table tests
    // -----------------------------------------------------------------------

    #[test]
    fn pro_digital_has_15_rates() {
        assert_eq!(PRO_DIGITAL_RATES.len(), 15);
    }

    #[test]
    fn pro_digital_rates_descending() {
        for window in PRO_DIGITAL_RATES.windows(2) {
            assert!(
                window[0] > window[1],
                "rates not descending: {} <= {}",
                window[0],
                window[1]
            );
        }
    }

    #[test]
    fn pro_digital_rates_start_at_500mhz() {
        assert_eq!(PRO_DIGITAL_RATES[0], 500_000_000);
    }

    #[test]
    fn pro_digital_rates_end_at_1mhz() {
        assert_eq!(PRO_DIGITAL_RATES[14], 1_000_000);
    }

    #[test]
    fn pro_analog_has_11_rates() {
        assert_eq!(PRO_ANALOG_RATES.len(), 11);
    }

    #[test]
    fn pro_analog_rates_descending() {
        for window in PRO_ANALOG_RATES.windows(2) {
            assert!(
                window[0] > window[1],
                "rates not descending: {} <= {}",
                window[0],
                window[1]
            );
        }
    }

    #[test]
    fn pro_analog_usb2_omits_100mhz() {
        assert_eq!(PRO_ANALOG_RATES_USB2.len(), 10);
        assert_eq!(PRO_ANALOG_RATES_USB2[0], 50_000_000);
        assert!(!PRO_ANALOG_RATES_USB2.contains(&100_000_000));
    }

    #[test]
    fn logic8_digital_has_9_rates() {
        assert_eq!(LOGIC8_DIGITAL_RATES.len(), 9);
    }

    #[test]
    fn logic8_digital_max_200mhz() {
        assert_eq!(LOGIC8_DIGITAL_RATES[0], 200_000_000);
    }

    #[test]
    fn logic8_digital_rates_are_factors_of_200mhz() {
        for &rate in LOGIC8_DIGITAL_RATES {
            assert_eq!(
                200_000_000 % rate,
                0,
                "{rate} Hz is not a factor of 200 MHz"
            );
        }
    }

    #[test]
    fn logic8_analog_has_3_rates() {
        assert_eq!(LOGIC8_ANALOG_RATES.len(), 3);
    }

    #[test]
    fn logic8_analog_rates_are_factors_of_20mhz() {
        for &rate in LOGIC8_ANALOG_RATES {
            assert_eq!(
                20_000_000 % rate,
                0,
                "{rate} Hz is not a factor of 20 MHz"
            );
        }
    }

    #[test]
    fn logic4_has_10_options() {
        assert_eq!(LOGIC4_OPTIONS.len(), 10);
    }

    #[test]
    fn logic4_first_option_is_full_rate() {
        let opt = &LOGIC4_OPTIONS[0];
        assert_eq!(opt.digital_rate, 12_000_000);
        assert_eq!(opt.analog_rate, 6_000_000);
        assert_eq!(opt.tier, PerformanceTier::AnalogBasic);
    }

    #[test]
    fn logic4_last_4_are_digital_only() {
        for opt in &LOGIC4_OPTIONS[6..] {
            assert!(opt.has_digital());
            assert!(!opt.has_analog());
        }
    }

    // -----------------------------------------------------------------------
    // Performance tier tests
    // -----------------------------------------------------------------------

    #[test]
    fn performance_tier_legacy_is_digital_only() {
        assert_eq!(
            performance_tier(DeviceType::LogicOriginal),
            PerformanceTier::DigitalOnly
        );
        assert_eq!(
            performance_tier(DeviceType::Logic16),
            PerformanceTier::DigitalOnly
        );
    }

    #[test]
    fn performance_tier_basic_devices() {
        assert_eq!(
            performance_tier(DeviceType::Logic4),
            PerformanceTier::AnalogBasic
        );
        assert_eq!(
            performance_tier(DeviceType::Logic8),
            PerformanceTier::AnalogBasic
        );
    }

    #[test]
    fn performance_tier_pro_devices() {
        assert_eq!(
            performance_tier(DeviceType::LogicPro8),
            PerformanceTier::AnalogAdvanced
        );
        assert_eq!(
            performance_tier(DeviceType::LogicPro16),
            PerformanceTier::AnalogAdvanced
        );
    }

    // -----------------------------------------------------------------------
    // available_rates tests
    // -----------------------------------------------------------------------

    #[test]
    fn available_rates_pro8_has_digital_and_analog() {
        let rates = available_rates(DeviceType::LogicPro8);
        // 15 digital + 11 analog = 26
        assert_eq!(rates.len(), 26);

        let digital_count = rates.iter().filter(|r| r.has_digital() && !r.has_analog()).count();
        let analog_count = rates.iter().filter(|r| r.has_analog() && !r.has_digital()).count();
        assert_eq!(digital_count, 15);
        assert_eq!(analog_count, 11);
    }

    #[test]
    fn available_rates_logic4_matches_fixed_table() {
        let rates = available_rates(DeviceType::Logic4);
        assert_eq!(rates.len(), 10);
        assert_eq!(rates, LOGIC4_OPTIONS);
    }

    #[test]
    fn available_rates_legacy_is_empty() {
        assert!(available_rates(DeviceType::LogicOriginal).is_empty());
        assert!(available_rates(DeviceType::Logic16).is_empty());
    }

    // -----------------------------------------------------------------------
    // select_closest_rate tests
    // -----------------------------------------------------------------------

    #[test]
    fn select_closest_exact_match() {
        assert_eq!(
            select_closest_rate(PRO_DIGITAL_RATES, 100_000_000),
            100_000_000
        );
    }

    #[test]
    fn select_closest_between_values() {
        // 75 MHz is between 100 MHz and 50 MHz -- closer to 100 MHz
        assert_eq!(
            select_closest_rate(PRO_DIGITAL_RATES, 75_000_000),
            100_000_000
        );
    }

    #[test]
    fn select_closest_below_minimum() {
        // 500 kHz is below the minimum 1 MHz
        assert_eq!(
            select_closest_rate(PRO_DIGITAL_RATES, 500_000),
            1_000_000
        );
    }

    #[test]
    fn select_closest_above_maximum() {
        // 1 GHz is above the maximum 500 MHz
        assert_eq!(
            select_closest_rate(PRO_DIGITAL_RATES, 1_000_000_000),
            500_000_000
        );
    }

    #[test]
    fn select_closest_equidistant_picks_lower() {
        // 37.5 MHz is equidistant between 25 MHz and 50 MHz.
        // min_by_key picks the first minimum, which is 50 MHz (earlier in array).
        let result = select_closest_rate(PRO_DIGITAL_RATES, 37_500_000);
        assert!(result == 50_000_000 || result == 25_000_000);
    }

    #[test]
    fn select_closest_index_returns_correct_index() {
        let idx = select_closest_index(PRO_DIGITAL_RATES, 100_000_000);
        assert_eq!(idx, 3); // 100 MHz is at index 3
        assert_eq!(PRO_DIGITAL_RATES[idx], 100_000_000);
    }

    #[test]
    #[should_panic(expected = "selections can not be empty")]
    fn select_closest_rate_panics_on_empty() {
        select_closest_rate(&[], 100_000_000);
    }

    #[test]
    fn select_closest_single_element() {
        assert_eq!(select_closest_rate(&[42], 100), 42);
    }

    // -----------------------------------------------------------------------
    // validate_rate tests
    // -----------------------------------------------------------------------

    #[test]
    fn validate_rate_pro8_digital_only() {
        let opt = validate_rate(DeviceType::LogicPro8, 500_000_000, 0).unwrap();
        assert_eq!(opt.digital_rate, 500_000_000);
        assert_eq!(opt.analog_rate, 0);
        assert_eq!(opt.tier, PerformanceTier::AnalogAdvanced);
    }

    #[test]
    fn validate_rate_pro8_analog_only() {
        let opt = validate_rate(DeviceType::LogicPro8, 0, 50_000_000).unwrap();
        assert_eq!(opt.digital_rate, 0);
        assert_eq!(opt.analog_rate, 50_000_000);
    }

    #[test]
    fn validate_rate_pro8_combined() {
        let opt = validate_rate(DeviceType::LogicPro8, 100_000_000, 50_000_000).unwrap();
        assert_eq!(opt.digital_rate, 100_000_000);
        assert_eq!(opt.analog_rate, 50_000_000);
    }

    #[test]
    fn validate_rate_rejects_invalid_digital() {
        let err = validate_rate(DeviceType::LogicPro8, 999_999_999, 0).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("digital rate"), "got: {msg}");
        assert!(msg.contains("999999999"), "got: {msg}");
    }

    #[test]
    fn validate_rate_rejects_invalid_analog() {
        let err = validate_rate(DeviceType::LogicPro8, 0, 3_000_000).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("analog rate"), "got: {msg}");
    }

    #[test]
    fn validate_rate_rejects_both_zero() {
        let err = validate_rate(DeviceType::LogicPro8, 0, 0).unwrap_err();
        assert!(err.to_string().contains("both"), "{}", err);
    }

    #[test]
    fn validate_rate_logic4_valid_combo() {
        let opt = validate_rate(DeviceType::Logic4, 12_000_000, 6_000_000).unwrap();
        assert_eq!(opt.digital_rate, 12_000_000);
        assert_eq!(opt.analog_rate, 6_000_000);
    }

    #[test]
    fn validate_rate_logic4_digital_only() {
        let opt = validate_rate(DeviceType::Logic4, 3_000_000, 0).unwrap();
        assert_eq!(opt.digital_rate, 3_000_000);
        assert!(!opt.has_analog());
    }

    #[test]
    fn validate_rate_logic4_rejects_invalid() {
        let err = validate_rate(DeviceType::Logic4, 12_000_000, 999).unwrap_err();
        assert!(err.to_string().contains("Logic4"), "{}", err);
    }

    #[test]
    fn validate_rate_legacy_device_rejected() {
        let err = validate_rate(DeviceType::LogicOriginal, 1_000_000, 0).unwrap_err();
        assert!(err.to_string().contains("legacy"), "{}", err);
    }

    #[test]
    fn validate_rate_logic8_valid_digital() {
        let opt = validate_rate(DeviceType::Logic8, 100_000_000, 0).unwrap();
        assert_eq!(opt.digital_rate, 100_000_000);
        assert_eq!(opt.tier, PerformanceTier::AnalogBasic);
    }

    #[test]
    fn validate_rate_logic8_valid_analog() {
        let opt = validate_rate(DeviceType::Logic8, 0, 10_000_000).unwrap();
        assert_eq!(opt.analog_rate, 10_000_000);
    }

    #[test]
    fn validate_rate_logic8_rejects_invalid_digital() {
        // 200 MHz is valid, but 150 MHz is not in the array
        let err = validate_rate(DeviceType::Logic8, 150_000_000, 0).unwrap_err();
        assert!(err.to_string().contains("digital rate"), "{}", err);
    }

    // -----------------------------------------------------------------------
    // SampleRateOption method tests
    // -----------------------------------------------------------------------

    #[test]
    fn sample_rate_option_digital_only_ctor() {
        let opt = SampleRateOption::digital_only(100_000_000);
        assert!(opt.has_digital());
        assert!(!opt.has_analog());
        assert_eq!(opt.tier, PerformanceTier::DigitalOnly);
    }

    #[test]
    fn sample_rate_option_analog_only_ctor() {
        let opt = SampleRateOption::analog_only(50_000_000, PerformanceTier::AnalogAdvanced);
        assert!(!opt.has_digital());
        assert!(opt.has_analog());
    }

    #[test]
    fn sample_rate_option_combined_ctor() {
        let opt = SampleRateOption::combined(100_000_000, 50_000_000, PerformanceTier::AnalogAdvanced);
        assert!(opt.has_digital());
        assert!(opt.has_analog());
    }

    #[test]
    fn sample_rate_option_display() {
        let d = SampleRateOption::digital_only(100_000_000);
        assert!(d.to_string().contains("digital"));
        assert!(d.to_string().contains("100000000"));

        let a = SampleRateOption::analog_only(50_000_000, PerformanceTier::AnalogAdvanced);
        assert!(a.to_string().contains("analog"));

        let c = SampleRateOption::combined(100_000_000, 50_000_000, PerformanceTier::AnalogAdvanced);
        assert!(c.to_string().contains("digital"));
        assert!(c.to_string().contains("analog"));
    }

    // -----------------------------------------------------------------------
    // PerformanceTier display
    // -----------------------------------------------------------------------

    #[test]
    fn performance_tier_display() {
        assert_eq!(PerformanceTier::DigitalOnly.to_string(), "DigitalOnly");
        assert_eq!(PerformanceTier::AnalogBasic.to_string(), "AnalogBasic");
        assert_eq!(PerformanceTier::AnalogAdvanced.to_string(), "AnalogAdvanced");
    }
}
