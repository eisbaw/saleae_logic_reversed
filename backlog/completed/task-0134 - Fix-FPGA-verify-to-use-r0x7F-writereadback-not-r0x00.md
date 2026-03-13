---
id: TASK-0134
title: Fix FPGA verify to use r0x7F write+readback (not r0x00)
status: Done
assignee: []
created_date: '2026-03-11 23:40'
labels:
  - bugfix
  - fpga
  - hardware-validated
dependencies: []
references:
  - crates/saleae-driver/src/fpga.rs
  - re/notes/init-sequence-pcap.md
  - re/ghidra/decompiled-xilinx-init.txt
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
FPGA programming verification was reading r0x00 and expecting 0xAA, but the pcap (Phase 3a, frames 1373-1384) and decompiled IsFpgaResponsive show the correct sequence:

1. WriteRegisters r0x7f=0xaa (twice)
2. ReadRegisters r0x7f -> expect 0xaa

Register 0x7F is a scratch/test register in the FPGA. The IsFpgaResponsive function (Ghidra 0x1d22a20, ELF 0x1c22a20) confirms: write 0xAA to r0x7F twice, read back, return true if 0xAA.

Fixed in fpga.rs program_fpga().
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Fixed. Changed fpga.rs to write 0xAA to r0x7F twice then readback, matching pcap and decompiled IsFpgaResponsive. Hardware-validated on local Logic Pro 8.
<!-- SECTION:FINAL_SUMMARY:END -->
