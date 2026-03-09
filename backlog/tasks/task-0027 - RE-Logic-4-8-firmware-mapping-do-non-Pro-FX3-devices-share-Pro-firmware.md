---
id: TASK-0027
title: RE Logic 4/8 firmware mapping -- do non-Pro FX3 devices share Pro firmware?
status: To Do
assignee: []
created_date: '2026-03-09 00:08'
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
