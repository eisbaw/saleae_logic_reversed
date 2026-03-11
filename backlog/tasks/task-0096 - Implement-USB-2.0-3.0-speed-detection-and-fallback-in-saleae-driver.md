---
id: TASK-0096
title: Implement USB 2.0/3.0 speed detection and fallback in saleae-driver
status: Done
assignee: []
created_date: '2026-03-10 23:13'
updated_date: '2026-03-11 00:32'
labels:
  - rust
  - saleae-driver
  - implementation
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-0071 documented USB 2.0 vs 3.0 differences. Implement speed-aware configuration.

Requirements:
- Detect USB speed after enumeration (Full=0, High=1, Super=2)
- FPGA chunk sizes: non-Pro 62 bytes, Pro 508 (USB2) / 1020 (USB3)
- WriteConfigurationData protocol: non-Pro 2-byte no-ack, Pro 3-byte with-ack
- Pro 16 channels 8-15 lose analog capability on USB 2.0
- Adjust max sample rates based on USB speed
- GPIF table selection (27 entries) based on speed + device type
- CaptureConfig validation: reject configs exceeding USB bandwidth

Reference: re/notes/usb2-fallback.md
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
USB 2.0/3.0 speed detection and fallback implemented.\n\nSpeed-aware DeviceProfile: max_analog_rate split into usb3/usb2 variants (Pro: 100/50 MHz), available_analog_channels reduces Pro 16 from 16 to 8 on USB 2.0. validate_config enforces speed limits.\n\nGPIF table: 27-entry lookup in saleae-proto/src/gpif.rs matching RE'd _INIT_55 at ELF 0x1dd9630. GpifBusWidth enum (Narrow/Medium/Wide), select_gpif_config dispatches by device type + speed.\n\nNon-Pro WriteConfigurationData: 8-bit length header, max 62 bytes/chunk, no ACK. write_config_data_for_device dispatches Pro vs non-Pro based on FpgaType.\n\n449 tests pass, clippy clean."
<!-- SECTION:FINAL_SUMMARY:END -->
