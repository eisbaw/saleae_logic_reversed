---
id: TASK-0110
title: Implement EEPROM I2C calibration read/write (ReadI2C/WriteI2C)
status: Done
assignee: []
created_date: '2026-03-11 05:59'
updated_date: '2026-03-11 06:19'
labels:
  - rust
  - saleae-driver
  - calibration
  - i2c
dependencies: []
references:
  - re/notes/i2c-eeprom-protocol.md
  - re/notes/eeprom-layout.md
  - re/notes/init-sequence-pcap.md
  - crates/saleae-driver/src/ep1.rs
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Pcap analysis (frames 1661-1752) shows the device reads calibration data from an I2C EEPROM at address 0xC0 during initialization. This is on-device calibration data separate from the server-hosted calibration download.

Protocol from pcap:
- Device ID read: I2C addr 0xC0, register 0x04, returns 5 bytes (0x02, 0x04, 0x11, 0x33, 0x43)
- Calibration block read: I2C addr 0xC0, reg 0x08+0x23, 35-byte blocks with polling
- Calibration block read: I2C addr 0xC0, reg 0x08+0x43, 67-byte blocks with polling
- Calibration write: I2C addr 0xC0, reg 0x1C, 26-byte parameter write

The EP1 opcodes for I2C access exist in our protocol but aren't wired up:
- WriteI2C / ReadI2C commands via EP1
- Polling loop: write register address, then poll for data ready, then read result

Implementation:
1. Add read_i2c() and write_i2c() methods to EP1 command layer
2. Add eeprom_read_block() higher-level method with polling
3. Add eeprom_read_calibration() that reads the full calibration dataset
4. Parse the calibration block format (35-byte and 67-byte variants)
5. Integrate into init sequence after FPGA programming

Reference: re/notes/i2c-eeprom-protocol.md, re/notes/eeprom-layout.md, re/notes/init-sequence-pcap.md (Phase 6)
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Created eeprom.rs (12 tests) with polled I2C read/write via EP1 opcodes 0x87/0x88. Constants match pcap: EEPROM_I2C_ADDR=0xC0, polling with status 0x01->0x02. Functions: eeprom_read_device_id, eeprom_read_calibration_block (3-step write-poll-ack), eeprom_write_calibration_params, eeprom_read_full_calibration. Integrated into init.rs Phase 6. EepromCalibration struct holds device_id + block_35 + block_67. Commit 755e58e.
<!-- SECTION:FINAL_SUMMARY:END -->
