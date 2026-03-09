---
id: TASK-0057
title: Implement firmware upload and re-enumeration against real hardware
status: To Do
assignee: []
created_date: '2026-03-09 11:21'
labels:
  - rust
  - hardware-testing
  - firmware
dependencies:
  - TASK-0055
  - TASK-0056
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Upload the embedded FX3 firmware to the Logic Pro 8 and handle re-enumeration.

Implementation:
1. Extract the Pro 8 firmware image from libgraph_server_shared.so (offset 0x0079e7a0, 119824 bytes)
   - Or add a `just extract-blobs` step as prerequisite and load from firmware/ dir
2. Open device in bootloader state (bcdDevice=1.00)
3. Upload firmware via vendor control transfers (bmRequestType=0x40, bRequest=0xA0)
4. Trigger execution with zero-length transfer to entry address 0x4000e020
5. Wait for device to disconnect and re-enumerate (~2-3 seconds)
6. Re-open the device (now bcdDevice=0.00, Product="Logic Pro", Manufacturer="Saleae")
7. Verify 3 bulk endpoints are available

Critical considerations:
- Device must be in bootloader state (bcdDevice=1.00). If Saleae software already loaded firmware, device is already at bcdDevice=0.00 and firmware upload is not needed.
- After upload, the USB device disappears briefly. Must poll for re-appearance.
- The firmware is RAM-only, safe to test (power cycle recovers).

Test: `just run-remote -- upload-firmware --firmware firmware/fw-fx3-pro8.img`

References: doc/protocol/fx3-firmware.md, re/notes/bootloader-descriptors.md
<!-- SECTION:DESCRIPTION:END -->
