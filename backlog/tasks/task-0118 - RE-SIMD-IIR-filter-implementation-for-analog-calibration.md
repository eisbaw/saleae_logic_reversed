---
id: TASK-0118
title: RE SIMD IIR filter implementation for analog calibration
status: Done
assignee: []
created_date: '2026-03-11 07:54'
updated_date: '2026-03-12 03:09'
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

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Verification complete. Our biquad.rs implementation matches the binary with one fix applied:

**Filter topology**: Direct Form I, cascaded SOS -- MATCHES. The binary converts DF1 SOS to companion matrix form for SIMD parallelism, but the mathematical operation is identical to our scalar DF1 cascade.

**Coefficient format**: Stored as f64 (double) in .cal file and binary's calibration storage (0xD8 bytes = 27 doubles = 5 sections x 5 coefficients + group_delay + impulse_length). Cast to f32 only at SIMD filter construction. Our f64 throughout is more accurate than the binary.

**Number of sections**: 5 SOS biquad sections confirmed. Section 0 is first-order (b2=0, a2=0). Total filter is 9th order.

**SIMD precision**: Binary uses f32 for SIMD processing. Our f64 scalar is equivalent but more precise. Acceptable difference -- the binary's f32 is the limiting factor.

**Fix applied**: process_block_i16() was using `.round()` (round-to-nearest) but the binary uses `cvttss2si` (truncation toward zero). Changed to `y as i32` which truncates toward zero in Rust, matching the binary. This was a +/-1 LSB error (~5mV on 12-bit ADC with 20V span). Added regression test `process_block_i16_truncates_toward_zero`.

**Saturation semantics**: `packssdw` saturates i32->i16, matched by our `.clamp(i16::MIN, i16::MAX)`. Correct.

All 404 saleae-proto tests pass.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Verified biquad.rs filter implementation against decompiled binary. Fixed one discrepancy: process_block_i16() used round-to-nearest but binary uses truncation-toward-zero (cvttss2si). Changed to truncation, added regression test. All other aspects (DF1 topology, 5 SOS sections, f64 coefficients, packssdw saturation) confirmed matching.
<!-- SECTION:FINAL_SUMMARY:END -->
