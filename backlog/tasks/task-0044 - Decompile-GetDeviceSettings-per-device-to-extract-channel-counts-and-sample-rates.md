---
id: TASK-0044
title: >-
  Decompile GetDeviceSettings() per device to extract channel counts and sample
  rates
status: Done
assignee: []
created_date: '2026-03-09 02:03'
updated_date: '2026-03-09 03:56'
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

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Extracted per-device channel counts, max sample rates, ADC ranges, and voltage transforms from libgraph_server_shared.so v2.4.40 and bundle.js.\n\nKey findings:\n- Logic 4: 12 MHz digital, 6 MHz analog, 4 channels (MOV immediates at 0x1c563e7)\n- Logic 8: 200 MHz digital, 20 MHz analog, 8 channels (MOV immediates at 0x1c3d1f6)\n- Logic Pro 8: 500 MHz digital, 50 MHz analog, 8 channels (MOV immediates at 0x1c44961)\n- Logic Pro 16: 500 MHz digital, 50 MHz analog, 16 channels (MOV immediates at 0x1c446e1)\n\nGetDeviceSettings() functions use XOR-obfuscated 65-byte blobs, making static extraction impractical. Instead, constants were extracted from device constructor MOV immediates and cross-referenced with JS bundle defaults.\n\nFunction at 0x1c44560 was misidentified -- it is GetPublicKey(), not GetDeviceSettings().\n\nDeliverables:\n- doc/devices/device-capabilities.md (confirmed documentation)\n- re/notes/device-settings.md (working notes)\n- re/ghidra/scripts/ExtractDeviceSettings.py (Ghidra script)"]
</invoke>
<!-- SECTION:FINAL_SUMMARY:END -->
