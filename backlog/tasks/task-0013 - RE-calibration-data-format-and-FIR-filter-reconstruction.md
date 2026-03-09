---
id: TASK-0013
title: RE calibration data format and FIR filter reconstruction
status: Done
assignee: []
created_date: '2026-03-08 22:44'
updated_date: '2026-03-09 01:02'
labels: []
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Reverse engineer the calibration system -- this is critical for correct analog output.

Known from string/symbol analysis:
- afeConfigCalibrations -- AFE calibration data structure
- "ADC failed to finish foreground calibration"
- Per-channel calibration
- FIR filter coefficients derived from calibration

Key insight: analog sampling is imperfect. The analog front-end introduces frequency-dependent distortion. Saleae compensates this with FIR filters whose coefficients come from factory calibration data. Without these filters, analog output will be wrong.

Investigate:
1. Where is calibration data stored? (server-side download? device EEPROM? embedded in app?)
2. What is the calibration data format? (binary? JSON? protobuf?)
3. What parameters are calibrated per channel? (gain, offset, frequency response)
4. How are FIR filter coefficients derived from calibration data?
5. How many FIR taps? Is it per-sample-rate or one set of coefficients?
6. What is the AFE configuration that calibration modifies?
7. Is there a calibration file on disk after first run?

This is the highest-difficulty RE task. The FIR filter reconstruction determines whether our analog output matches Saleae's.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Key Findings

### Calibration Data Source
Logic devices download calibration from Saleae's cloud servers, NOT from device EEPROM. The calibration file is Boost.Serialization XML format (.cal extension) hosted at `https://downloads.saleae.com/calibration/{deviceIdHex}.cal`. A GraphQL API at `https://api.saleae.com/graphql` provides metadata (download URL, last updated timestamp).

### Filter Type: IIR (not FIR)
Contrary to initial assumptions, the calibration filter is an IIR filter, not FIR. Specifically, it uses a cascade of 5 second-order sections (SOS) in Direct Form I (DF1) topology. Each section is a biquad:
```
H(z) = (b0 + b1*z^-1 + b2*z^-2) / (1 + a1*z^-1 + a2*z^-2)
```
5 sections = 10th order IIR filter total. SIMD-optimized implementation exists (`SimdIirFilterPrototype`, `SimdPrototype_Static2By3IirFilter`).

### Calibration Data Structure
Per-device calibration file contains:
- Device ID and type validation
- `mAdcGain` (Hittite HMCAD1100 full-scale gain register value)
- `mFullScaleVoltageRanges` -- per-channel min/max voltage
- `mCalibrationData` -- per (channel, sampleRate) IIR coefficients:
  - `mGroupDelay` (float) -- filter group delay in samples
  - `mImpulseLength` (float) -- effective impulse response length
  - `mDf1SosCoefficients` -- 30 floats (5 sections x 6 values: b0,b1,b2,a0,a1,a2)

### Devices Requiring Calibration
Logic4, Logic8, LogicPro8, LogicPro16 (all devices with analog channels).

### ADC-to-Voltage Transform
Separate from the IIR filter. Simple linear: `voltage = adc_code * m + b` where m and b are computed from fullScaleVoltageRanges.

### EEPROM vs Server
- Logic devices: calibration from cloud server (NOT EEPROM)
- MSO devices: calibration from AT24C64D I2C EEPROM on device

## Files Created
- `re/notes/calibration-system.md` -- detailed analysis notes
- `doc/calibration/calibration-format.md` -- verified protocol doc
- `re/ghidra/scripts/TraceCalibration.py` -- Ghidra tracing script
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Fully reverse engineered the calibration system for Saleae Logic 2. Discovered that the filter is IIR (not FIR as initially assumed) -- specifically a cascade of 5 biquad second-order sections in Direct Form I. Calibration files are Boost.Serialization XML downloaded from `downloads.saleae.com/calibration/{hex}.cal`, containing per-channel, per-sample-rate IIR coefficients plus full-scale voltage ranges and ADC gain register values. Created comprehensive documentation in `doc/calibration/calibration-format.md`, analysis notes in `re/notes/calibration-system.md`, and Ghidra tracing script `re/ghidra/scripts/TraceCalibration.py`.
<!-- SECTION:FINAL_SUMMARY:END -->
