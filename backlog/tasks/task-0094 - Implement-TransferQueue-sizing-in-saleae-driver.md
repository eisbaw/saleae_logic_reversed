---
id: TASK-0094
title: Implement TransferQueue sizing in saleae-driver
status: Done
assignee: []
created_date: '2026-03-10 23:13'
updated_date: '2026-03-11 00:12'
labels:
  - rust
  - saleae-driver
  - implementation
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-0070 documented the transfer queue sizing formula. Implement in saleae-driver.

Requirements:
- ComputeTransferSize: round_up(bandwidth_bytes_per_sec / 240, block_size)
- block_size: 4096 (USB 2.0 High) vs 16384 (USB 3.0 Super)
- Queue depth: 128 transfers
- Cap at 1.6MB per transfer
- SALEAE_USB_TRANSFER_LIMIT env var override support
- Bandwidth calculation from sample_rate * channels * bytes_per_sample
- Unit tests for USB 2.0 and 3.0 scenarios

Reference: re/notes/transfer-queue-sizing.md
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
TransferQueue sizing implemented in saleae-driver/src/transfer_queue.rs (538 lines).\n\nFormula: ceil(bandwidth/240) rounded up to block boundary, capped at 1,638,400 bytes.\n- USB 2.0: block_size=4096, USB 3.0: block_size=16384\n- Queue depth=128, env var SALEAE_USB_TRANSFER_LIMIT override\n- compute_bandwidth: digital=1 bit/sample/ch, analog=bytes_per_sample/sample/ch\n- TransferConfig::from_capture convenience constructor\n\nDiscovery: RE notes arithmetic error corrected -- 1,638,400 = 100*16384 (already 16K-aligned, mask is no-op).\n\n26 unit tests covering USB 2.0/3.0 scenarios, clamping, alignment, env var.\n\nCommit included."
<!-- SECTION:FINAL_SUMMARY:END -->
