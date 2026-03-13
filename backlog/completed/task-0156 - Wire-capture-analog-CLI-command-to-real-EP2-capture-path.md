---
id: TASK-0156
title: Wire capture-analog CLI command to real EP2 capture path
status: Done
assignee: []
created_date: '2026-03-12 22:27'
updated_date: '2026-03-12 23:30'
labels:
  - cli
  - feature
  - analog
  - PRD
dependencies: []
references:
  - 'crates/saleae-cli/src/commands.rs:1695'
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
capture_analog() in commands.rs is a stub that validates config and loads calibration, then unconditionally bails at line 1695 with "hardware capture not yet available." PRD Slice 5 requires a working analog capture. Needs: EP2 read loop, disaggregation, calibration application, output to file. The pieces (run_capture, Disaggregator, apply_calibration) all exist -- just need wiring.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Replaced the capture_analog stub (which bailed with "hardware capture not yet available") with a fully wired implementation following the same pattern as capture_digital:

1. Firmware upload + EP1 init (auto-recovery via ensure_device_ready)
2. FPGA programming + verify
3. AD9637 ADC init + DCM lock
4. EP1 re-init + channel enables
5. Pre-capture ADC power-on
6. EP2 capture loop via run_capture (using CaptureConfig with analog channels, no digital)
7. Disaggregate raw EP2 data into per-channel i16 ADC samples via Disaggregator
8. Apply calibration pipeline (VoltageTransform + IIR biquad filter) per channel
9. Export calibrated voltages to CSV or binary via AnalogExporter
10. ADC power-down teardown

Added --firmware and --bitstream CLI args to CaptureAnalog (matching CaptureDigital pattern). Calibration gracefully degrades: without a .cal file, falls back to naive +/-10V linear transform.

Changed files:
- crates/saleae-cli/src/main.rs (added firmware/bitstream args, updated match arm)
- crates/saleae-cli/src/commands.rs (rewrote capture_analog from stub to full pipeline)
<!-- SECTION:FINAL_SUMMARY:END -->
