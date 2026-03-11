---
id: TASK-0081
title: RE packed_data_collection -- slab allocator and deque arena
status: Done
assignee: []
created_date: '2026-03-10 12:32'
updated_date: '2026-03-10 18:51'
labels:
  - re
  - ghidra
  - data-format
dependencies: []
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Source paths leaked:
- `/workspace/monorepo/graph-io/core/packed_data_collection/include/packed_data_collection/fixed_deque.h`
- `/workspace/monorepo/graph-io/core/packed_data_collection/src/deque_arena.cpp`
- `/workspace/monorepo/graph-io/core/packed_data_collection/src/fixed_deque.cpp`
- `/workspace/monorepo/graph-io/core/packed_data_collection/src/slab.cpp`

Custom memory allocator / data structure for storing captured samples. Understanding this reveals:
- How captured data is buffered in memory during streaming
- Memory layout for pre/post trigger buffers
- Slab allocation strategy (important for understanding overflow behavior)
- How the ring buffer interacts with the trigger engine

Investigation steps:
1. Search for strings related to these source files
2. Decompile the slab allocator and fixed_deque
3. Determine allocation sizes and growth strategy
4. Check if this is where trigger pre-buffer lives
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
packed_data_collection is a three-layer custom memory management system that bypasses malloc entirely:\n\n1. Slab (24-byte struct): mmap(MAP_PRIVATE|MAP_ANON) regions with mutex-protected freelist. madvise(MADV_DONTNEED) for physical page release without unmapping.\n\n2. DequeArena: Bump-pointer arena backed by Slabs with doubly-linked 0x28-byte nodes. FreeBefore/FreeAfter provide O(1) trim-from-either-end -- this is the pre-trigger buffer management mechanism.\n\n3. FixedByteDeque: VM-backed circular buffer with large mmap reservation, lazy mprotect(PROT_RW) commit, madvise(MADV_DONTNEED) decommit. 2MB alignment targets Linux THP.\n\nWaveformDataCollection::Writer::PushBack writes to 5 buffer regions: raw waveform (0x90-byte entries), serialized data, sample indices (0x10-byte), data pointers (0x08-byte), and min/max summary at 6 power-of-2 reduction levels.\n\n43 functions decompiled. Analysis: re/notes/packed-data-collection.md"
<!-- SECTION:FINAL_SUMMARY:END -->
