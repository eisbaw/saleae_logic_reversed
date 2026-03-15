---
id: TASK-0207
title: 'RE: Determine mix ratio computation formula for registers 0x0D/0x0E'
status: To Do
assignee: []
created_date: '2026-03-13 20:48'
labels:
  - reverse-engineering
  - fpga-registers
  - capture
dependencies: []
references:
  - re/notes/disaggregator-interleaving.md
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Registers MIX_RATIO_A (0x0D) and MIX_RATIO_B (0x0E) control analog/digital sample interleaving in mixed-mode captures. The computation formula from sample rate and channel config is unknown.

These values determine how analog and digital samples are interleaved in the EP2 data stream. Critical for correct mixed-mode capture disaggregation.

Approach: decompile the function that writes these registers in Ghidra. The caller likely takes sample_rate, analog_channel_count, digital_channel_count as inputs.
<!-- SECTION:DESCRIPTION:END -->
