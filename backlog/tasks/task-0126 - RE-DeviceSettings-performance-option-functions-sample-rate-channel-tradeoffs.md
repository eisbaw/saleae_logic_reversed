---
id: TASK-0126
title: RE DeviceSettings performance option functions (sample rate/channel tradeoffs)
status: To Do
assignee: []
created_date: '2026-03-11 11:20'
labels:
  - re
  - ghidra
  - device-settings
dependencies: []
references:
  - re/notes/sample-rate-options.md
  - re/notes/device-settings.md
  - crates/saleae-proto/src/sample_rate.rs
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Three DeviceSettings functions control the sample rate / channel count tradeoff tables:
- GetAnalogAdvancedPerformanceOptions(...)
- GetAnalogBasicPerformanceOptions(...)
- GetDigitalOnlyPerforamnceOptions(...) (note: typo in original)

Plus the Set* variants:
- SetAnalogAdvancedPerformanceOption(...)
- SetAnalogBasicPerformanceOption(...)
- SetDigitalOnlyPerformanceOption(...)

These define the valid combinations of sample rate and active channels for each device type. Our sample_rate.rs has static tables from RE notes, but these functions may reveal the exact algorithm (not just the lookup table) and handle edge cases we're missing.

Investigation:
1. Decompile all 6 functions
2. Document the PerformanceOption struct fields
3. Compare the generated options against our static rate tables
4. Check if there are device-specific overrides or USB-speed-dependent options
5. Update sample_rate.rs if discrepancies found
<!-- SECTION:DESCRIPTION:END -->
