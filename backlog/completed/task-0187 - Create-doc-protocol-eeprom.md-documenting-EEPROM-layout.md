---
id: TASK-0187
title: Create doc/protocol/eeprom.md documenting EEPROM layout
status: Done
assignee: []
created_date: '2026-03-13 05:36'
updated_date: '2026-03-13 07:51'
labels:
  - docs
  - protocol
  - EEPROM
dependencies: []
references:
  - re/captures/saleae-eeprom-512.bin
  - crates/saleae-driver/src/eeprom.rs
  - crates/saleae-driver/src/init.rs
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
EEPROM layout (calibration coefficients at 0x0010, device ID at 0x0008, HW revision at 0x0020) is documented in MEMORY.md and code comments but NOT in doc/protocol/. Need a proper doc covering: CY bootloader header, VID/PID, device serial, calibration coefficients format, REV field with FPGA variant/ADC type classification. Cross-reference to calibration-format.md and fpga-registers.md.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Created `doc/protocol/eeprom.md` documenting the full EEPROM layout and access mechanism for FX3-based Saleae Logic devices. Covers the main EEPROM (CY bootloader header, PID/VID, device serial, 16 calibration coefficients, hardware revision) and the separate calibration EEPROM at I2C address 0xC0 with its polled read protocol. All fields are Confirmed confidence level. Includes cross-references to calibration-format.md, fpga-registers.md, ep1-commands.md, and the Rust implementation. Commit: 550bb4e.
<!-- SECTION:FINAL_SUMMARY:END -->
