---
id: TASK-0044
title: >-
  Decompile GetDeviceSettings() per device to extract channel counts and sample
  rates
status: To Do
assignee: []
created_date: '2026-03-09 02:03'
labels:
  - re
  - class-hierarchy
  - device-constants
dependencies:
  - TASK-0016
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
LogicGraduateDevice::GetDeviceSettings() and LogicProfessionalDevice::GetDeviceSettings() are virtual overrides that return per-device DeviceSettings containing channel counts, max sample rates, and performance option vectors. Decompile these in Ghidra to extract the exact numeric constants. The vtable addresses are: LogicGraduateDevice slot ~13 at 0x1c3f920, LogicProfessionalDevice slot ~13 at 0x1c44560.
<!-- SECTION:DESCRIPTION:END -->
