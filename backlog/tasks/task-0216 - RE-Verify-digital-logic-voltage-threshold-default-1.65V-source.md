---
id: TASK-0216
title: 'RE: Verify digital logic voltage threshold default 1.65V source'
status: To Do
assignee: []
created_date: '2026-03-13 20:49'
labels:
  - reverse-engineering
  - capture
dependencies: []
references:
  - 'crates/saleae-driver/src/capture.rs:141'
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
CaptureConfig uses 1.65V as the default digital logic voltage threshold. This value is used in practice but its origin from the Saleae binary has not been explicitly verified. 1.65V is half of 3.3V (standard LVCMOS), so it's a reasonable default, but should be confirmed against the binary.

Approach: search for 1.65 or 0x3FA66666 (IEEE 754 for 1.65f) in the binary. Or search for the voltage threshold setter in the decompilation.
<!-- SECTION:DESCRIPTION:END -->
