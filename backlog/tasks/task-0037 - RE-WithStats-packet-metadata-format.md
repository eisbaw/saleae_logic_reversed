---
id: TASK-0037
title: RE WithStats packet metadata format
status: Done
assignee: []
created_date: '2026-03-09 00:38'
updated_date: '2026-03-09 06:54'
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

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Reverse engineered the WithStats packet metadata format by disassembling the stats parser at VA 0x2d41510 (ParseMsoWaveformData in mso_data.cpp).\n\nKey findings:\n- Stats are APPENDED to sample data at the end of each chunk (not prepended)\n- Mandatory stats header is 20 bytes: u32 packet_id (4) + u48 timestamp (6) + u16 flags (2) + u64 host_time (8)\n- Flags u16 has 8 individual boolean flags extracted from bits 15 down to 8\n- Bit 9 controls overflow metadata presence (adds 16 bytes)\n- Bit 12 controls overflow data blocks (636 bytes each: 512 sample + 124 stats)\n- Same FPGA framework (MsoFpga) generates stats for all FX3 devices (Logic Pro, Logic 8, Logic 4, MSO)\n- For Logic devices, DisaggregatorNode strips stats and stores in mAnalogStats\n\nDocumented in re/notes/withstats-format.md"
<!-- SECTION:FINAL_SUMMARY:END -->
