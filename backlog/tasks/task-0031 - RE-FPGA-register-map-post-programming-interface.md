---
id: TASK-0031
title: RE FPGA register map (post-programming interface)
status: To Do
assignee: []
created_date: '2026-03-09 00:21'
labels:
  - fpga
  - registers
  - protocol
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Map the FPGA register addresses and their functions. The host uses 16-bit addressed, 32-bit valued registers via FpgaDeviceFeatures::ReadRegisters/WriteRegisters. Log format: "FPGA WriteRegister 0x{:04x} = 0x{:08x} ({})". Also includes CalculateDigitalConfigurationAddress functions for smart cable support. This register map is needed for capture configuration, ADC control, and trigger setup.
<!-- SECTION:DESCRIPTION:END -->
