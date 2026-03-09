---
id: TASK-0029
title: RE FX2 Intel HEX firmware mapping -- which image goes to which legacy device?
status: Done
assignee: []
created_date: '2026-03-09 00:08'
updated_date: '2026-03-09 08:14'
labels:
  - firmware
  - fx2
  - investigation
dependencies:
  - TASK-0007
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-0007 found 4 Intel HEX firmware images embedded in the .so for FX2/FX2LP devices (Logic original 0925:3881, Logic 16 21a9:1001). Need to trace IHexFirmwareImageDescriptor path and map the 4 images to specific devices. Lower priority since FX2 devices are legacy.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Resolved by TASK-0027 firmware mapping analysis. The GetFirmwareForDeviceType jump table maps: DeviceType 1 (Original Logic, 0925:3881) -> IHex group 3, DeviceType 4 (Logic 16, 21a9:1001) -> IHex group 4, DeviceType 5 (Logic 4, 21a9:1003) -> IHex group 2. The 4th IHex image (group 1) is likely a variant or second-stage loader. See re/notes/logic4-8-firmware.md.
<!-- SECTION:FINAL_SUMMARY:END -->
