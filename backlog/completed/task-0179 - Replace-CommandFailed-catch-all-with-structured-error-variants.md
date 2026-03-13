---
id: TASK-0179
title: Replace CommandFailed catch-all with structured error variants
status: Done
assignee: []
created_date: '2026-03-13 05:35'
updated_date: '2026-03-13 07:09'
labels:
  - driver
  - error-handling
  - quality
dependencies: []
references:
  - crates/saleae-driver/src/error.rs
  - crates/saleae-driver/src/ep1.rs
  - crates/saleae-driver/src/calibration.rs
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
DriverError::CommandFailed { command, detail: String } is used for 18+ distinct failure modes across ep1.rs, eeprom.rs, fpga.rs, capture.rs. The detail field is always a formatted string, making it impossible to distinguish "wrong response length" from "NACK" from "readback mismatch" programmatically. Split into specific variants: NackResponse, ReadbackMismatch, WrongResponseLength, UnexpectedResponse, etc. Also replace CalibrationDownload(String) with structured sub-variants for HTTP error, parse error, empty response, corrupt cache.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Split DriverError::CommandFailed into 5 structured variants plus CalibrationError:

- **NackResponse** { command, actual } -- WriteRegisters/WriteConfigData non-zero ACK
- **WrongResponseLength** { command, expected, actual } -- Ping, ReadRegister16, ReadRegister8, ReadEepromDeviceId, EepromReadDeviceId
- **UnexpectedResponse** { command, expected, actual: Vec<u8> } -- Ping response mismatch, EEPROM unexpected status byte
- **ReadbackMismatch** { register, expected, actual } -- capture register verification
- **PollTimeout** { operation, attempts } -- EEPROM poll exhaustion
- **CalibrationError**(String) -- parse failures separated from HTTP transport errors

Also moved WriteEeprom size check from CommandFailed to InvalidArgument.

CommandFailed retained for: getrandom failure, bulk OUT wrapping, HwRevision parse, data chunk parse.

All 870 tests pass, zero clippy warnings. Commit: b308bc5
<!-- SECTION:FINAL_SUMMARY:END -->
