---
id: TASK-0045
title: Map FpgaDeviceFeatures virtual method slots to named functions
status: To Do
assignee: []
created_date: '2026-03-09 02:03'
labels:
  - re
  - class-hierarchy
  - fpga
dependencies:
  - TASK-0016
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
FpgaDeviceFeatures has 8 virtual methods, of which ProFpgaDeviceFeatures overrides slots [1] and [3]. Known methods: ReadRegisters, WriteRegisters, WriteConfigurationData, WriteRegisterBit. Decompile the 8 methods in Ghidra to determine which slot maps to which function, and understand what the Pro overrides change (likely register addressing width or bus protocol).
<!-- SECTION:DESCRIPTION:END -->
