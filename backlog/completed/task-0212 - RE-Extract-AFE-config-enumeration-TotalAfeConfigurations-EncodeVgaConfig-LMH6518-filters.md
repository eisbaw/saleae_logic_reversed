---
id: TASK-0212
title: >-
  RE: Extract AFE config enumeration -- TotalAfeConfigurations, EncodeVgaConfig,
  LMH6518 filters
status: Done
assignee: []
created_date: '2026-03-13 20:49'
updated_date: '2026-03-16 06:27'
labels:
  - reverse-engineering
  - analog
  - afe
dependencies: []
references:
  - backlog/tasks/task-0040*
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The analog front-end (AFE) configuration system has several unknowns:
- TotalAfeConfigurations count (how many indexed AFE configs exist)
- EncodeVgaConfig bit layout (how VGA gain/attenuation is encoded)
- VoltageLimitAfterAttenuators value
- LMH6518 filter option values (the programmable filter in the AFE chain)

AfeConfig_RawHardware::ForConfigNumber(int) exists for indexed access but the enumeration is not extracted.

Approach: decompile ForConfigNumber and GetConfigNumber in Ghidra. The LMH6518 datasheet defines the available filter options -- cross-reference with the binary's enum values.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Updated AfeChannelConfig with const ALL_CONFIGS table (66 entries), for_config_number(), config_number() (linear scan), encode_vga_config(). All matching the decompiled binary. 6 new tests, all 953 tests pass.
<!-- SECTION:FINAL_SUMMARY:END -->
