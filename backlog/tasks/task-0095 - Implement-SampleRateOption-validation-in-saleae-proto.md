---
id: TASK-0095
title: Implement SampleRateOption validation in saleae-proto
status: Done
assignee: []
created_date: '2026-03-10 23:13'
updated_date: '2026-03-11 00:21'
labels:
  - rust
  - saleae-proto
  - implementation
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-0090 documented the SampleRateOption struct and per-device rate arrays. Implement sample rate validation.

Requirements:
- SampleRateOption struct: digital_rate, analog_rate, performance_tier
- Three performance tiers: digital-only, analog basic, analog advanced
- Per-device rate arrays:
  - Pro: 15 digital rates (500MHz-1MHz), 11 analog rates (100MHz-1MHz)
  - Logic 8: rates constrained by non-Pro FPGA
  - Logic 4: fixed 12MHz/6MHz with decimation
- validate_sample_rate(device_type, digital_rate, analog_rate) -> Result
- get_available_rates(device_type) -> Vec<SampleRateOption>
- Rate selection: find closest valid rate for a requested rate (SelectClosestIndex from TASK-0075)

Reference: re/notes/sample-rate-options.md, re/notes/computation-utilities.md
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
SampleRateOption validation in saleae-proto/src/sample_rate.rs (470 lines).\n\nPerformanceTier enum: DigitalOnly, AnalogBasic, AnalogAdvanced.\n\nStatic rate tables from RE notes:\n- PRO_DIGITAL_RATES: 15 entries (500M-1M)\n- PRO_ANALOG_RATES: 11 entries (100M-1M), USB2 variant omits 100M\n- LOGIC8_DIGITAL_RATES: 9 entries (200M-1.25M)\n- LOGIC8_ANALOG_RATES: 3 entries (20M, 10M, 5M)\n- LOGIC4_OPTIONS: 10 fixed digital+analog combos including decimated analog\n\nFunctions: available_rates, select_closest_rate, select_closest_index, validate_rate, performance_tier, digital_rates, analog_rates.\n\n40+ tests, clippy clean."
<!-- SECTION:FINAL_SUMMARY:END -->
