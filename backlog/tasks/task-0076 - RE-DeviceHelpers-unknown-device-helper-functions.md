---
id: TASK-0076
title: RE DeviceHelpers -- unknown device helper functions
status: Done
assignee: []
created_date: '2026-03-10 12:31'
updated_date: '2026-03-10 17:49'
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
Reverse engineered DeviceHelpers.cpp. The module contains exactly 2 named functions:

1. **DeviceCalculateMixRate** (ELF 0x1c19fa0, 317 bytes) - Computes digital/analog interleave ratios for mixed-mode captures using GCD. Handles three modes: digital-only, analog-only, and mixed. Validates GPIF width (16/32) and ADC bits (10/12) combinations. Called by 5 sites across Logic2FpgaDevice.cpp, DeviceSettings.cpp, logic_device_node.cpp, and internally by DeviceCalculateSamplesCollected.

2. **DeviceCalculateSamplesCollected** (ELF 0x1c1a130, 329 bytes) - Converts raw byte count to sample count using MixRate outputs and LCM-based consistency check. Has zero external callers (likely dead code).

Total module size: ~726 bytes. Documentation written to re/notes/device-helpers.md with full parameter analysis, algorithm reconstruction, Rust implementation sketches, and caller cross-references.
<!-- SECTION:FINAL_SUMMARY:END -->
