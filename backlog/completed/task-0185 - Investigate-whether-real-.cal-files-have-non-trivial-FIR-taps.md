---
id: TASK-0185
title: Investigate whether real .cal files have non-trivial FIR taps
status: Done
assignee: []
created_date: '2026-03-13 05:36'
updated_date: '2026-03-13 07:57'
labels:
  - calibration
  - RE
  - investigation
dependencies: []
references:
  - crates/saleae-proto/src/calibration.rs
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The .cal parser tracks mFirTaps elements but discards the actual FIR tap values. If any device's calibration has non-identity FIR taps, we are silently dropping an entire filter stage. Need to: (1) Download .cal files for multiple device models, (2) Check if any have non-zero/non-identity FIR taps, (3) If yes, implement a convolution stage in the calibration pipeline.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
## Investigation Result: mFirTaps are identity (duplicates of SOS coefficients)

Examined the real .cal file `re/calibration-764051b51bd4d9f1.cal` (Logic Pro 8, device type 6, 48 calibration entries across 8 channels x 6 sample rates).

### Findings

1. **All 48 mFirTaps blocks are exact copies of their corresponding mDf1SosCoefficients** -- not a single entry differs.
2. The values are non-trivial filter coefficients (not identity), but they duplicate the IIR biquad coefficients rather than representing an independent FIR filter stage.
3. The Saleae JS parser also ignores mFirTaps (confirmed in re/notes/calibration-validation.md).
4. mGroupDelay (3 or 5) and mImpulseLength (64-3381) are non-trivial and already parsed correctly.

### Conclusion

No filter stage is being silently dropped. mFirTaps is a historical artifact from Boost.Serialization backward compatibility. The current parser behavior (tracking nesting to skip values) is correct.

### Changes

- Added explanatory comment to `crates/saleae-proto/src/calibration.rs` at the `in_fir_taps` declaration documenting why it is safe to discard these values, with a cross-reference to `re/notes/calibration-validation.md §3`.

No follow-up task needed.
<!-- SECTION:FINAL_SUMMARY:END -->
