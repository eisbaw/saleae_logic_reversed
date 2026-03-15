---
id: TASK-0205
title: >-
  RE: Decode FPGA register 0x1A (ADC_CONFIG) bit field layout for Pro/JESD
  devices
status: To Do
assignee: []
created_date: '2026-03-13 20:48'
labels:
  - reverse-engineering
  - fpga-registers
  - adc
dependencies: []
references:
  - 'doc/protocol/fpga-registers.md:224'
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
ADC_CONFIG register (0x1A) is written by SetCaptureParameters for Pro/JESD devices. Exact bit field layout is not documented. Purpose inferred as ADC configuration but specific fields are unknown.

Approach: decompile SetCaptureParameters in Ghidra, trace the value construction for register 0x1A writes. Compare with AD9637 datasheet registers.
<!-- SECTION:DESCRIPTION:END -->
