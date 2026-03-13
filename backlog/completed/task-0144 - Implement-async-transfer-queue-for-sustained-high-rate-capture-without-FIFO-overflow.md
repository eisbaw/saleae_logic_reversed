---
id: TASK-0144
title: >-
  Implement async transfer queue for sustained high-rate capture without FIFO
  overflow
status: Done
assignee: []
created_date: '2026-03-12 14:12'
updated_date: '2026-03-12 15:25'
labels:
  - phase-2
  - performance
dependencies: []
references:
  - re/notes/streaming-thread-decompilation.md
  - re/notes/transfer-queue.md
  - crates/saleae-driver/src/transfer_queue.rs
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Current capture uses synchronous single-buffered EP2 reads (16 KiB). At high sample rates the device FIFO will overflow because no USB transfer is pending while we process data. Need to use nusb::transfer::Queue with multiple outstanding transfers (128 depth, computed transfer sizes up to 1.6 MB).

The RE analysis in re/notes/streaming-thread-decompilation.md documents the exact two-thread model Saleae uses. Our approach should be simpler (nusb Queue handles the low-level parts) but needs:
1. Computed transfer size via TransferConfig::from_capture()
2. Pre-submitted transfer queue (128 depth)
3. Buffer reuse via RequestBuffer::reuse()
4. Stall recovery (clear_halt + retry)
5. Timeout detection (distinguish "never got data" vs "data stopped")

The transfer_queue.rs module already has TransferConfig but it's not connected to the capture path.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Implemented in commit 812bcae. Added BulkStreamReader trait in saleae-usb with start/next_completion/recycle/stop/pending methods. NusbStreamReader wraps nusb Queue with timeout racing and stall recovery. MockStreamReader for testing. StreamingCapture in saleae-driver wraps TransferConfig + BulkStreamReader. Not yet wired to live capture path (device not attached).
<!-- SECTION:FINAL_SUMMARY:END -->
