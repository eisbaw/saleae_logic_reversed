---
id: TASK-0150
title: Pre-allocate raw_data buffer in run_capture based on expected throughput
status: Done
assignee: []
created_date: '2026-03-12 16:40'
updated_date: '2026-03-12 16:59'
labels:
  - driver
  - performance
  - capture
dependencies: []
references:
  - crates/saleae-driver/src/capture.rs
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
run_capture() uses `Vec::new()` for raw_data without pre-allocation. For multi-second captures at high throughput, this hits multiple reallocations. A `Vec::with_capacity()` estimate based on duration and buf_size would reduce allocation overhead in the streaming data path.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Pre-allocate raw_data with capacity estimated from buf_size * duration, capped at 256MB. Avoids repeated reallocations during high-throughput captures.
<!-- SECTION:FINAL_SUMMARY:END -->
