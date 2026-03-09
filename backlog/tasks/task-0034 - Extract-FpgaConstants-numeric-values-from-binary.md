---
id: TASK-0034
title: Extract FpgaConstants numeric values from binary
status: To Do
assignee: []
created_date: '2026-03-09 00:38'
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
