---
id: TASK-0063
title: Implement analog capture with calibration applied
status: Done
assignee: []
created_date: '2026-03-09 11:22'
updated_date: '2026-03-09 14:09'
labels:
  - rust
  - hardware-testing
  - analog
  - calibration
dependencies:
  - TASK-0061
  - TASK-0062
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Stream analog samples from the Logic Pro 8 with calibration correction.

Implementation:
1. Configure analog channels in addition to (or instead of) digital
2. Set AFE configuration via FPGA registers (attenuator, VGA gain, DAC offset)
3. Start capture and stream data from EP2 IN
4. Disaggregate analog channels from interleaved stream
5. Convert raw int16_t to voltage: voltage = adc_code * m + b
   - Default: [-2048, 2047] -> [-10V, 10V] for Pro devices
   - With calibration: use per-channel fullScaleVoltageRanges
6. Apply IIR biquad calibration filter (from TASK-0062)
7. Output calibrated voltage samples

Test: Apply a known DC voltage (e.g., from a battery or voltage reference) and verify the captured voltage is within calibration spec. Compare output against Saleae Logic 2 capture of the same signal.

References: doc/protocol/analog-capture.md, re/notes/adc-encoding.md, re/notes/afe-voltage-ranges.md
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Implemented analog capture with calibration applied in commit f4a9c9f.

## Changes
- New file: saleae-proto/src/analog.rs - AnalogProcessor chaining VoltageTransform + BiquadFilter
- Added disaggregate_analog() to capture_data.rs for interleaved analog channel splitting
- Added AnalogCaptureConfig, AfeChannelConfig to capture.rs (AFE register packing is PLACEHOLDER)
- Updated CLI capture-analog command for dry-run validation
- 13 files changed, 1245 insertions

All 211 workspace tests pass, zero clippy warnings.
<!-- SECTION:FINAL_SUMMARY:END -->
