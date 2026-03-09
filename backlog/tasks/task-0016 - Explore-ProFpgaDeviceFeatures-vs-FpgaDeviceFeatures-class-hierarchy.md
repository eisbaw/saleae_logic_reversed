---
id: TASK-0016
title: Explore ProFpgaDeviceFeatures vs FpgaDeviceFeatures class hierarchy
status: Done
assignee: []
created_date: '2026-03-08 22:45'
updated_date: '2026-03-09 02:05'
labels: []
dependencies:
  - TASK-0003
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Investigate the device feature class hierarchy to understand per-device differences.

Known classes:
- Fx2DeviceFeatures (legacy FX2 devices)
- FpgaDeviceFeatures (Logic 4, Logic 8)
- ProFpgaDeviceFeatures (Logic Pro 8, Logic Pro 16)
- Logic2FpgaDevice (common base?)

Hypothesis: these classes define per-device capabilities -- max sample rate, channel count, supported voltage levels, ADC type, available analog channels, etc. Understanding this hierarchy tells us what differs between device variants.

Investigate:
1. What vtable entries differ between Pro and non-Pro?
2. What constants/capabilities are defined per class?
3. How does the code select which class to instantiate? (based on USB PID?)
4. What is the relationship to bitstream selection?

Create follow-up tasks for each significant behavioral difference found.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Extracted complete C++ class hierarchy from stripped libgraph_server_shared.so binary using Itanium ABI RTTI structures and ELF relocation table analysis.

Key findings:
- Two parallel hierarchies confirmed: Device hierarchy (GenericDevice -> LogicProfessional16Device) for behavior, DeviceFeatures hierarchy (Fpga/ProFpga, Fx2/Fx3) for transport
- Hierarchies are composed, not inherited from each other
- LogicStudentDevice inherits from LogicAnalyzerDevice, NOT Logic2FpgaDevice (corrects prior hypothesis)
- ProFpgaDeviceFeatures overrides 2 of 8 virtual methods (slots [1] and [3], likely ReadRegisters and WriteRegisters)
- Fx3DeviceFeatures overrides 11 of 13 virtual methods from Fx2DeviceFeatures
- LogicProfessional8Device and LogicProfessional16Device each override exactly 2 methods from LogicProfessionalDevice (slots [1] and [13])
- DevicesManager uses VMI (virtual/multiple inheritance), suggesting multiple interface inheritance

Deliverables:
- re/notes/device-class-hierarchy.md - comprehensive findings document
- re/ghidra/scripts/TraceDeviceHierarchy.py - automated Ghidra script for hierarchy tracing

Follow-up tasks created: TASK-0043 (LogicStudentDevice anomaly), TASK-0044 (GetDeviceSettings decompilation), TASK-0045 (FpgaDeviceFeatures slot mapping)

Commit: d49552d
<!-- SECTION:FINAL_SUMMARY:END -->
