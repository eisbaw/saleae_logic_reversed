---
id: TASK-0174
title: Replace placeholder transfer size constants with TransferConfig in capture.rs
status: Done
assignee: []
created_date: '2026-03-13 00:49'
updated_date: '2026-03-13 00:59'
labels:
  - driver
  - capture
  - quality
dependencies: []
references:
  - 'crates/saleae-driver/src/capture.rs:41'
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
capture.rs lines 41-51 have DEFAULT_BUF_SIZE (16384) and DEFAULT_SAMPLES_PER_CHUNK (1) marked as "placeholder". TransferConfig (added in TASK-0163) computes correct values from bandwidth. The old run_capture() still uses the placeholders. Either update run_capture to use TransferConfig, or mark the defaults as intentional for the synchronous fallback path.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added CaptureConfig::transfer_config() as single point of truth. Removed DEFAULT_TRANSFER_SIZE and buf_size parameter. All three capture paths now compute proper transfer sizes from bandwidth.
<!-- SECTION:FINAL_SUMMARY:END -->
