---
id: TASK-0163
title: Wire async transfer queue (BulkStreamReader) to live capture path
status: Done
assignee: []
created_date: '2026-03-12 22:27'
updated_date: '2026-03-12 23:35'
labels:
  - driver
  - feature
  - performance
  - capture
dependencies: []
references:
  - crates/saleae-usb/src/stream.rs
  - crates/saleae-driver/src/capture.rs
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-0144 implemented BulkStreamReader/NusbStreamReader/MockStreamReader but they are not integrated into the active capture path. run_capture() still uses single-buffered synchronous reads (one 16KiB transfer at a time). The binary queues 128 outstanding transfers of up to 1.6MB each. At high sample rates, the current approach guarantees FPGA FIFO overflow. Wire StreamingCapture into the capture pipeline.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added `run_capture_streaming()` to `crates/saleae-driver/src/capture.rs` that wires the `BulkStreamReader` trait (from TASK-0144) into the capture pipeline. The function uses `TransferConfig::from_capture()` to compute transfer sizes matching the Saleae binary's formula (bandwidth/240, 128 queue depth, up to 1.6 MB per transfer). The existing synchronous `run_capture()` path is preserved unchanged. Three tests validate the streaming path: basic data collection, empty capture, and error-stops-early behavior. All 858 tests pass, clippy clean.
<!-- SECTION:FINAL_SUMMARY:END -->
