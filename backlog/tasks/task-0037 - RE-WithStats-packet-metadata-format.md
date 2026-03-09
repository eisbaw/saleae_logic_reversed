---
id: TASK-0037
title: RE WithStats packet metadata format
status: To Do
assignee: []
created_date: '2026-03-09 00:38'
labels:
  - re
  - data-format
dependencies:
  - TASK-0011
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Reverse engineer the stats/metadata bytes appended to each capture data packet.

Known from string analysis:
- SamplesPerPacketMultiple_WithStats alignment requirement
- cur_overflow and prev_overflow fields exist
- Per-packet timestamp (ts field)

Need to determine:
- How many bytes of stats metadata per packet
- Exact field layout of stats section
- Whether stats are appended or prepended
- What the "WithStats" alignment multiple value is

Method: Compare packet sizes with and without stats. Ghidra analysis of the packet parsing code that reads cur_overflow/prev_overflow. The format string "id={}, start={}, end={}, sample_count={}, config_id={}, ts={}, software={}, cur_overflow={}, prev_overflow={}" reveals the fields but not the binary layout.
<!-- SECTION:DESCRIPTION:END -->
