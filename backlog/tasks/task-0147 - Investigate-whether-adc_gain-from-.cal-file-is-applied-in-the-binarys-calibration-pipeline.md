---
id: TASK-0147
title: >-
  Investigate whether adc_gain from .cal file is applied in the binary's
  calibration pipeline
status: Done
assignee: []
created_date: '2026-03-12 16:40'
updated_date: '2026-03-12 16:51'
labels:
  - RE
  - calibration
dependencies: []
references:
  - crates/saleae-proto/src/calibration.rs
  - crates/saleae-driver/src/calibrate.rs
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The .cal file contains an `adc_gain` field that is parsed and stored in `CalibrationData` but never used in the calibration pipeline. It's unclear whether the binary applies this value. If it does, the voltage transform is missing a multiplicative factor. If it doesn't, the field should be documented as unused. Needs Ghidra investigation of the calibration graph node.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
RE investigation confirms mAdcGain from .cal files is always 0.0 and is NOT applied in the software calibration pipeline. The binary's SetAdcFullScaleGain node writes hittiteAdcGainRegisterValue to HMCAD1100 hardware, a separate mechanism. Documented the field in calibration.rs. Commit 05a2dd8.
<!-- SECTION:FINAL_SUMMARY:END -->
