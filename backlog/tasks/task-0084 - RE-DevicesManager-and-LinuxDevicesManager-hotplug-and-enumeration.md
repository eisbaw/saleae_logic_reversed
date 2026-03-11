---
id: TASK-0084
title: RE DevicesManager and LinuxDevicesManager -- hotplug and enumeration
status: Done
assignee: []
created_date: '2026-03-10 12:32'
updated_date: '2026-03-10 19:39'
labels:
  - re
  - ghidra
  - device-class
dependencies: []
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Source paths leaked:
- `/workspace/monorepo/graph-io/devices/devices_manager/src/DevicesManager.cpp`
- `/workspace/monorepo/graph-io/devices/devices_manager/src/LinuxDevicesManager.cpp`

The device manager handles USB hotplug detection and device lifecycle. Reveals:
- How devices are discovered (libusb hotplug API usage)
- Device factory pattern (VID:PID → device class mapping)
- Multi-device handling (multiple Logics connected simultaneously)
- Device state tracking across connect/disconnect cycles
- Error recovery on USB disconnect during capture

Investigation steps:
1. Search for strings like "DevicesManager", "hotplug", "device_added", "device_removed"
2. Decompile the Linux-specific enumeration code
3. Map VID:PID to device class constructor dispatch
4. Document the hotplug event flow
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
DevicesManager uses libusb polling (libusb_get_device_list), NOT hotplug callbacks. Simplifies Rust implementation.\n\nComplete VID:PID dispatch in GetAllDevices (ELF 0x1c18240):\n- 21a9:1003 = Logic 4, 21a9:1004 = Logic 8, 21a9:1005 = Logic Pro 8\n- 21a9:1006 = Logic Pro 16, 21a9:1001 = Logic 16, 0925:3881 = Original Logic\n- 21a9:1007 = Mso (DeviceType 12) -- previously unknown PID confirmed\n\nDevice state machine: variant<DSInitializing, DSReady, DSError> with clean state transitions. Full disconnect/reconnect cycle with new DeviceInstanceId on error.\n\nObject layout: mutex (+0x90), libusb_context (+0x130), device map (+0x138), device refs (+0x150).\n\nSimulation support: AddFakeDevice/RemoveFakeDataDevice for dev testing.\n\nAnalysis: re/notes/devices-manager.md (364 lines)"
<!-- SECTION:FINAL_SUMMARY:END -->
