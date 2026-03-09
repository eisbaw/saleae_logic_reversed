---
id: TASK-0062
title: Implement calibration download and IIR biquad filter
status: Done
assignee: []
created_date: '2026-03-09 11:22'
updated_date: '2026-03-09 13:18'
labels:
  - rust
  - calibration
  - analog
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Download device-specific calibration data and implement the IIR biquad filter for analog signal correction.

Implementation:
1. After reading EEPROM device ID, construct calibration URL:
   `https://downloads.saleae.com/calibration/{device_id_hex}.cal`
2. Download .cal file (Boost.Serialization XML format)
3. Parse XML: extract mDf1SosCoefficients (30 values per channel per sample rate)
4. Implement Direct Form I Second-Order Section (DF1 SOS) IIR filter:
   - 5 cascaded biquad sections per channel
   - Section 0 is first-order (b2=0, a2=0), sections 1-4 are second-order
   - Coefficients: b0, b1, b2, a0 (always 1.0), a1, a2
   - Sections 1-4 are shared across channels at same sample rate
5. Apply filter to raw int16_t ADC samples after voltage transform
6. Cache calibration file locally

Unit tests:
- Parse the reference .cal file at re/calibration-764051b51bd4d9f1.cal
- Verify coefficient counts (48 entries, 30 coeffs each)
- Verify a0 is always 1.0
- Filter a known impulse signal and check output

References: doc/calibration/calibration-format.md, re/notes/calibration-validation.md, re/notes/calibration-system.md
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Implemented calibration parser, IIR biquad filter, and ADC voltage transform in commit d3f9ecc.

## saleae-proto (new modules)

### calibration.rs
- Parses Boost.Serialization XML .cal files using quick-xml event-based reader
- CalibrationData, CalibrationSet, BiquadSection, VoltageRange structs
- VoltageTransform for linear ADC-to-voltage conversion
- Validates a0=1.0 for all biquad sections during parsing
- 19 tests covering: device ID, type, date, ADC gain, 48 cal sets, 8 voltage ranges, coefficient values, section sharing, a0 validation, voltage transform for Pro (12-bit signed) and Logic 4 (8-bit unsigned)

### biquad.rs
- Direct Form I cascaded biquad filter (BiquadFilter)
- process_sample, process_block (in-place), reset methods
- 8 tests: identity, cascade, impulse response, DC convergence, block vs sample equivalence, reset, real calibration coefficients

## saleae-driver

### calibration.rs
- download_calibration(): HTTP download from downloads.saleae.com with local file caching
- load_calibration_file(): Direct file load for testing/offline use
- Uses ureq v3 for HTTP with TLS
- 3 tests: reference file load, nonexistent file error, cache hit skips download

## Also fixed
- Pre-existing doctest failure in obfuscation.rs (missing is_firmware_download argument)

## Validation
All 112 workspace tests pass, zero clippy warnings.
<!-- SECTION:FINAL_SUMMARY:END -->
