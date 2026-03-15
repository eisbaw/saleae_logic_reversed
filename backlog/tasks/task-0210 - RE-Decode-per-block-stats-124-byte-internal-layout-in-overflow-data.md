---
id: TASK-0210
title: 'RE: Decode per-block stats 124-byte internal layout in overflow data'
status: To Do
assignee: []
created_date: '2026-03-13 20:49'
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
