---
id: TASK-0215
title: 'RE: Validate transfer queue depth constant 128 and max_transfer_bytes 5GB'
status: To Do
assignee: []
created_date: '2026-03-13 20:49'
labels:
  - reverse-engineering
  - capture
  - performance
dependencies: []
references:
  - 're/notes/transfer-queue-sizing.md:31-34'
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Transfer queue depth formula: queue_depth = min(128, max_transfer_bytes / transfer_size). The constant 128 was extracted from binary decompilation but not hardware-validated. max_transfer_bytes = 5,000,000,000 is assumed but not confirmed.

These constants affect sustained capture throughput. If wrong, captures at high sample rates may overflow or underutilize USB bandwidth.

Approach: test with different queue depths on hardware and observe throughput/overflow behavior. Alternatively, confirm the constants by finding them in the decompiled transfer queue setup code.
<!-- SECTION:DESCRIPTION:END -->
