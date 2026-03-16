---
id: TASK-0214
title: 'RE: Verify TriggerMode values 1 (Auto) and 2 (Single) in TRIGGER_CONTROL'
status: Done
assignee: []
created_date: '2026-03-13 20:49'
updated_date: '2026-03-16 06:27'
labels:
  - reverse-engineering
  - fpga-registers
  - triggers
dependencies: []
references:
  - 're/notes/fpga-glitch-filter.md:54-68'
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TriggerMode is a 2-bit field in TRIGGER_CONTROL register. Values 0=Normal and 3=Disabled are confirmed. Values 1 and 2 are inferred as Auto and Single respectively, based on config struct field naming and oscilloscope convention -- not independently verified.

Approach: capture USB traffic with Saleae Logic 2 while changing trigger mode in the UI. Compare the register writes for each mode setting. Alternatively, decompile the trigger mode setter.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
All 4 TriggerMode values now confirmed. ProcessPendingConfiguration (ELF 0x2d4a280) reveals +1 offset mapping from UI: 0=Normal (default/zero-init), 1=Auto (UI val 0+1), 2=Single (UI val 1+1), 3=Disabled (UI val 2+1, independently confirmed by DisableAnalogTrigger writing 0x0C).
<!-- SECTION:FINAL_SUMMARY:END -->
