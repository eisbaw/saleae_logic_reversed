---
id: TASK-0151
title: Extract CaptureConfig device-specific constructor to reduce CLI boilerplate
status: Done
assignee: []
created_date: '2026-03-12 16:40'
updated_date: '2026-03-12 17:03'
labels:
  - driver
  - cli
  - capture
dependencies: []
references:
  - crates/saleae-driver/src/capture.rs
  - crates/saleae-cli/src/commands.rs
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Three CLI call sites construct CaptureConfig inline with nearly identical boilerplate (same total_device_channels, board_revision, is_pro, base_digital_rate, base_analog_rate). A `CaptureConfig::pro8_default(sample_rate, digital_channels, analog_channels)` constructor would reduce duplication and make device-specific assumptions explicit in one place.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added CaptureConfig::pro8() constructor with device-specific defaults. Replaced 3 CLI call sites with one-liners, eliminating ~30 lines of duplicated boilerplate.
<!-- SECTION:FINAL_SUMMARY:END -->
