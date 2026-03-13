---
id: TASK-0148
title: Add early-stop indicator to CaptureResult (full vs partial capture)
status: Done
assignee: []
created_date: '2026-03-12 16:40'
updated_date: '2026-03-12 16:55'
labels:
  - driver
  - capture
dependencies: []
references:
  - crates/saleae-driver/src/capture.rs
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
CaptureResult currently returns partial data silently when EP2 errors cause the read loop to break early. The caller cannot distinguish "got all the data we asked for" from "EP2 broke halfway through." Add a field or enum (e.g., `completed_normally: bool` or a `CaptureOutcome` enum) so callers can make informed decisions about the data quality.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added `completed_normally: bool` to CaptureResult, set false when EP2 read loop breaks early on error. All 3 CLI call sites print a warning when capture ended early.
<!-- SECTION:FINAL_SUMMARY:END -->
