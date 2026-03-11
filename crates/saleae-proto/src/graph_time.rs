//! High-precision timestamp for the Saleae graph system.
//!
//! `GraphTime` wraps a 128-bit nanosecond count, matching the internal
//! representation used by `libgraph_server_shared.so`. The original C++
//! `GraphTime` stores `{uint64_t seconds, double fractional_ns}` and
//! converts to `i128` nanoseconds for precision-sensitive operations
//! (RocksDB keys, sample index math, modulus for repeating signals).
//!
//! Rust's native `i128` replaces boost::multiprecision's `cpp_int`
//! without any loss of precision or range.
//!
//! # Confidence
//!
//! **High** -- reconstructed from 20+ call sites using `__fixunsdfti`
//! (double-to-i128) with constant `1_000_000_000`. See
//! `re/notes/boost-multiprecision.md` for the full analysis.

use std::fmt;
use std::ops::{Add, Div, Mul, Neg, Sub};

/// Nanoseconds per second, used throughout for conversions.
const NANOS_PER_SEC: i128 = 1_000_000_000;

/// High-precision timestamp stored as nanoseconds.
///
/// This is a newtype around `i128`, representing nanoseconds from an
/// arbitrary epoch (typically Unix epoch in the Saleae software).
///
/// The range is effectively unlimited for practical use:
/// `i128::MAX` nanoseconds is approximately 5.4 * 10^21 years.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct GraphTime(pub i128);

impl GraphTime {
    /// The zero timestamp.
    pub const ZERO: GraphTime = GraphTime(0);

    /// Construct from a raw nanosecond count.
    pub const fn from_nanos(nanos: i128) -> Self {
        GraphTime(nanos)
    }

    /// Construct from whole seconds.
    ///
    /// Matches the C++ path: `(i128)seconds * 1_000_000_000`.
    pub const fn from_seconds(seconds: u64) -> Self {
        GraphTime(seconds as i128 * NANOS_PER_SEC)
    }

    /// Construct from seconds and a fractional nanosecond component.
    ///
    /// This mirrors the binary's internal conversion exactly:
    /// ```text
    /// total_ns = (i128)seconds * 1_000_000_000 + (i128)fractional_ns
    /// ```
    ///
    /// The `fractional_ns` parameter is a `f64` representing additional
    /// nanoseconds (can be negative for sub-nanosecond adjustments).
    /// It is truncated toward zero when cast to `i128`.
    pub fn from_seconds_and_frac(seconds: u64, fractional_ns: f64) -> Self {
        let seconds_ns = seconds as i128 * NANOS_PER_SEC;
        // Match the C++ pattern: absolute value conversion with sign preservation.
        let frac_abs = fractional_ns.abs();
        let frac_i128 = frac_abs as i128;
        let frac_ns = if fractional_ns < 0.0 {
            -(frac_i128)
        } else {
            frac_i128
        };
        GraphTime(seconds_ns + frac_ns)
    }

    /// Return the raw nanosecond count.
    pub const fn nanos(&self) -> i128 {
        self.0
    }

    /// Return whole seconds (truncated toward zero).
    pub const fn seconds(&self) -> i64 {
        (self.0 / NANOS_PER_SEC) as i64
    }

    /// Return the sub-second nanosecond remainder.
    ///
    /// For positive times, this is in `[0, 999_999_999]`.
    /// For negative times, this is in `[-999_999_999, 0]`.
    pub const fn subsec_nanos(&self) -> i64 {
        (self.0 % NANOS_PER_SEC) as i64
    }

