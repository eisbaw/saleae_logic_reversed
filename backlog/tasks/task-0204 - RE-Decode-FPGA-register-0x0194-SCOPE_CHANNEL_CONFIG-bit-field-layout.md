---
id: TASK-0204
title: 'RE: Decode FPGA register 0x0194 (SCOPE_CHANNEL_CONFIG) bit field layout'
status: To Do
assignee: []
created_date: '2026-03-13 20:48'
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
