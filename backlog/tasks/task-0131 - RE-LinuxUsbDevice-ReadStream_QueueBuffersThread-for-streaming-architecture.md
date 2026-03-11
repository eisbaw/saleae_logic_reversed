---
id: TASK-0131
title: 'RE LinuxUsbDevice::ReadStream_QueueBuffersThread for streaming architecture'
status: To Do
assignee: []
created_date: '2026-03-11 11:21'
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