    /// Serialize to 16 big-endian bytes (for RocksDB key ordering).
    ///
    /// The sign bit is flipped so that lexicographic byte comparison
    /// matches numeric ordering across the full `i128` range. This is
    /// the standard encoding for signed keys in sorted KV stores:
    /// `encoded = big_endian(value XOR (1 << 127))`.
    ///
    /// Without this flip, negative values (high bit set in two's
    /// complement) would sort after positive values in byte order.
    pub const fn to_be_bytes(&self) -> [u8; 16] {
        // XOR with sign-bit mask: flips i128::MIN..=-1 to 0x00..=0x7F
        // and 0..=i128::MAX to 0x80..=0xFF in the high byte.
        let flipped = self.0 ^ (1_i128 << 127);
        flipped.to_be_bytes()
    }

    /// Deserialize from 16 big-endian bytes produced by [`to_be_bytes`].
    pub const fn from_be_bytes(bytes: &[u8; 16]) -> Self {
        let flipped = i128::from_be_bytes(*bytes);
        GraphTime(flipped ^ (1_i128 << 127))
    }

    /// Convert a sample index to a timestamp given the sample rate.
    ///
    /// `time_ns = index * 1_000_000_000 / sample_rate`
    ///
    /// This uses i128 arithmetic to avoid overflow for large indices.
    ///
    /// # Panics
    ///
    /// Panics if `sample_rate` is zero.
    pub const fn from_sample_index(index: u64, sample_rate: u64) -> Self {
        assert!(sample_rate > 0, "sample_rate must be > 0");
        let nanos = (index as i128) * NANOS_PER_SEC / (sample_rate as i128);
        GraphTime(nanos)
    }

    /// Convert a timestamp to the nearest sample index at a given rate.
    ///
    /// `index = time_ns * sample_rate / 1_000_000_000`
    ///
    /// Negative times return 0. Overflow beyond `u64::MAX` saturates.
    ///
    /// # Panics
    ///
    /// Panics if `sample_rate` is zero.
    pub const fn to_sample_index(&self, sample_rate: u64) -> u64 {
        assert!(sample_rate > 0, "sample_rate must be > 0");
        if self.0 <= 0 {
            return 0;
        }
        let idx = self.0 * (sample_rate as i128) / NANOS_PER_SEC;
        if idx > u64::MAX as i128 {
            u64::MAX
        } else {
            idx as u64
        }
    }
}

// ---------------------------------------------------------------------------
// Arithmetic
// ---------------------------------------------------------------------------

impl Add for GraphTime {
    type Output = Self;

    fn add(self, rhs: Self) -> Self {
        GraphTime(self.0 + rhs.0)
    }
}

impl Sub for GraphTime {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self {
        GraphTime(self.0 - rhs.0)
    }
}

impl Neg for GraphTime {
    type Output = Self;

    fn neg(self) -> Self {
        GraphTime(-self.0)
    }
}

impl Mul<i128> for GraphTime {
    type Output = Self;

    fn mul(self, rhs: i128) -> Self {
        GraphTime(self.0 * rhs)
    }
}

impl Div<i128> for GraphTime {
    type Output = Self;

    fn div(self, rhs: i128) -> Self {
        GraphTime(self.0 / rhs)
    }
}

// ---------------------------------------------------------------------------
// Display
// ---------------------------------------------------------------------------

impl fmt::Display for GraphTime {
    /// Format as "X.XXXXXXXXX s" (seconds with 9 decimal places).
    ///
    /// Negative values show a leading minus sign. The fractional part
    /// is always 9 digits, zero-padded.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let total = self.0;
        let negative = total < 0;
        let abs_total = if total == i128::MIN {
            // i128::MIN.abs() overflows; handle it specially.
            // This value is so extreme it will never appear in practice,
            // but we must not panic.
            (i128::MAX as u128) + 1
        } else {
            total.unsigned_abs()
        };

        let secs = abs_total / (NANOS_PER_SEC as u128);
        let frac = abs_total % (NANOS_PER_SEC as u128);

