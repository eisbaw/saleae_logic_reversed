---
id: TASK-0186
title: Convert sample_rate.rs panicking asserts to Result returns
status: Done
assignee: []
created_date: '2026-03-13 05:36'
updated_date: '2026-03-13 07:32'
labels:
  - proto
  - error-handling
  - quality
dependencies: []
references:
  - 'crates/saleae-proto/src/sample_rate.rs:639'
  - 'crates/saleae-cli/src/commands.rs:1411'
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
sample_rate.rs:639,656 use assert!(!available.is_empty()) in public functions select_closest_rate() and select_closest_index(). These are called from the capture pipeline, and a panic kills the process instead of returning a recoverable error. Convert to ProtoError returns. Also fix CLI commands.rs:1411 unwrap() on cal.find_voltage_range(ch) which panics without error context.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Converted select_closest_rate() and select_closest_index() from panicking assert!() to Result<_, ProtoError> returns with new EmptySelection variant. Replaced unwrap() on cal.find_voltage_range() in CLI capture_analog with let-else that prints diagnostic and continues. All 8 select_closest tests updated and passing. Full test suite green.
<!-- SECTION:FINAL_SUMMARY:END -->
