---
id: TASK-0215
title: 'RE: Validate transfer queue depth constant 128 and max_transfer_bytes 5GB'
status: Done
assignee: []
created_date: '2026-03-13 20:49'
updated_date: '2026-03-16 06:06'
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

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Both constants are fully confirmed by Ghidra decompilation in re/notes/transfer-queue-sizing.md:
- queue_depth = 128 (hardcoded in ReadStream_QueueBuffersThread at 0x02d93900)
- max_transfer_bytes = 5,000,000,000 (hardcoded in all three StartSampling functions)
- Rust implementation in crates/saleae-driver/src/transfer_queue.rs matches exactly

No code changes needed -- the constants were already correctly implemented.
<!-- SECTION:FINAL_SUMMARY:END -->
