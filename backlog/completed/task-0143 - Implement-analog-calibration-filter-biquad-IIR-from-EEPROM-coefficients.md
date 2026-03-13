---
id: TASK-0143
title: Implement analog calibration filter (biquad IIR from EEPROM coefficients)
status: Done
assignee: []
created_date: '2026-03-12 14:12'
updated_date: '2026-03-12 15:24'
labels:
  - phase-2
  - data-path
dependencies: []
references:
  - doc/calibration/calibration-format.md
  - crates/saleae-proto/src/biquad.rs
  - re/ghidra/decompiled-calibration-filter.txt
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Raw analog samples from the disaggregator need calibration applied before they match Saleae Logic 2 output. The EEPROM at offset 0x10 contains 16 calibration coefficients. These feed into a biquad IIR filter chain that compensates AFE distortion.

The biquad filter infrastructure exists in saleae-proto (biquad.rs) but the coefficient-to-filter pipeline is not connected. Need to:
1. Parse EEPROM calibration bytes into filter coefficients
2. Apply biquad IIR to each analog channel's i16 samples
3. Convert to voltage (scale by ADC VREF and gain)

RE source: re/notes/calibration-format.md, re/ghidra/decompiled-calibration-filter.txt, TASK-0130 (done)
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Implemented in commits 46bbafc + MPED fixes. Added calibrate.rs module with CalibratedChannel/CalibratedOutput structs and apply_calibration() function. Processing order matches binary: filter raw i16 first via BiquadFilter, then VoltageTransform to voltages. Returns Result with DriverError for missing calibration data. ADC range parameterized (not hardcoded). CLI wired with TODO for deriving .cal path from device serial.
<!-- SECTION:FINAL_SUMMARY:END -->
