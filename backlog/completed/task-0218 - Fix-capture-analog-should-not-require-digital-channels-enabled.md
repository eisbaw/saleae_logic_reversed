---
id: TASK-0218
title: 'Fix capture-analog: should not require digital channels enabled'
status: Done
assignee: []
created_date: '2026-03-13 21:30'
updated_date: '2026-03-15 09:05'
labels:
  - bug
  - capture
  - cli
dependencies: []
references:
  - crates/saleae-driver/src/capture.rs
  - crates/saleae-cli/src/commands.rs
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Running `capture-analog` with only analog channels fails with:
"invalid capture config: at least one digital channel must be enabled"

Analog-only capture should work without requiring any digital channels. The CaptureConfig validation is too strict — it mandates digital channels even for analog-only mode.

Reproduce: `just hw-capture-analog CHANNELS=0,1`
Expected: analog capture runs with channels 0,1
Actual: error about digital channels
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Changed CaptureConfig validation to allow analog-only captures (no digital channels required). Analog-only EP2 data path still needs investigation -- capture starts but EP2 times out with zero bytes.
<!-- SECTION:FINAL_SUMMARY:END -->
