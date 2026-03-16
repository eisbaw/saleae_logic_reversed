---
id: TASK-0210
title: 'RE: Decode per-block stats 124-byte internal layout in overflow data'
status: Done
assignee: []
created_date: '2026-03-13 20:49'
updated_date: '2026-03-16 06:27'
labels:
  - reverse-engineering
  - protocol
dependencies: []
references:
  - 'doc/protocol/overflow-handling.md:74-85'
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Each overflow data block has a 124-byte (0x7C) per-block stats section appended. Total size confirmed from decompilation (n_blocks * 0x7c), but the internal bit field layout is unknown. The parser currently skips without field-level parsing.

This data likely contains per-block timing, sample counts, or error flags. Understanding it would improve overflow diagnostics.

Approach: decompile the function that populates the stats struct. The size 0x7C is confirmed -- look for struct definitions of that size in Ghidra.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
124 bytes = multi-resolution min/max pyramid for fast waveform rendering. 31 packed u32 entries (each min/max int16 pair) across 5 decimation levels: 16+8+4+2+1=31 entries. Writer::PushBack generates the pyramid with 5 loops. PackedWaveformGetStats uses recursive level indexing for O(log N) queries. Updated capture_data.rs with MinMaxPair type and pyramid structure.
<!-- SECTION:FINAL_SUMMARY:END -->
