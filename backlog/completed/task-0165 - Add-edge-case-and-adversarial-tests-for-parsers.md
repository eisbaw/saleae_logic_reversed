---
id: TASK-0165
title: Add edge-case and adversarial tests for parsers
status: Done
assignee: []
created_date: '2026-03-12 22:27'
updated_date: '2026-03-13 00:05'
labels:
  - testing
  - proto
  - driver
dependencies: []
references:
  - crates/saleae-proto/src/calibration.rs
  - crates/saleae-proto/src/biquad.rs
  - crates/saleae-usb/src/discovery.rs
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Multiple parsers lack error/edge-case tests: (1) calibration: malformed XML, missing fields, wrong coeff count, NaN values, empty input, a0!=1.0 rejection. (2) biquad: NaN propagation, zero sections, extreme coefficients. (3) fx3: MAX_SECTIONS overflow, section length overflow, trailing garbage. (4) VoltageTransform: adc_min==adc_max (div by zero). (5) discovery.rs: VID:PID classification has zero unit tests.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added 46 edge-case and adversarial tests across 4 modules in saleae-proto and device.rs. Tests cover: empty/garbage/malformed input, missing required fields, wrong coefficient counts, a0!=1.0 rejection, NaN/infinity propagation, unstable filters, FX3 section overflow/truncation, trailing garbage, VID:PID classification boundaries. Discovered NaN-in-a0 bug: the normalization check silently passes NaN due to NaN comparison semantics. Commit: 6b45bfe.
<!-- SECTION:FINAL_SUMMARY:END -->
