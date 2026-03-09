---
id: TASK-0033
title: Capture USB descriptors before firmware upload (FX3 bootloader state)
status: Done
assignee: []
created_date: '2026-03-09 00:29'
updated_date: '2026-03-09 06:20'
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

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
## Findings

Investigated the FX3 bootloader state on the Logic Pro 8 attached to nvdk-msdlab-09.

### Key Discovery: Two-Stage Boot Process

The FX3 does NOT expose the standard Cypress bootloader VID:PID (04b4:00f3). Instead, it boots from SPI flash into a minimal WestBridge firmware:

**Stage 1 - SPI Flash Boot:**
- VID:PID = 21a9:1005 (Saleae, not Cypress default)
- bcdDevice = 1.00
- Product = WestBridge, Manufacturer = Cypress
- SerialNumber = 0000000004BE (real hardware serial)
- USB 2.0 High Speed only

**Stage 2 - After Saleae firmware upload:**
- VID:PID = 21a9:1005 (same)
- bcdDevice = 0.00
- Product = Logic Pro, Manufacturer = Saleae
- SerialNumber = SERNUM (generic label)
- USB 3.0 SuperSpeed

### Implications for Rust Driver

1. bcdDevice reliably distinguishes boot states (1.00 = needs FW, 0.00 = ready)
2. No Cypress bootloader (04b4:00f3) handling needed
3. Real hardware serial available in SPI boot state at iSerialNumber=3
4. Firmware upload runs over USB 2.0 (SPI boot firmware is HS only)

### Limitations

Could not capture full lsusb -v in SPI boot state -- requires physical power cycling which is not possible remotely (device is on root hub without per-port power switching).

### Files Created
- re/notes/bootloader-descriptors.md -- full analysis with dmesg evidence
- Updated re/notes/usb-capture-setup.md with serial number correction

### Follow-up Tasks Created
- TASK-0051: Investigate real device serial number
- TASK-0052: Capture full lsusb -v in SPI boot state (requires physical access)
<!-- SECTION:FINAL_SUMMARY:END -->
