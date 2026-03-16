---
id: TASK-0204
title: 'RE: Decode FPGA register 0x0194 (SCOPE_CHANNEL_CONFIG) bit field layout'
status: Done
assignee: []
created_date: '2026-03-13 20:48'
updated_date: '2026-03-16 06:27'
labels:
  - reverse-engineering
  - fpga-registers
dependencies: []
references:
  - 'doc/protocol/fpga-registers.md:190'
  - re/notes/fpga-glitch-filter.md
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
FPGA register 0x0194 is written by function at ELF VA 0x02d662c0 with a single uint32 parameter. The exact bit field layout is unknown. Possibly scope-mode channel select or test pattern config.

Approach: decompile the function at 0x02d662c0 in Ghidra, trace callers to understand what values are passed. Cross-reference with fpga-glitch-filter.md notes.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Decompiled SetScopeChannelConfig and WriteRegister_0x194. Found 4 related register groups in the scope instrument area: 0x0194 (master config, pass-through uint32), 0x0198-0x01A8 (per-channel with InstrumentIndex[7:5] and InstrumentType[4:0]), 0x0180-0x0190 (config IDs), 0x01B0-0x01BC (voltage thresholds). Updated docs, fpga_regs.rs, and RE notes.
<!-- SECTION:FINAL_SUMMARY:END -->