        if negative {
            write!(f, "-{}.{:09} s", secs, frac)
        } else {
            write!(f, "{}.{:09} s", secs, frac)
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
    // Construction and accessors
    // -------------------------------------------------------------------

    #[test]
    fn zero_constant() {
        assert_eq!(GraphTime::ZERO.nanos(), 0);
        assert_eq!(GraphTime::ZERO.seconds(), 0);
        assert_eq!(GraphTime::ZERO.subsec_nanos(), 0);
    }

    #[test]
    fn from_nanos_basic() {
        let t = GraphTime::from_nanos(1_500_000_000);
        assert_eq!(t.seconds(), 1);
        assert_eq!(t.subsec_nanos(), 500_000_000);
    }

    #[test]
    fn from_seconds_basic() {
        let t = GraphTime::from_seconds(42);
        assert_eq!(t.nanos(), 42_000_000_000);
        assert_eq!(t.seconds(), 42);
        assert_eq!(t.subsec_nanos(), 0);
    }

    #[test]
    fn from_seconds_and_frac_positive() {
        // 10 seconds + 500_000_000.0 fractional ns = 10.5 seconds
        let t = GraphTime::from_seconds_and_frac(10, 500_000_000.0);
        assert_eq!(t.nanos(), 10_500_000_000);
        assert_eq!(t.seconds(), 10);
        assert_eq!(t.subsec_nanos(), 500_000_000);
    }

    #[test]
    fn from_seconds_and_frac_negative_frac() {
        // 10 seconds - 100 ns = 9_999_999_900 ns
        let t = GraphTime::from_seconds_and_frac(10, -100.0);
        assert_eq!(t.nanos(), 9_999_999_900);
    }

    #[test]
    fn from_seconds_and_frac_zero_frac() {
        let t = GraphTime::from_seconds_and_frac(5, 0.0);
        assert_eq!(t.nanos(), 5_000_000_000);
    }

    #[test]
    fn negative_time_accessors() {
        let t = GraphTime::from_nanos(-2_500_000_000);
        assert_eq!(t.seconds(), -2);
        assert_eq!(t.subsec_nanos(), -500_000_000);
    }

    // -------------------------------------------------------------------
    // Arithmetic
    // -------------------------------------------------------------------

    #[test]
    fn add_basic() {
        let a = GraphTime::from_seconds(1);
        let b = GraphTime::from_nanos(500_000_000);
        let sum = a + b;
        assert_eq!(sum.nanos(), 1_500_000_000);
    }

    #[test]
    fn sub_basic() {
        let a = GraphTime::from_seconds(3);
        let b = GraphTime::from_seconds(1);
        let diff = a - b;
        assert_eq!(diff.nanos(), 2_000_000_000);
    }

    #[test]
    fn sub_negative_result() {
        let a = GraphTime::from_seconds(1);
        let b = GraphTime::from_seconds(3);
        let diff = a - b;
        assert_eq!(diff.nanos(), -2_000_000_000);
    }

    #[test]
    fn neg_basic() {
        let t = GraphTime::from_seconds(5);
        let neg_t = -t;
        assert_eq!(neg_t.nanos(), -5_000_000_000);
    }

    #[test]
    fn neg_zero() {
        let t = -GraphTime::ZERO;
        assert_eq!(t, GraphTime::ZERO);
    }

    #[test]
    fn mul_basic() {
        let t = GraphTime::from_seconds(2);
        let scaled = t * 3;
        assert_eq!(scaled.nanos(), 6_000_000_000);
    }

    #[test]
    fn div_basic() {
        let t = GraphTime::from_seconds(6);
        let scaled = t / 3;
        assert_eq!(scaled.nanos(), 2_000_000_000);
    }

    #[test]
    fn div_truncates() {
        let t = GraphTime::from_nanos(10);
        let scaled = t / 3;
        assert_eq!(scaled.nanos(), 3); // 10/3 = 3 (truncated)
    }

    // -------------------------------------------------------------------
    // Comparison
    // -------------------------------------------------------------------

    #[test]
    fn ordering() {
        let a = GraphTime::from_seconds(1);
        let b = GraphTime::from_seconds(2);
        let c = GraphTime::from_nanos(-1);

        assert!(a < b);
        assert!(c < a);
        assert!(b > a);
        assert_eq!(a, GraphTime::from_nanos(1_000_000_000));
    }

    // -------------------------------------------------------------------
    // Display
    // -------------------------------------------------------------------

    #[test]
    fn display_zero() {
        assert_eq!(format!("{}", GraphTime::ZERO), "0.000000000 s");
    }

    #[test]
    fn display_positive() {
        let t = GraphTime::from_nanos(1_234_567_890);
        assert_eq!(format!("{}", t), "1.234567890 s");
    }

    #[test]
    fn display_subsecond() {
        let t = GraphTime::from_nanos(42);
        assert_eq!(format!("{}", t), "0.000000042 s");
    }

    #[test]
    fn display_negative() {
        let t = GraphTime::from_nanos(-1_234_567_890);
        assert_eq!(format!("{}", t), "-1.234567890 s");
    }

    #[test]
    fn display_large() {
        // 1000 seconds exactly
        let t = GraphTime::from_seconds(1000);
        assert_eq!(format!("{}", t), "1000.000000000 s");
    }

    // -------------------------------------------------------------------
    // Byte serialization
    // -------------------------------------------------------------------

    #[test]
    fn be_bytes_roundtrip_positive() {
        let t = GraphTime::from_nanos(123_456_789_012_345_678);
        let bytes = t.to_be_bytes();
        let recovered = GraphTime::from_be_bytes(&bytes);
        assert_eq!(t, recovered);
    }

    #[test]
    fn be_bytes_roundtrip_negative() {
        let t = GraphTime::from_nanos(-999_999_999);
        let bytes = t.to_be_bytes();
        let recovered = GraphTime::from_be_bytes(&bytes);
        assert_eq!(t, recovered);
    }

    #[test]
    fn be_bytes_roundtrip_zero() {
        let t = GraphTime::ZERO;
        let bytes = t.to_be_bytes();
        let recovered = GraphTime::from_be_bytes(&bytes);
        assert_eq!(t, recovered);
    }

    #[test]
    fn be_bytes_ordering() {
        // Big-endian encoding must preserve ordering for RocksDB scans.
        let a = GraphTime::from_seconds(1);
        let b = GraphTime::from_seconds(2);
        assert!(a.to_be_bytes() < b.to_be_bytes());
    }

    #[test]
    fn be_bytes_ordering_negative() {
        // Negative values should sort before positive in i128 big-endian.
        let neg = GraphTime::from_nanos(-1);
        let pos = GraphTime::from_nanos(1);
        assert!(neg.to_be_bytes() < pos.to_be_bytes());
    }

    // -------------------------------------------------------------------
    // Sample index conversion
    // -------------------------------------------------------------------

    #[test]
    fn from_sample_index_1mhz() {
        // At 1 MHz, sample 0 = 0 ns, sample 1 = 1000 ns
        let t = GraphTime::from_sample_index(1, 1_000_000);
        assert_eq!(t.nanos(), 1_000); // 1e9 / 1e6 = 1000 ns
    }

    #[test]
    fn from_sample_index_500mhz() {
        // At 500 MHz, sample 1 = 2 ns
        let t = GraphTime::from_sample_index(1, 500_000_000);
        assert_eq!(t.nanos(), 2);
    }

    #[test]
    fn from_sample_index_zero() {
        let t = GraphTime::from_sample_index(0, 44_100);
        assert_eq!(t, GraphTime::ZERO);
    }

    #[test]
    fn from_sample_index_large() {
        // 1 billion samples at 1 GHz = 1 second
        let t = GraphTime::from_sample_index(1_000_000_000, 1_000_000_000);
        assert_eq!(t.nanos(), 1_000_000_000);
    }

    #[test]
    fn to_sample_index_1mhz() {
        let t = GraphTime::from_nanos(1_000);
        assert_eq!(t.to_sample_index(1_000_000), 1);
    }

    #[test]
    fn to_sample_index_500mhz() {
        let t = GraphTime::from_nanos(2);
        assert_eq!(t.to_sample_index(500_000_000), 1);
    }

    #[test]
    fn to_sample_index_zero() {
        assert_eq!(GraphTime::ZERO.to_sample_index(44_100), 0);
    }

    #[test]
    fn to_sample_index_negative_returns_zero() {
        let t = GraphTime::from_nanos(-100);
        assert_eq!(t.to_sample_index(1_000_000), 0);
    }

    #[test]
    fn sample_index_roundtrip_exact() {
        // Exact divisors should round-trip perfectly.
        let rate = 1_000_000_000u64; // 1 GHz -- each sample is exactly 1 ns
        for idx in [0u64, 1, 100, 1_000_000, u64::MAX / 2] {
            let t = GraphTime::from_sample_index(idx, rate);
            assert_eq!(
                t.to_sample_index(rate),
                idx,
                "roundtrip failed for idx={idx}"
            );
        }
    }

    #[test]
    fn sample_index_roundtrip_inexact() {
        // At 44.1 kHz, 1e9/44100 = 22675.736... ns per sample.
        // Truncation means roundtrip is not exact but should be close.
        let rate = 44_100u64;
        let idx = 100u64;
        let t = GraphTime::from_sample_index(idx, rate);
        let recovered = t.to_sample_index(rate);
        // Should be within 1 sample of original
        assert!(
            recovered == idx || recovered == idx - 1 || recovered == idx + 1,
            "expected ~{idx}, got {recovered}"
        );
    }

    // -------------------------------------------------------------------
    // Edge cases
    // -------------------------------------------------------------------

    #[test]
    fn very_large_positive() {
        let t = GraphTime::from_nanos(i128::MAX);
        assert_eq!(t.nanos(), i128::MAX);
        // Roundtrip through bytes
        let recovered = GraphTime::from_be_bytes(&t.to_be_bytes());
        assert_eq!(t, recovered);
    }

    #[test]
    fn very_large_negative() {
        let t = GraphTime::from_nanos(i128::MIN);
        assert_eq!(t.nanos(), i128::MIN);
        let recovered = GraphTime::from_be_bytes(&t.to_be_bytes());
        assert_eq!(t, recovered);
    }

    #[test]
    fn display_i128_min() {
        // Should not panic
        let t = GraphTime::from_nanos(i128::MIN);
        let s = format!("{}", t);
        assert!(s.starts_with('-'));
        assert!(s.ends_with(" s"));
    }

    #[test]
    #[should_panic(expected = "sample_rate must be > 0")]
    fn from_sample_index_zero_rate_panics() {
        GraphTime::from_sample_index(1, 0);
    }

    #[test]
    #[should_panic(expected = "sample_rate must be > 0")]
    fn to_sample_index_zero_rate_panics() {
        GraphTime::from_seconds(1).to_sample_index(0);
    }

    // -------------------------------------------------------------------
    // Conversion roundtrips
    // -------------------------------------------------------------------

    #[test]
    fn seconds_and_frac_reconstruction() {
        // Verify we can reconstruct from seconds() + subsec_nanos()
        let t = GraphTime::from_nanos(3_750_000_001);
        let reconstructed =
            GraphTime::from_nanos(t.seconds() as i128 * NANOS_PER_SEC + t.subsec_nanos() as i128);
        assert_eq!(t, reconstructed);
    }

    #[test]
    fn seconds_and_frac_negative_reconstruction() {
        let t = GraphTime::from_nanos(-3_750_000_001);
        let reconstructed =
            GraphTime::from_nanos(t.seconds() as i128 * NANOS_PER_SEC + t.subsec_nanos() as i128);
        assert_eq!(t, reconstructed);
    }
}
