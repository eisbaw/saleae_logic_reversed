---
id: TASK-0071
title: RE USB 2.0 fallback handling and speed negotiation
status: Done
assignee: []
created_date: '2026-03-10 12:23'
updated_date: '2026-03-10 16:39'
labels:
  - re
  - ghidra
  - usb
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Reverse engineer how the driver handles USB 2.0 vs 3.0 connections. Logic Pro devices support both but with different constraints. Need:

- Sample rate limits on USB 2.0 vs 3.0
- Power budget differences (500mA vs 900mA)
- SetOverrideUsb2PowerLimit command usage
- Packet size differences (62-byte vs 509-byte chunks for FPGA programming)
- Channel count restrictions on USB 2.0
- How the driver detects USB speed and adjusts configuration
- Any USB 2.0-specific initialization differences

Important for users who connect through USB 2.0 hubs or ports.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
USB speed enum: 0=Full, 1=High(USB2), 2=Super(USB3), stored at UsbDevice+0x20. FPGA chunk sizes: non-Pro always 62 bytes, Pro uses 508 (USB2) or 1020 (USB3). WriteConfigurationData differs: non-Pro 2-byte header no ack, Pro 3-byte header with 1-byte ack. Transfer alignment: 4096 (USB2) vs 16384 (USB3). Pro 16 channels 8-15 lose analog on USB 2.0. 27-entry GPIF table maps {device_type, bus_width, usb_speed} to states. Created re/notes/usb2-fallback.md (519 lines) and 2 Ghidra scripts.
<!-- SECTION:FINAL_SUMMARY:END -->
