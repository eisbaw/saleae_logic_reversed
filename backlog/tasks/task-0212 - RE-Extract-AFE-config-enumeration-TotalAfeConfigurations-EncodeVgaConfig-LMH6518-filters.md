---
id: TASK-0212
title: >-
  RE: Extract AFE config enumeration -- TotalAfeConfigurations, EncodeVgaConfig,
  LMH6518 filters
status: To Do
assignee: []
created_date: '2026-03-13 20:49'
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
