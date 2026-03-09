---
id: TASK-0034
title: Extract FpgaConstants numeric values from binary
status: Done
assignee: []
created_date: '2026-03-09 00:38'
updated_date: '2026-03-09 06:21'
labels:
  - re
  - fpga
  - constants
dependencies:
  - TASK-0011
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Extract the actual numeric values of:
- FpgaConstants::MinSamplesPerPacket_Altera
- FpgaConstants::MaxSamplesPerPacket_Altera
- FpgaConstants::MinSamplesPerPacket_Xilinx
- FpgaConstants::MaxSamplesPerPacket_Xilinx
- FpgaConstants::SamplesPerPacketMultiple_WithStats

These are referenced in assertion strings but the actual values need Ghidra decompilation of the functions that use them. Look at SetCaptureParameters and the assertion checking code to extract the constants.

Also extract MsoConstants values for trigger buffer sizes.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Covered by TASK-0023. All FpgaConstants values extracted: MinSamplesPerPacket_Xilinx=256, MaxSamplesPerPacket_Xilinx=7680, MinSamplesPerPacket_Altera=64, MaxSamplesPerPacket_Altera=6016, SamplesPerPacketMultiple_WithStats=256. See doc/protocol/capture-constants.md.
<!-- SECTION:FINAL_SUMMARY:END -->
