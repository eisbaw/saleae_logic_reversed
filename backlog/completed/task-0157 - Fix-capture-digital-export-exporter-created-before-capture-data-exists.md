---
id: TASK-0157
title: Fix capture-digital export -- exporter created before capture data exists
status: Done
assignee: []
created_date: '2026-03-12 22:27'
updated_date: '2026-03-12 23:23'
labels:
  - cli
  - bug
  - export
dependencies: []
references:
  - 'crates/saleae-cli/src/commands.rs:1106'
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
In capture_digital(), the SAL exporter is created and finish() called before the actual capture runs, so it gets an empty dataset. The export path needs restructuring: capture first, then create exporter with captured data.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Moved the digital export block in `capture_digital()` from before the capture (where it produced empty files) to after the capture completes. The fix disaggregates raw EP2 data into per-channel packed digital samples using the existing `Disaggregator`, then feeds them to the `DigitalExporter` (CSV or Binary) before calling `finish()`. Raw data is trimmed to cycle-size alignment before disaggregation; a warning is printed if no usable data is available.
<!-- SECTION:FINAL_SUMMARY:END -->
