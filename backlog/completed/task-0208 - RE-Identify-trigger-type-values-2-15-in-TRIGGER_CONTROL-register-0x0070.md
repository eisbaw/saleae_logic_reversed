---
id: TASK-0208
title: 'RE: Identify trigger type values 2-15 in TRIGGER_CONTROL register 0x0070'
status: Done
assignee: []
created_date: '2026-03-13 20:48'
updated_date: '2026-03-16 06:27'
labels:
  - reverse-engineering
  - fpga-registers
  - triggers
dependencies: []
references:
  - 're/notes/fpga-glitch-filter.md:86-104'
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TriggerType is a 4-bit field in TRIGGER_CONTROL register (0x0070) bits [7:4]. Only two values are confirmed:
- 0 = EdgeTrigger
- 1 = PulseTrigger
- 2-15 = unknown (register supports up to 16 types)

May support additional hardware trigger types not exposed in standard Logic 2 software (e.g., pattern trigger, serial trigger).

Approach: search for TriggerType enum in Ghidra decompilation, or look for switch/case on the 4-bit field. Cross-reference with Saleae Logic 2 UI trigger options.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Confirmed: only 2 trigger types exist in v2.4.40. CheckTrigger has a definitive 2-way switch: 0=Edge->EdgeTrigger_Detect, 1=Pulse->PulseTrigger_Detect. Any other value throws a C++ exception via __cxa_throw. Values 2-15 are reserved for future FPGA expansion.
<!-- SECTION:FINAL_SUMMARY:END -->
