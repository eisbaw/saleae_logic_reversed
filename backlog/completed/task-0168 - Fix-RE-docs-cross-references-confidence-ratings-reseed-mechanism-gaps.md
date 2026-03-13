---
id: TASK-0168
title: 'Fix RE docs: cross-references, confidence ratings, reseed-mechanism gaps'
status: Done
assignee: []
created_date: '2026-03-12 22:28'
updated_date: '2026-03-13 00:39'
labels:
  - docs
  - RE
dependencies: []
references:
  - re/notes/reseed-mechanism.md
  - doc/protocol/capture-sequence.md
  - doc/protocol/fpga-registers.md
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
RE docs review found 20 issues: (1) reseed-mechanism.md missing initial seed constant 0x354b248e. (2) capture-sequence.md not cross-linked to init-sequence-pcap.md validation. (3) Multiple "Confirmed" ratings based only on decompilation should be "High". (4) capture-constants.md lists open questions that are resolved in fpga-registers.md. (5) analog-capture.md voltage transform marked "hypothesis" but confirmed by multiple sources. (6) calibration-format.md conflates data format with SIMD implementation. (7) fpga-registers.md register 0x0194 "TBD" with no follow-up task.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Fixed 7 issues across 6 RE doc files:

1. **reseed-mechanism.md**: Added initial seed constant 0x354b248e section with cross-reference to init-sequence-pcap.md
2. **capture-sequence.md**: Added cross-references section linking to init-sequence-pcap.md, ep1-commands.md, fpga-registers.md, capture-constants.md. Updated status header to note pcap partial validation.
3. **Confidence ratings**: Downgraded Confirmed->High across capture-sequence.md (6 items), capture-constants.md (7 items), analog-capture.md (2 items), reseed-mechanism.md (2 items) where evidence was Ghidra-only without hardware validation.
4. **capture-constants.md**: Added cross-references section and linked open question 3 to fpga-registers.md FPGA variant constraints.
5. **analog-capture.md**: Upgraded voltage transform from "hypothesis" to "Confirmed" -- now backed by Ghidra assertions + real .cal file data + Rust implementation (3 of 3 sources).
6. **calibration-format.md**: Separated SIMD implementation details into its own top-level section ("SIMD Filter Implementation (Saleae Binary Internals)") with clear framing that it's Saleae binary internals, not calibration data format.
7. **fpga-registers.md**: Register 0x0194 updated from bare "TBD" to document known info (ELF function address, single uint32 param, cross-ref to re/notes/fpga-glitch-filter.md).

Commit: d8967c4
<!-- SECTION:FINAL_SUMMARY:END -->
