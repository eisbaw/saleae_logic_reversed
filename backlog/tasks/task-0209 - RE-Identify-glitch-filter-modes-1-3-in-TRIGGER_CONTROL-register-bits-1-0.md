---
id: TASK-0209
title: 'RE: Identify glitch filter modes 1-3 in TRIGGER_CONTROL register bits [1:0]'
status: To Do
assignee: []
created_date: '2026-03-13 20:48'
labels:
  - reverse-engineering
  - fpga-registers
  - triggers
dependencies: []
references:
  - 're/notes/fpga-glitch-filter.md:107-119'
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
GlitchFilterEnabled is a 2-bit field at TRIGGER_CONTROL register (0x0070) bits [1:0]. Value 0 = disabled is confirmed. Values 1, 2, 3 are known to enable the filter but the specific mode/behavior for each value is unknown.

Approach: decompile the glitch filter configuration path in Ghidra. Look for how the UI's glitch filter settings map to these 2-bit values. May correspond to different filter widths or filter types.
<!-- SECTION:DESCRIPTION:END -->
