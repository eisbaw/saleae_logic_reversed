---
id: TASK-0115
title: Implement USB hotplug device management with reconnect
status: Done
assignee: []
created_date: '2026-03-11 05:59'
updated_date: '2026-03-11 06:56'
labels:
  - rust
  - saleae-driver
  - usb
dependencies: []
references:
  - re/notes/devices-manager.md
  - crates/saleae-usb/src/discovery.rs
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The driver currently requires restart to detect new devices. RE notes (devices-manager.md) document DevicesManager as a polling-based system with reconnect support.

From RE:
- Uses libusb_hotplug_register_callback (but we use nusb which has its own hotplug API)
- CheckForConnectionThread polls for device changes
- DeviceReconnectTimeout for graceful handling
- "Device instance {} being forcibly removed" on disconnect

Implementation:
1. Add a DeviceManager struct that maintains a list of connected devices
2. Use nusb's hotplug API (or polling fallback) to detect connect/disconnect
3. On connect: auto-identify device type, create DeviceHandle
4. On disconnect: clean up resources, notify caller
5. Support re-initialization when a device reconnects (firmware re-upload needed)
6. Add saleae-cli `monitor` subcommand that prints connect/disconnect events

Reference: re/notes/devices-manager.md
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Created device_manager.rs (750 lines) with: DeviceEnumerator trait for testability, DeviceManager&lt;E&gt; with poll()-based connect/disconnect detection, TrackedDevice with 4-state lifecycle (Discovered/Initializing/Ready/Lost), DeviceEvent enum. CLI monitor subcommand with configurable interval. 15 tests using MockEnumerator. No new dependencies, no async, no threads -- matches Saleae's polling approach.
<!-- SECTION:FINAL_SUMMARY:END -->
