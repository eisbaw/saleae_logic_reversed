---
id: TASK-0027
title: RE Logic 4/8 firmware mapping -- do non-Pro FX3 devices share Pro firmware?
status: Done
assignee: []
created_date: '2026-03-09 00:08'
updated_date: '2026-03-09 05:55'
labels:
  - firmware
  - fx3
  - investigation
dependencies:
  - TASK-0007
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-0007 found only 2 CY firmware images (Pro 8 and Pro 16) but 4 FX3-based devices exist (Logic 4, 8, Pro 8, Pro 16). Need Ghidra analysis of GetFirmwareForDeviceType to determine if Logic 4/8 share Pro firmware with runtime PID patching, or if their firmware is stored elsewhere.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Investigated GetFirmwareForDeviceType in libgraph_server_shared.so.

Key findings:
1. Logic 4 (PID 0x1003) is NOT FX3-based -- it uses FX2LP and receives Intel HEX firmware (group 2 at 0x39743d). The PRD incorrectly assumed FX3. Class hierarchy confirms: LogicStudentDevice inherits from LogicAnalyzerDevice (FX2 base), not Logic2FpgaDevice (FX3 base).

2. Logic 8 (PID 0x1004) shares the Pro 8 FX3 firmware image (at 0x79e7a0, PID 0x1005 embedded). After upload, the reseed mechanism patches the PID to 0x1004.

3. Logic Pro 8 (PID 0x1005) gets the Pro 16 FX3 firmware (at 0x7bbbb0, PID 0x1006 embedded). After upload, reseed patches PID to 0x1005.

4. Logic Pro 16 (DeviceType 12) has no embedded firmware fallback in GetFirmwareForDeviceType. It uses a custom firmware file path or throws an exception.

5. The two FX3 firmware images serve all FX3 devices. The reseed mechanism patches USB PIDs post-upload.

Documented in re/notes/logic4-8-firmware.md.
<!-- SECTION:FINAL_SUMMARY:END -->
