---
id: TASK-0201
title: >-
  Investigate analog bit inversion (XOR 0xFF) and Logic 8 deinterleave
  uncertainty
status: Done
assignee: []
created_date: '2026-03-13 10:09'
updated_date: '2026-03-13 11:29'
labels:
  - RE
  - EP2
  - Ghidra
dependencies: []
references:
  - crates/saleae-proto/src/disaggregator.rs
  - re/notes/disaggregator-interleaving.md
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Two EP2 data format uncertainties for non-Pro devices:

1. **Analog bit inversion** (FUN_02eae7f0): Conditional bitwise NOT on analog data. Hypothesis: offset-binary to two's-complement conversion for certain ADCs (HMCAD1100?). Need to determine which ADC types / hardware revisions trigger this.

2. **Logic 8 deinterleave**: The NonPro deinterleaver has a "shift mode" for Logic 4 (conditional left-shift by field & 0x1f bits). Purpose unclear — channel grouping? data alignment? The Logic 8 transform is marked "still uncertain" in disaggregator.rs.

Both need Ghidra decompilation of the selection logic to understand when each path activates.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Bit inversion trigger condition documented
- [ ] #2 Logic 4 shift mode purpose identified
- [ ] #3 Logic 8 deinterleave confidence upgraded or corrected
- [ ] #4 Disaggregator code updated with findings
<!-- AC:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Resolved two EP2 data format uncertainties via Ghidra decompilation:

1. **Analog bit inversion (XOR 0xFF)**: Decompiled FUN_02eae7f0 and FUN_02eadc90. The XOR 0xFF is a streaming pipeline feature for polarity/edge tracking across chunks in the DisaggregatorNode graph, NOT an ADC encoding difference. FUN_02eadc90 is a 2826-byte bit-level edge detector. Not relevant to raw deinterleaving or Pro 8 (AD9637).

2. **Logic 8 transform (FUN_01bf1ff0)**: Decompiled and identified as a LUT-based 8x8 bit matrix transpose. The FPGA packs Logic 8 data with bit-level interleaving (bit N of byte M = sample M from channel N). The transpose converts this to per-channel byte streams. Previous implementation incorrectly used byte-swap.

Code changes:
- Added `bit_transpose_8x8()` public function
- Rewrote NonPro deinterleaver to use 8-byte groups with bit transpose
- Fixed `cycle_bytes()` to return 8 for NonPro (was incorrectly 16)
- Updated confidence comments from "uncertain" to "Confirmed"
- All 956 workspace tests pass

Commit: deabe0d
<!-- SECTION:FINAL_SUMMARY:END -->
