---
id: TASK-0074
title: RE LogicGraduateDevice -- unknown device class
status: Done
assignee: []
created_date: '2026-03-10 12:31'
updated_date: '2026-03-10 17:22'
labels:
  - re
  - ghidra
  - device-class
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Source path leaked: `/workspace/monorepo/graph-io/devices/logic_interface/src/LogicGraduateDevice.cpp`

This is a device class we haven't seen before. Not Logic, not Pro, not Student, not MSO. Could be:
- An unreleased product tier
- A renamed/rebranded device
- An educational edition (between Student and Pro?)
- A regional variant

Investigation steps:
1. Search binary for strings containing "Graduate" to find assertion messages
2. Find xrefs to those strings to locate the class vtable
3. Decompile key virtual methods (GetDeviceSettings, channel counts, sample rates)
4. Determine VID:PID and how it maps to our known device list
5. Check if it shares firmware/bitstream with any known device

This may reveal a device we need to support or can safely ignore.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
LogicGraduateDevice = Logic 8 (DeviceType 5). 42-entry vtable at ELF 0x303e950. Key findings:\n- Uses FpgaDeviceFeatures (non-Pro FPGA interface, 8-bit registers via EP1 0x80/0x81)\n- 8 digital + 8 analog channels\n- Constructor at ELF 0x1ac6160 sets up channel arrays and FPGA config\n- Shares firmware/bitstream infrastructure with Logic 4 (Student) but different channel counts\n- Simpler AFE configuration than Pro models (no per-channel gain stages)\n- Sample rates constrained by non-Pro FPGA limits\n- 42 virtual methods decompiled, vtable extraction script created\n- Full analysis in re/notes/logic-graduate-device.md (541 lines)\nCommit: 7337619"
<!-- SECTION:FINAL_SUMMARY:END -->
