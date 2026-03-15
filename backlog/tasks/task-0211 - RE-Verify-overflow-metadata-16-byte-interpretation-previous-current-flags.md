---
id: TASK-0211
title: 'RE: Verify overflow metadata 16-byte interpretation (previous/current flags)'
status: To Do
assignee: []
created_date: '2026-03-13 20:49'
labels:
  - reverse-engineering
  - protocol
dependencies: []
references:
  - 'doc/protocol/overflow-handling.md:22-42'
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Overflow metadata header is 16 bytes: offsets 0x00-0x07 and 0x08-0x0F, each 8 bytes. Currently hypothesized as "previous overflow flags" (u64 LE) and "current overflow flags" (u64 LE) based on format string parsing from the binary.

Structure size is confirmed by decompilation, but the semantic meaning of each u64 is hypothesis-level confidence.

Approach: capture a real overflow event (high sample rate, long duration) and compare the metadata bytes against the hypothesized interpretation. Alternatively, decompile the overflow handler that reads these fields.
<!-- SECTION:DESCRIPTION:END -->
