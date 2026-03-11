---
id: TASK-0130
title: RE AnalogCalibrationFilter node -- coefficient computation from .cal data
status: To Do
assignee: []
created_date: '2026-03-11 11:21'
labels:
  - re
  - ghidra
  - analog
  - calibration
dependencies: []
references:
  - re/notes/calibration-system.md
  - re/notes/calibration-download.md
  - crates/saleae-proto/src/calibration.rs
  - crates/saleae-proto/src/biquad.rs
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The AnalogCalibrationFilter graph node receives calibration data via SetCalibration action and computes IIR/FIR filter coefficients. This is the bridge between the downloaded .cal file and the actual filter that processes analog samples.

We know:
- Calibration downloads as XML (Boost.Serialization) from downloads.saleae.com
- Filter is applied per-channel, per-sample-rate
- SimdIirFilterPrototype and SimdPrototype_Static2By3IirFilter are the filter implementations (TASK-0118)

What we DON'T know:
- How .cal XML data is transformed into filter coefficients
- Whether coefficients are pre-computed in the .cal file or computed at runtime
- The exact mapping: .cal fields -> biquad section coefficients

Investigation:
1. Decompile AnalogCalibrationFilter::SetCalibration handler
2. Trace the data flow from CalibrationData to filter coefficients
3. Document the coefficient computation algorithm
4. Compare against our calibration.rs and biquad.rs

This is separate from TASK-0118 (which RE's the filter execution). This task RE's how coefficients are derived from calibration data.
<!-- SECTION:DESCRIPTION:END -->
