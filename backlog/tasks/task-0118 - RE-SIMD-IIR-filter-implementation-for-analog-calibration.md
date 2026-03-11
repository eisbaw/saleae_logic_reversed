---
id: TASK-0118
title: RE SIMD IIR filter implementation for analog calibration
status: To Do
assignee: []
created_date: '2026-03-11 07:54'
labels:
  - re
  - ghidra
  - analog
  - calibration
dependencies: []
references:
  - crates/saleae-proto/src/biquad.rs
  - re/notes/analog-capture-pipeline.md
  - re/notes/calibration-system.md
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Strings reveal SimdIirFilterPrototype and SimdPrototype_Static2By3IirFilter -- SIMD-optimized IIR biquad filters in the analog calibration pipeline. We implemented basic biquad filters (biquad.rs) but haven't verified they match the actual binary's implementation.

Investigation:
1. Search for SimdIirFilter, SimdPrototype, Static2By3 strings and decompile referencing functions
2. Determine the exact filter topology (direct form I/II, transposed?)
3. Check coefficient format (f32 or f64? how many stages?)
4. Compare against our biquad.rs implementation
5. Check if the SIMD optimization affects numerical precision (SSE vs scalar)

This matters for analog sample accuracy -- wrong filter implementation = wrong voltages.
<!-- SECTION:DESCRIPTION:END -->
