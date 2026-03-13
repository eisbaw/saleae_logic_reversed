---
id: TASK-0193
title: >-
  RE: Decompile FPGA register fields for CAPTURE_CONTROL, SCOPE_CONFIG_IDs,
  INIT_CONFIG
status: Done
assignee: []
created_date: '2026-03-13 05:36'
updated_date: '2026-03-13 09:17'
labels:
  - RE
  - Ghidra
  - fpga
dependencies: []
references:
  - doc/protocol/fpga-registers.md
  - re/notes/fpga-register-map.md
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Several FPGA registers have incomplete documentation: (1) CAPTURE_CONTROL (0x0040) -- bits [3:0], [7], [31:8] unknown. Need to decompile all writers. (2) SCOPE_CONFIG_ID registers (0x0044-0x0050) -- values unknown. Decompile SetConfigurationIds. (3) INIT_CONFIG (0x00E0) -- value written post-programming unknown. (4) TRIGGER_TICK_SIZE (0x0094) -- clock rate constant unknown. (5) DIGITAL_VOLTAGE_THRESHOLD (0x0104) -- secondary write mechanism (I2C DAC?) unconfirmed.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Decompiled all 5 target register writers. CAPTURE_CONTROL full bit layout, INIT_CONFIG formula, TRIGGER_TICK_SIZE computation, DIGITAL_VOLTAGE_THRESHOLD per-channel registers 0x01B0-0x01BC all documented.
<!-- SECTION:FINAL_SUMMARY:END -->
