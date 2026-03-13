---
id: TASK-0172
title: 'Make calibration adc_gain optional and add ProtoError::Io variant'
status: Done
assignee: []
created_date: '2026-03-12 22:28'
updated_date: '2026-03-12 22:52'
labels:
  - proto
  - error-handling
  - robustness
dependencies: []
references:
  - 'crates/saleae-proto/src/calibration.rs:389'
  - crates/saleae-proto/src/error.rs
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Two proto crate robustness issues: (1) adc_gain is required by parser but always 0.0, not used in pipeline, and documented as hardware-only. If a future .cal file omits mAdcGain, parser fails entirely. Should default to 0.0. (2) ProtoError has no Io variant, but sal.rs does I/O operations. Callers mixing proto parsing with .sal I/O must handle two error types. Add Io variant wrapping io::Error.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Made two robustness improvements to saleae-proto:

1. **adc_gain defaults to 0.0 when missing**: Changed `parse_calibration` to use `unwrap_or(0.0)` instead of erroring when `mAdcGain` is absent from a .cal file. The field is always 0.0 in observed files and unused in the software calibration pipeline (it controls a hardware ADC register). Added test with minimal XML lacking the element.

2. **Added ProtoError::Io variant**: New `Io(Arc<std::io::Error>)` variant with `From<std::io::Error>` impl. Uses `Arc` wrapper to preserve the existing `Clone` derive on `ProtoError`. Enables callers to use `?` with io::Error in functions returning `ProtoResult`.

Files changed:
- `crates/saleae-proto/src/error.rs` — Added `Io` variant and `From` impl
- `crates/saleae-proto/src/calibration.rs` — Default adc_gain, added test
<!-- SECTION:FINAL_SUMMARY:END -->
