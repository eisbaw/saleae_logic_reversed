---
id: TASK-0154
title: Replace panic!/assert! with DriverError returns in library code
status: Done
assignee: []
created_date: '2026-03-12 22:27'
updated_date: '2026-03-12 23:01'
labels:
  - driver
  - error-handling
  - quality
dependencies: []
references:
  - 'crates/saleae-driver/src/ep1.rs:262'
  - 'crates/saleae-driver/src/hmcad.rs:216'
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Multiple public functions panic on invalid input instead of returning errors: ep1.rs:262 (write_registers >31), ep1.rs:313 (read_registers), jesd.rs:131 (write_calibration_coefficients), hmcad.rs:216/299/385 (defaults/init/test_pattern). A driver library must not panic -- all should return DriverError variants.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Replaced all panic!/assert! in saleae-driver public library functions with Err(DriverError::InvalidArgument { .. }) returns. Added InvalidArgument variant to DriverError enum. Changed Hmcad1100Settings::defaults return type from Self to DriverResult<Self>. Converted 7 should_panic tests to proper error-assertion tests. All 342 unit tests + 21 integration tests pass, clippy clean.
<!-- SECTION:FINAL_SUMMARY:END -->
