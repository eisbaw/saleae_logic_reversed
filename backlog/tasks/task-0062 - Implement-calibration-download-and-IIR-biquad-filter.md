---
id: TASK-0062
title: Implement calibration download and IIR biquad filter
status: To Do
assignee: []
created_date: '2026-03-09 11:22'
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
