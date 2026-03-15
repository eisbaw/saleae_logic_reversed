---
id: TASK-0203
title: >-
  Investigate full-init failure: was opcode 0x88 (ReadI2C), not ep1-test
  corruption
status: Done
assignee: []
created_date: '2026-03-13 14:13'
updated_date: '2026-03-13 14:19'
labels:
  - hardware-test
  - debugging
  - ep1
dependencies: []
references:
  - re/scripts/ep1-test-pyusb.py
  - crates/saleae-cli/src/commands.rs (ep1-test command)
  - re/notes/init-sequence-pcap.md
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
**RESOLVED**: The DCM lock failure observed on 2026-03-13 was NOT caused by ep1-test corrupting device state. The root cause was the newly added `read_eeprom_device_id()` call in `full_init()` which uses opcode 0x88 (ReadI2C) with no payload. This opcode either doesn't exist on this firmware or requires different parameters -- the firmware responds with a bulk transfer error that kills the EP1 session.

## Root cause

Commit `dfb8285` ("Fix hardcoded calibration coefficients in init.rs to use EEPROM data") added a call to `dev.channel.read_eeprom_device_id()` in `full_init()` between FPGA programming and ADC init. This function sends opcode 0x88 with an empty payload -- a HYPOTHESIS that was never hardware-validated. The firmware doesn't handle this, causing a bulk transfer failure on EP1 IN.

The tagged version (v0.1-capture-works) used `read_version()` (opcode 0x07) with I2C params `[0x33, 0x81, 0x08, 0x00, 0x08, 0x00]` for all EEPROM reads, which is pcap-validated and works.

## Fix applied

Replaced `read_eeprom_device_id()` (opcode 0x88) with `read_version()` (opcode 0x07) in commands.rs. After fix: full-init works, DCM locks (r0x40=0xE0), capture runs at 199.2 Mbps.

## Debugging timeline

1. Checked out v0.1-capture-works tag, ran full-init from bootloader -- worked
2. Switched to master, ran full-init -- also worked (device still had good state from step 1)
3. Power cycled device, ran master full-init -- FAILED with "failed to read EEPROM device ID" / "bulk transfer failed on endpoint 0x81"
4. Identified opcode 0x88 as the culprit -- HYPOTHESIS/PLACEHOLDER code that was never validated
5. Replaced with pcap-validated read_version approach -- works
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Run the 6-step test protocol above and document results
- [ ] #2 If confirmed: add a visible warning to ep1-test output about state corruption
- [ ] #3 If confirmed: document in re/notes/ that ep1-test requires power cycle afterward
<!-- AC:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Root cause: unvalidated opcode 0x88 (ReadI2C) in `read_eeprom_device_id()` killed EP1 session. Fix: use pcap-validated `read_version` (opcode 0x07) with I2C params. The ep1-test corruption hypothesis was wrong.
<!-- SECTION:FINAL_SUMMARY:END -->
