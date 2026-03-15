---
id: TASK-0205
title: >-
  RE: Decode FPGA register 0x1A (ADC_CONFIG) bit field layout for Pro/JESD
  devices
status: Done
assignee: []
created_date: '2026-03-13 20:48'
updated_date: '2026-03-15 22:31'
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

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Decoded from Ghidra decompilation: r0x1a = ADC_CONFIG based on raw digital divider (500M/digitalRate) before /5 step. Values: raw==2 (250MHz) -> 1, raw==4 (125MHz) -> 2, else -> 0. Validated against pcap config 6 (250MHz digital). Implemented in capture.rs.
<!-- SECTION:FINAL_SUMMARY:END -->
