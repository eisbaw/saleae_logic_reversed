---
id: TASK-0206
title: 'RE: Decode FPGA register 0x1B (ANALOG_DEC_UPPER) purpose and bit layout'
status: To Do
assignee: []
created_date: '2026-03-13 20:48'
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
