---
id: TASK-0177
title: Consolidate dual calibration pipelines (filter-first vs voltage-first)
status: Done
assignee: []
created_date: '2026-03-13 05:35'
updated_date: '2026-03-13 06:58'
labels:
  - calibration
  - bug
  - domain
dependencies: []
references:
  - crates/saleae-driver/src/calibrate.rs
  - crates/saleae-proto/src/analog.rs
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Two calibration pipelines apply biquad filter and voltage transform in opposite orders: (A) calibrate.rs::apply_calibration does filter-first (raw i16 -> biquad -> voltage transform) which matches the binary. (B) analog.rs::AnalogProcessor does voltage-first (raw i16 -> voltage transform -> biquad). The binary operates on raw i16 ADC codes through IIR, then applies voltage transform. Pipeline A is correct per RE findings. Pipeline B produces subtly different results due to i16 truncation/saturation differences. Deprecate Pipeline B or make both use filter-first order.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Fixed AnalogProcessor in analog.rs to use filter-first order (raw i16 -> biquad filter -> voltage transform), matching both the Saleae binary's processing order and calibrate.rs::apply_calibration. Previously AnalogProcessor applied voltage transform first, then filtered on f64 -- wrong order. Both process() and process_into() methods updated. All 863 tests pass. Commit 32629a5.
<!-- SECTION:FINAL_SUMMARY:END -->
