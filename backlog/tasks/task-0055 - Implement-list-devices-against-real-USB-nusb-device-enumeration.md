---
id: TASK-0055
title: Implement list-devices against real USB (nusb device enumeration)
status: To Do
assignee: []
created_date: '2026-03-09 11:21'
labels:
  - rust
  - hardware-testing
  - usb
dependencies:
  - TASK-0054
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The list-devices CLI command must scan USB buses via nusb and identify Saleae devices by VID:PID.

Implementation:
1. Use nusb::list_devices() to enumerate all USB devices
2. Filter by Saleae VIDs (0x21a9, 0x0925) 
3. For each match, read bcdDevice to determine boot state (1.00 = bootloader, 0.00 = firmware running)
4. Print device type, serial, bus/port, USB speed, boot state
5. Handle permission errors gracefully (suggest udev rules)

Test against real hardware on nvdk-msdlab-09:
- Should detect the Logic Pro 8 (21a9:1005)
- Should show bcdDevice=0.00 if Saleae software already uploaded firmware
- Should show bcdDevice=1.00 if device is in SPI flash boot state

This is the first real hardware validation. Must work before anything else.
<!-- SECTION:DESCRIPTION:END -->
