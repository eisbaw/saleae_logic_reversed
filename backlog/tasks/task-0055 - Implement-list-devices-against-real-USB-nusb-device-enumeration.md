---
id: TASK-0055
title: Implement list-devices against real USB (nusb device enumeration)
status: Done
assignee: []
created_date: '2026-03-09 11:21'
updated_date: '2026-03-09 12:12'
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

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Enhanced list-devices with bcdDevice boot state detection and USB speed display.\n\n## Changes\n\n- **saleae-proto/device.rs**: Added `UsbSpeed` enum (Low/Full/High/Super/SuperPlus) and `BootState` enum (Bootloader/FirmwareRunning/Unknown) with `from_bcd_device()` constructor. Added both fields to `SaleaeDevice` struct.\n- **saleae-proto/lib.rs**: Re-exported `BootState` and `UsbSpeed` at crate root.\n- **saleae-usb/discovery.rs**: Populates boot_state from `DeviceInfo::device_version()` (bcdDevice) and usb_speed from `DeviceInfo::speed()` via `convert_speed()` mapping from nusb types.\n- **saleae-cli/commands.rs**: Displays boot state and USB speed in list-devices output.\n\n## Key design decisions\n\n- `UsbSpeed` is defined in saleae-proto (not re-exporting nusb::Speed) to keep proto free of USB backend dependencies.\n- `BootState::Unknown(u16)` variant handles unexpected bcdDevice values gracefully.\n- nusb::Speed is #[non_exhaustive], so convert_speed has a catch-all arm.\n\n## Hardware verification\n\nTested on Logic Pro 8 (21a9:1005) on nvdk-msdlab-09:\n- Boot state: Firmware running (bcdDevice=0.00)\n- USB speed: SuperSpeed (5 Gbit/s)\n- Serial: SERNUM\n\n41 tests pass including 4 new tests for BootState and UsbSpeed.
<!-- SECTION:FINAL_SUMMARY:END -->
