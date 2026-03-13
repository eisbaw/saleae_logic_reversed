---
id: TASK-0146
title: Derive .cal file path from device EEPROM serial instead of hardcoded path
status: Done
assignee: []
created_date: '2026-03-12 16:40'
updated_date: '2026-03-12 16:51'
labels:
  - driver
  - cli
  - calibration
dependencies: []
references:
  - crates/saleae-cli/src/commands.rs
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The CLI currently hardcodes `re/calibration-764051b51bd4d9f1.cal` as a relative path containing a device-specific serial number. This breaks for any other device and depends on the working directory. Should be a CLI argument or derived from the device's EEPROM serial (which is already read during init at offset 0x0008).
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added --cal-file CLI argument and resolve_cal_file() helper that derives calibration path from EEPROM device ID. Searches ., re/, calibration/ for calibration-{id:x}.cal and {id:x}.cal patterns. full_init now reads device ID from EEPROM and prints calibration download URL.
<!-- SECTION:FINAL_SUMMARY:END -->
