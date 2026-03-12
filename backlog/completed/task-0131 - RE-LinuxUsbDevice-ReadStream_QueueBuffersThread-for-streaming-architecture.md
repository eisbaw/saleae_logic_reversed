---
id: TASK-0131
title: 'RE LinuxUsbDevice::ReadStream_QueueBuffersThread for streaming architecture'
status: Done
assignee: []
created_date: '2026-03-11 11:21'
updated_date: '2026-03-12 03:54'
labels:
  - re
  - ghidra
  - usb
  - streaming
dependencies: []
references:
  - re/notes/transfer-queue-sizing.md
  - re/notes/libusb-boundaries.md
  - crates/saleae-driver/src/transfer_queue.rs
  - re/notes/streaming-thread-decompilation.md
  - re/ghidra/decompiled-streaming-thread.txt
  - re/ghidra/decompiled-streaming-thread2.txt
  - re/ghidra/scripts/DecompileStreamingThread.java
  - re/ghidra/scripts/DecompileStreamingThread2.java
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
LinuxUsbDevice::ReadStream_QueueBuffersThread(...) is the thread that manages USB bulk transfer queuing during capture. Understanding this reveals:
- How transfers are queued and reaped (async libusb pattern)
- Buffer lifecycle (allocate -> submit -> complete -> reuse)
- Overflow detection and recovery
- Thread synchronization with the main capture thread

Our nusb-based approach is different from libusb async transfers, but understanding the original architecture helps ensure we handle edge cases (transfer timeout, short transfers, device disconnect during capture).

Investigation:
1. Decompile ReadStream_QueueBuffersThread
2. Document the transfer state machine
3. Document overflow/error handling
4. Document thread synchronization (mutexes, condition variables)
5. Compare against our transfer_queue.rs sizing and capture.rs streaming
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Decompiled and documented the complete streaming architecture from libgraph_server_shared.so.

## Key findings

**Two-thread model**: The original uses two cooperating threads:
1. BulkReadStream_Read (FUN_02e93900, 3865 bytes): outer event loop that processes completed transfers via libusb_handle_events_timeout_completed, delivers data to callbacks
2. ReadStream_QueueBuffersThread (FUN_02e94ae0, 2579 bytes): inner submit loop that allocates buffers from MemoryPool and submits transfers

**on_libusb_transfer_complete callback** (FUN_02e95650, 110 bytes): intentionally minimal -- only sets mComplete=1 and increments counter. All data processing in Thread 1.

**Transfer metadata**: 40 bytes per entry (libusb_transfer*, buffer shared_ptr, mComplete flag, transfer_id, transfer_count).

**Error handling**: ReadStreamError enum codes 1=Timeout, 2=StaleTimeout, 4=DeviceDisconnected, 5=Fatal. Dynamic queue capacity reduction only on LIBUSB_ERROR_BUSY (-11).

**No per-transfer timeout**: libusb_transfer timeout set to 0. Timeout handled at event loop level (1s libusb_handle_events_timeout).

## Gaps in our implementation

Our CaptureStream uses synchronous single-buffered reads (one 16 KiB transfer at a time). The original queues 128 outstanding transfers of up to 1.6 MB each. At high sample rates, our approach guarantees FPGA FIFO overflow.

nusb provides `nusb::transfer::Queue` which is the direct equivalent of Saleae's TransferQueue. Updated capture.rs with detailed gap documentation. transfer_queue.rs already correctly implements the sizing formula.

## Files changed
- `crates/saleae-driver/src/transfer_queue.rs`: Added streaming architecture docs, cross-references to decompilation
- `crates/saleae-driver/src/capture.rs`: Documented 7 known gaps vs original implementation
- `re/notes/streaming-thread-decompilation.md`: Full analysis with reconstructed pseudocode
- `re/ghidra/decompiled-streaming-thread.txt`: Raw decompiled BulkReadStream_Read
- `re/ghidra/decompiled-streaming-thread2.txt`: Raw decompiled QueueBuffersThread + callback
<!-- SECTION:FINAL_SUMMARY:END -->
