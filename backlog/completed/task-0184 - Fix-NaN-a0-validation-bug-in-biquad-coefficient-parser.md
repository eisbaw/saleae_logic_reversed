---
id: TASK-0184
title: Fix NaN a0 validation bug in biquad coefficient parser
status: Done
assignee: []
created_date: '2026-03-13 05:36'
updated_date: '2026-03-13 07:28'
labels:
  - proto
  - bug
  - calibration
dependencies: []
references:
  - crates/saleae-proto/src/calibration.rs
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
In calibration.rs parse_sections, the a0 normalization check `(a0 - 1.0).abs() > 1e-10` evaluates to false when a0 is NaN (IEEE 754 NaN comparison semantics). A corrupted .cal file with NaN in the a0 position would silently pass validation. Fix: use `a0.is_nan() || (a0 - 1.0).abs() > 1e-10`. Also consider adding is_finite() checks on all coefficient values.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Fixed IEEE 754 NaN validation bug in `parse_sections()` in `crates/saleae-proto/src/calibration.rs`. The expression `(a0 - 1.0).abs() > 1e-10` evaluates to false when a0 is NaN because NaN comparisons always return false. Added `is_finite()` check on all 6 coefficients (b0, b1, b2, a0, a1, a2) per section before the a0 normalization check. Updated two existing tests that documented the bug to assert proper rejection. Added new test for infinity rejection. Commit: 1092d37.
<!-- SECTION:FINAL_SUMMARY:END -->
