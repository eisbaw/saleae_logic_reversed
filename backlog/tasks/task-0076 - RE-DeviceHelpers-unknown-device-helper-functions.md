---
id: TASK-0076
title: RE DeviceHelpers -- unknown device helper functions
status: Done
assignee: []
created_date: '2026-03-10 12:31'
updated_date: '2026-03-10 17:50'
labels:
  - re
  - ghidra
  - device-helpers
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Source path leaked: `/workspace/monorepo/graph-io/devices/logic_interface/src/DeviceHelpers.cpp`

Unexplored helper module in the device layer. Could contain:
- Device identification / feature detection
- Hardware revision handling
- USB speed detection helpers
- Channel configuration helpers
- Common initialization subroutines shared across device classes

Investigation steps:
1. Search binary for strings containing "DeviceHelpers" or related assertion messages
2. Identify functions compiled from this translation unit
3. Decompile and document what helper operations exist
4. Check if any of these are called from device init paths we've already traced
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
DeviceHelpers.cpp contains only 2 functions (not the device identification/revision helpers hypothesized):\n\n1. DeviceCalculateMixRate (ELF 0x1c19fa0, 317 bytes) -- computes digital/analog interleave ratios using GCD. Three modes: digital-only, analog-only, mixed. Validates GPIF width (16/32) against ADC bits (10/12) -- only 3 valid combos. 5 callers across Logic2FpgaDevice.cpp, DeviceSettings.cpp, logic_device_node.cpp.\n\n2. DeviceCalculateSamplesCollected (ELF 0x1c1a130, 329 bytes) -- converts byte counts to sample counts using LCM. Zero external callers (likely dead code or indirect).\n\nThe device identification, revision handling, and USB speed functions are elsewhere (Logic2FpgaDevice.cpp, DeviceSettings.cpp).\n\nAnalysis: re/notes/device-helpers.md"
<!-- SECTION:FINAL_SUMMARY:END -->
