---
id: TASK-0051
title: Investigate real device serial number from SPI flash boot state
status: To Do
assignee: []
created_date: '2026-03-09 06:20'
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
