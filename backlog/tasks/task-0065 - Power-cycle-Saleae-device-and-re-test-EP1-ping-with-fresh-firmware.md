---
id: TASK-0065
title: Power-cycle Saleae device and re-test EP1 ping with fresh firmware
status: To Do
assignee: []
created_date: '2026-03-09 13:06'
labels:
  - hardware-testing
  - blocking
dependencies:
  - TASK-0058
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The EP1 command channel implementation (TASK-0058) is complete and passes all unit tests, but hardware testing against the Logic Pro 8 on nvdk-msdlab-09 failed because the firmware's command processing loop is not consuming data from the EP1 DMA buffer. Both nusb (Rust) and pyusb (Python/libusb) exhibit the same behavior: first bulk OUT succeeds, second always hangs.

Root cause: The FX3 firmware is in a stale state from a previous session. The FX3 stores firmware in volatile RAM, so a physical power cycle (unplug/replug) resets it to bootloader mode.

Steps:
1. Physically unplug and replug the Saleae Logic Pro 8 on nvdk-msdlab-09
2. Verify device appears in bootloader mode: `just run-remote list-devices` should show bcdDevice=0x0100
3. Upload firmware: `just run-remote upload-firmware --firmware firmware/fw-fx3-pro8.img`
4. Wait for re-enumeration (bcdDevice=0x0000)
5. Immediately test EP1: `just run-remote ep1-test`
6. If ep1-test passes steps 1-4, test the full ping: `just run-remote ping`
7. Document results

This requires physical access to the USB cable on nvdk-msdlab-09. If the lab is in a remote datacenter, coordinate with lab ops to power-cycle the USB port or replug the device.

If ping succeeds after fresh firmware upload, the TASK-0058 code is correct. If it still fails, investigate the FX3 DMA channel configuration and firmware command loop architecture.
<!-- SECTION:DESCRIPTION:END -->
