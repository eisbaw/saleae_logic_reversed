---
id: TASK-0160
title: Add EP1 command opcode constants module to saleae-proto
status: Done
assignee: []
created_date: '2026-03-12 22:27'
updated_date: '2026-03-12 22:36'
labels:
  - proto
  - protocol
  - quality
dependencies: []
references:
  - crates/saleae-driver/src/ep1.rs
  - crates/saleae-proto/src/
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
EP1 opcodes are the central protocol vocabulary but live only as magic hex in saleae-driver/ep1.rs. The proto crate (wire format types) should define named constants and document command/response structures: 0x01 StartRead, 0x02 StopStream, 0x07 ReadVersion, 0x55 WriteEeprom, 0x7D Ping, 0x7E ResetFpga, 0x7F WriteConfigData, 0x80 WriteRegisters, 0x81 ReadRegisters, 0x86 StopDataPath, 0x87 WriteI2C, 0x88 ReadI2C, 0x89 Device2_1, 0x8B GetDeviceInfo.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Created `crates/saleae-proto/src/ep1_opcodes.rs` with named constants for all 13 known EP1 opcodes, each with doc comments describing the command's wire format and payload structure. Wired the module into `saleae-proto/src/lib.rs`. Replaced all 16 magic hex `send_command(0xNN, ...)` calls in `crates/saleae-driver/src/ep1.rs` with the named constants. All 837 tests pass, clippy clean.
<!-- SECTION:FINAL_SUMMARY:END -->
