---
id: TASK-0164
title: Investigate calibration device_type numbering mismatch (potential bug)
status: Done
assignee: []
created_date: '2026-03-12 22:27'
updated_date: '2026-03-13 00:34'
labels:
  - proto
  - bug
  - calibration
dependencies: []
references:
  - 'crates/saleae-proto/src/calibration.rs:490'
  - crates/saleae-proto/src/device.rs
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
calibration.rs test asserts device_type==6 and comments "LogicPro8". But in device.rs, DeviceType::Logic8.type_id()==6 and DeviceType::LogicPro8.type_id()==7. Either the .cal file uses a different numbering scheme from the binary's device type table, or the comment is wrong and the calibration file is for a Logic 8. If the mapping is wrong, the code could load wrong calibration data. Also CalibrationData.device_type should use DeviceType enum, not raw u8.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Investigated and fixed the calibration device_type numbering mismatch.

**Root cause:** `type_id()` used values from an early RE analysis (`fx3-firmware-protocol.md`) that conflated the binary's internal DeviceType enum with the external one. The more thorough analysis in `device-type-enum.md` documents two separate enums -- external (used by GetVidPidForDevice, calibration server, .cal files) and internal (used by DeviceTypeToString). The .cal file for our known Logic Pro 8 (confirmed by EEPROM serial) contains `mDeviceType=6`, which matches the external enum (LogicPro8=6), not the old `type_id()` value of 7.

**Changes:**
1. Fixed `type_id()` to return correct external enum values (LogicOriginal=0, Logic16=1, Logic4=4, Logic8=5, LogicPro8=6, LogicPro16=7)
2. Added `from_type_id()` reverse lookup for converting .cal file values to DeviceType
3. Changed `CalibrationData.device_type` from raw `u8` to `DeviceType` enum
4. Updated calibration parser to validate and convert mDeviceType via `from_type_id()`
5. Corrected the VID:PID table in `fx3-firmware-protocol.md` to show both external and internal enum columns
6. Fixed misleading comment in `calibration-validation.md`

The test comment `// LogicPro8` on the `device_type == 6` assertion was correct all along -- the bug was in `type_id()` returning wrong values.
<!-- SECTION:FINAL_SUMMARY:END -->
