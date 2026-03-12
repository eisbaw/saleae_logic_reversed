---
id: TASK-0126
title: RE DeviceSettings performance option functions (sample rate/channel tradeoffs)
status: Done
assignee: []
created_date: '2026-03-11 11:20'
updated_date: '2026-03-12 03:28'
labels:
  - re
  - ghidra
  - device-settings
dependencies: []
references:
  - re/notes/sample-rate-options.md
  - re/notes/device-settings.md
  - crates/saleae-proto/src/sample_rate.rs
  - re/ghidra/decompiled-performance-options.txt
  - re/ghidra/decompiled-get-device-settings.txt
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

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
## RE Findings

Decompiled all 6 DeviceSettings performance option functions from `libgraph_server_shared.so` v2.4.40:
- `GetPerformanceInterfaceType()` at ELF VA 0x1c49e40
- `GetDigitalOnlyPerformanceOptions()` at ELF VA 0x1c49ec0
- `GetAnalogBasicPerformanceOptions()` at ELF VA 0x1c4a140
- `GetAnalogAdvancedPerformanceOptions()` at ELF VA 0x1c4a250
- `SetAnalogBasicPerformanceOption()` at ELF VA 0x1c4ac00
- `SetDigitalOnlyPerformanceOption()` at ELF VA 0x1c4af40
- `SetAnalogAdvancedPerformanceOption()` at ELF VA 0x1c4b130

### SampleRateOption Binary Struct (24 bytes)

| Offset | Size | Field |
|--------|------|-------|
| 0x00 | 4 | digital_rate (u32) |
| 0x04 | 1 | has_digital (bool) |
| 0x08 | 4 | analog_output_rate (u32) |
| 0x0c | 1 | has_analog (bool) |
| 0x0d | 1 | is_set (bool) |
| 0x0e | 1 | has_decimation (bool) |
| 0x10 | 4 | analog_physical_rate (u32) |
| 0x14 | 4 | bandwidth_tier_index (u32) |

### USB Maximum Bit Rate Table (ELF VA 0x4748f0)

- Full Speed: 6 Mbps
- High Speed: 300 Mbps
- SuperSpeed: 3000 Mbps

### Performance Interface Type Table (ELF VA 0x4748cc)

Maps device_class index to tier: 0,1,2=DigitalOnly, 3,4=AnalogBasic, 5,6,7=AnalogAdvanced, 8=AnalogBasic.

### Bandwidth Allocation Algorithm

AnalogAdvanced generates 5 tiers at 100%, 80%, 60%, 40%, 20% of max USB bandwidth. Formula: `bits_per_sample * analog_physical_rate * analog_channels + digital_rate * digital_channels <= budget`.

### Rate Table Validation

All existing static rate tables (PRO_DIGITAL_RATES, PRO_ANALOG_RATES, PRO_ANALOG_RATES_USB2, LOGIC8_DIGITAL_RATES, LOGIC8_ANALOG_RATES, LOGIC4_OPTIONS) confirmed correct against .rodata.

Pro digital rates validated: all divide 500 MHz evenly (base clock DAT_0044d9d0).

## Code Changes

Updated `crates/saleae-proto/src/sample_rate.rs`:
- Added USB bandwidth constants and `max_usb_bitrate()` function
- Added `bandwidth_tier_budget()` function for the 5-tier system
- Added `analog_physical_rate` and `bandwidth_tier` fields to `SampleRateOption`
- Added `has_decimation()` method
- Added `SampleRateOption::advanced()` constructor
- Added `compute_digital_only_options()` - bandwidth-filtered digital rates
- Added `compute_advanced_options()` - full bandwidth allocation with 5 tiers
- Added 15 new unit tests (63 total, all passing)
<!-- SECTION:FINAL_SUMMARY:END -->
