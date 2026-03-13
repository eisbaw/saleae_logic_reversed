---
id: TASK-0153
title: Fix eeprom_read_register_poll silent data corruption on unknown status byte
status: Done
assignee: []
created_date: '2026-03-12 22:27'
updated_date: '2026-03-12 22:54'
labels:
  - driver
  - bug
  - eeprom
dependencies: []
references:
  - 'crates/saleae-driver/src/eeprom.rs:240'
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
eeprom.rs:240-250: An unexpected status byte is treated as success, returning the full response including the unknown status byte. When callers expect only data bytes (status stripped), this silently corrupts output (e.g., block_35 gets 35 bytes instead of 34, with byte 0 being the mystery status). Should return DriverError on unknown status, not silently succeed.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Fixed silent data corruption in `eeprom_read_register_poll` where an unknown status byte was returned as `Ok(response)` including the status byte itself. Callers expecting status-stripped data would receive an extra leading byte (e.g., block_35 getting 35 bytes instead of 34). The catch-all match arm now returns `DriverError::CommandFailed` with diagnostic detail including the unknown status value, attempt number, and full response bytes. Updated the corresponding test to assert the error path.
<!-- SECTION:FINAL_SUMMARY:END -->
