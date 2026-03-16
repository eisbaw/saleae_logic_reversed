---
id: TASK-0206
title: 'RE: Decode FPGA register 0x1B (ANALOG_DEC_UPPER) purpose and bit layout'
status: Done
assignee: []
created_date: '2026-03-13 20:48'
updated_date: '2026-03-16 06:27'
labels:
  - reverse-engineering
  - fpga-registers
  - analog
dependencies: []
references:
  - 'doc/protocol/fpga-registers.md:225'
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
ANALOG_DEC_UPPER (register 0x1B) is written only for board revision 1. Rev 0 asserts analog_decimation_upper == 0. The computation formula and bit layout are unknown.

Approach: decompile the capture config builder in Ghidra, find where 0x1B is computed. Likely related to analog decimation ratio for higher sample rates.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Formula confirmed from capture-register-formulas.md: value = min(1, (analog_decimation >> 8) & 0xFF). Only written for board_revision==1. High byte of decimation, clamped to max 1. Always 0x00 for validated pcap configs. Updated fpga_regs.rs comment.
<!-- SECTION:FINAL_SUMMARY:END -->
