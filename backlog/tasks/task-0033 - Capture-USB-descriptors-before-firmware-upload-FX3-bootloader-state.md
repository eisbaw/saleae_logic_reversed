---
id: TASK-0033
title: Capture USB descriptors before firmware upload (FX3 bootloader state)
status: To Do
assignee: []
created_date: '2026-03-09 00:29'
labels:
  - usb
  - hardware
  - descriptor
dependencies:
  - TASK-0010
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Power-cycle the Logic Pro 8 on nvdk-msdlab-09 and capture lsusb -v output BEFORE Saleae Logic 2 uploads firmware. This will reveal:
1. What VID:PID the FX3 bootloader exposes (likely 04b4:00f3 Cypress default)
2. What endpoints are available in bootloader mode (likely EP0 only)
3. Whether the device has any DFU-mode descriptors

This fills a gap in TASK-0010 where we only captured post-firmware descriptors.

Steps:
1. SSH to nvdk-msdlab-09
2. Stop any running Saleae Logic 2 process
3. Power-cycle the Logic Pro 8 (unplug/replug USB)
4. Immediately run: lsusb -v (before Saleae software can upload firmware)
5. Document the bootloader-state descriptors
6. Compare with post-firmware descriptors from TASK-0010
<!-- SECTION:DESCRIPTION:END -->
