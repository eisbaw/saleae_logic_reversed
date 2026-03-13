---
id: TASK-0145
title: >-
  Unify capture code paths (full-init, capture-digital, capture-quick) into
  single capture pipeline
status: Done
assignee: []
created_date: '2026-03-12 14:12'
updated_date: '2026-03-12 15:32'
labels:
  - phase-2
  - refactor
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
There are currently three CLI capture commands with duplicated/divergent capture logic:
- full-init: hardcoded pcap registers, collects raw data, runs disaggregator, prints stats
- capture-digital: uses CaptureConfig/CaptureStream from capture.rs, supports CSV/binary export
- capture-quick: minimal capture with raw EP2 dump

These should converge into a single pipeline: init -> configure registers -> start -> stream (with queue) -> disaggregate -> export. The disaggregator and export should be common code regardless of which CLI entry point is used.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Implemented in commit b160d94. Added CaptureResult struct and run_capture() function to saleae-driver/capture.rs. All three CLI capture commands (full_init, capture_digital, capture_quick) now use run_capture() instead of inline capture logic. Added print_hex_dump() helper to replace 3 copies of identical hex display code. Net -29 lines (231 added, 260 removed).
<!-- SECTION:FINAL_SUMMARY:END -->
