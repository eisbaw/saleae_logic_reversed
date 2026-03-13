---
id: TASK-0198
title: >-
  Decode EP2 overflow metadata (16 bytes) and per-block overflow stats (124
  bytes)
status: Done
assignee: []
created_date: '2026-03-13 10:09'
updated_date: '2026-03-13 10:29'
labels:
  - RE
  - EP2
  - reliability
dependencies: []
references:
  - crates/saleae-proto/src/capture_data.rs
  - doc/protocol/digital-capture.md
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
When stats flags bit 9 (overflow_meta) is set, 16 bytes of metadata follow the mandatory 20-byte stats header — currently skipped without parsing. When bit 12 (overflow_data) is set, overflow blocks of 636 bytes each follow (512 sample + 124 stats). Neither structure is decoded.

Need to decompile the overflow handling path at VA 0x2d416c0 (overflow_meta skip) and VA 0x2d41887 (overflow_data processing) to understand:
- What the 16 overflow metadata bytes contain
- What the 124-byte per-block stats contain
- How overflow recovery works (continue with gap vs abort)

This blocks robust long-duration capture support.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Overflow metadata 16-byte structure documented with field names
- [x] #2 Per-block 124-byte stats structure documented
- [x] #3 Overflow fields parsed in capture_data.rs
- [x] #4 Recovery strategy documented (continue/abort/mark gap)
<!-- AC:END -->
