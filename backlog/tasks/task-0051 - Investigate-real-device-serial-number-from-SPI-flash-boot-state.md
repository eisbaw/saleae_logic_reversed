---
id: TASK-0051
title: Investigate real device serial number from SPI flash boot state
status: Done
assignee: []
created_date: '2026-03-09 06:20'
updated_date: '2026-03-09 08:49'
labels:
  - usb
  - hardware
  - serial-number
dependencies:
  - TASK-0033
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-0033 discovered that in the SPI flash boot state (before firmware upload), the FX3 exposes the real hardware serial number (0000000004BE) at iSerialNumber=3. After firmware upload, this is replaced with the generic "SERNUM" label at iSerialNumber=4.

Questions to investigate:
1. Is the real serial number accessible via I2C EEPROM after firmware loads? (cross-ref i2c-eeprom-protocol.md)
2. Is 0000000004BE a Cypress serial number or a Saleae-assigned serial?
3. Does the calibration system use the real serial to fetch device-specific calibration data?
4. Should the Rust driver capture the real serial before uploading firmware?
5. Does the Saleae Logic 2 host code read this serial before firmware upload?
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Investigated the device serial number lifecycle across three identifiers:

1. **SPI flash serial (0000000004BE)**: Exposed at iSerialNumber=3 during FX3 boot state. Likely a Cypress/factory default (decimal 1214, not in calibration DB). Not used by host application.

2. **"SERNUM" literal**: Hardcoded UTF-16LE string in uploaded FX3 firmware blobs at two offsets (0x7a69f4 and 0x7c3e06). A fixed placeholder, never a real serial.

3. **EEPROM device ID (764051B51BD4D9F1)**: Read via I2C tunnel (EP1 opcode 0x88) during CommonDeviceInit. This is the authoritative identifier used for calibration lookup, user-visible "Serial #" display, and multi-device disambiguation. The JS frontend converts it via `BigInt(deviceId).toString(16).toUpperCase()`.

Key finding for Rust driver: Do not rely on USB iSerialNumber. Read the 64-bit EEPROM device ID after firmware upload and FPGA init. No need to capture the SPI flash serial before firmware upload.

Document: re/notes/device-serial-number.md
<!-- SECTION:FINAL_SUMMARY:END -->
