---
id: TASK-0152
title: >-
  Fix hardcoded calibration coefficients in init.rs -- use EEPROM data instead
  of pcap values
status: Done
assignee: []
created_date: '2026-03-12 22:26'
updated_date: '2026-03-12 23:04'
labels:
  - driver
  - bug
  - calibration
dependencies: []
references:
  - 'crates/saleae-driver/src/init.rs:401'
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
init.rs:401-405 hardcodes 16 calibration bytes from ONE pcap of ONE device, ignoring the calibration data just read from EEPROM at line 394. This produces wrong calibration on every other device. Replace with the EEPROM-read values.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Replaced hardcoded 16-byte calibration array in `post_fpga_init()` with the EEPROM data already read at line 394. Added length validation (must be exactly 16 bytes) that returns `DriverError::InvalidArgument` on mismatch. All 342 unit tests and 21 integration tests pass. Commit: dfb8285.
<!-- SECTION:FINAL_SUMMARY:END -->
