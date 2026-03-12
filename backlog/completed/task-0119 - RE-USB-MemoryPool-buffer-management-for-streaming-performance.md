---
id: TASK-0119
title: RE USB MemoryPool buffer management for streaming performance
status: Done
assignee: []
created_date: '2026-03-11 07:54'
updated_date: '2026-03-12 04:23'
labels:
  - re
  - ghidra
  - usb
  - performance
dependencies: []
references:
  - re/notes/transfer-queue-sizing.md
  - crates/saleae-driver/src/transfer_queue.rs
documentation:
  - doc/memory-pool.md
  - re/ghidra/decompiled-memory-pool.txt
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

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
## MemoryPool Reverse Engineering Complete

### Decompiled Functions
- **MemoryPool::MemoryPool(Configuration)** at ELF 0x2d9d770 (381 bytes) -- constructor, copies config, asserts mMaxChunks > 0, pre-allocates free list vector
- **MemoryPool::Allocate()** at ELF 0x2d9da80 (254 bytes) -- LIFO free list pop, lazy allocation via operator new, returns shared_ptr with custom deleter
- **MemoryPool::Deallocate()** at ELF 0x2d9dbc0 (515 bytes) -- returns buffer to free list or deletes if cap exceeded, notifies condvar
- **MemoryPool::HasAvailable()** at ELF 0x2d9da00 (62 bytes) -- checks free list or allocated < max
- **MemoryPool::FreeCount()** at ELF 0x2d9da40 (49 bytes) -- returns free list size
- **MemoryPool::~MemoryPool()** at ELF 0x2d9d970 (132 bytes) -- deletes all free list buffers

### Key Corrections
- MemoryPool uses `operator new`, NOT mmap. The `munmap(mAllocation, mCapacity)` assertion belongs to FixedByteDeque, a separate class.
- The `committed_pivot` assertion also belongs to FixedByteDeque::EnsureCommitted, not MemoryPool.
- Pool is a simple free-list with mutex protection, not a virtual memory region.

### nusb Assessment
No separate buffer pool needed. nusb's `Queue` + `RequestBuffer::reuse(completion.data, size)` provides identical buffer recycling. Our QUEUE_DEPTH=128 and transfer_size formula already replicate the original sizing.

### Deliverables
- `doc/memory-pool.md` -- full struct layout, method signatures, streaming integration
- `re/ghidra/decompiled-memory-pool.txt` -- raw decompiled output
- `re/ghidra/scripts/DecompileMemoryPool.java` -- Ghidra headless script
- Updated `crates/saleae-driver/src/transfer_queue.rs` module docs
- Corrected `re/notes/transfer-queue.md` MemoryPool section
<!-- SECTION:FINAL_SUMMARY:END -->
