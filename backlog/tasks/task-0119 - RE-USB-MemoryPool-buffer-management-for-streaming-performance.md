---
id: TASK-0119
title: RE USB MemoryPool buffer management for streaming performance
status: To Do
assignee: []
created_date: '2026-03-11 07:54'
labels:
  - re
  - ghidra
  - usb
  - performance
dependencies: []
references:
  - re/notes/transfer-queue-sizing.md
  - crates/saleae-driver/src/transfer_queue.rs
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Source path leaked: /workspace/monorepo/graph-io/devices/usb_device/src/memory_pool.cpp

We have transfer queue sizing (TASK-0094) but not the memory pool itself. This manages USB bulk transfer buffer allocation and reuse during streaming. Understanding it could be important for avoiding allocation stalls during high-speed capture.

Investigation:
1. Find assertion strings referencing memory_pool.cpp
2. Decompile the pool allocator
3. Document: pool size, buffer sizes, pre-allocation strategy, reuse policy
4. Determine if our nusb-based approach needs similar pooling or if nusb handles it

Low priority because nusb may handle buffer management differently than libusb.
<!-- SECTION:DESCRIPTION:END -->
