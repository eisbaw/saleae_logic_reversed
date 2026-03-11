---
id: TASK-0100
title: Implement device state machine (DSInitializing/DSReady/DSError)
status: Done
assignee: []
created_date: '2026-03-10 23:13'
updated_date: '2026-03-11 00:42'
labels:
  - rust
  - saleae-driver
  - implementation
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-0084 documented the device state machine as variant<DSInitializing, DSReady, DSError>. Our SaleaeDriver has a basic DeviceState but doesn't match the binary's state transitions.

Requirements:
- DeviceState enum: Disconnected, Initializing, Ready, Capturing, Error(String)
- State transition validation (only allow valid transitions, panic on invalid)
- Full disconnect/reconnect cycle with new DeviceInstanceId on error
- Polling-based device discovery (libusb_get_device_list equivalent via nusb)
- Device lifecycle: enumerate -> upload FW -> wait re-enum -> program FPGA -> post-init -> ready
- Error recovery: on USB error during capture, transition to Error, allow reconnect

Reference: re/notes/devices-manager.md, doc/state-machine/device-states.md
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Device state machine implemented with 6 states: Disconnected, Bootloader, Initializing, Ready, Capturing, Error(String).\n\nValid transitions enforced via can_transition_to() + transition_to(). DeviceInstanceId via global AtomicU64 (matches binary's DeviceInstanceId).\n\nIntegrated with driver methods: upload_firmware (Bootloader->Initializing), start_capture (Ready->Capturing), stop_capture (Capturing->Ready), set_error (Any->Error), set_disconnected (Error->Disconnected).\n\n20 new tests covering valid/invalid transitions, instance ID monotonicity, error recovery cycle. 332 workspace tests pass."
<!-- SECTION:FINAL_SUMMARY:END -->
