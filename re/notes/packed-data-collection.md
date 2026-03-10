# packed_data_collection: Slab Allocator and Deque Arena

Reverse engineered from `libgraph_server_shared.so` (Saleae Logic 2 v2.4.40).

Source paths leaked via assertions:
- `/workspace/monorepo/graph-io/core/packed_data_collection/include/packed_data_collection/fixed_deque.h`
- `/workspace/monorepo/graph-io/core/packed_data_collection/src/deque_arena.cpp`
- `/workspace/monorepo/graph-io/core/packed_data_collection/src/fixed_deque.cpp`
- `/workspace/monorepo/graph-io/core/packed_data_collection/src/slab.cpp`

## Overview

packed_data_collection is a custom memory management subsystem used to buffer
captured sample data during streaming. It consists of three layers:

1. **Slab** -- mmap-based memory regions of fixed size
2. **DequeArena** -- arena allocator backed by slabs, with linked-list tracking
3. **FixedByteDeque** -- circular buffer backed by a large virtual mmap reservation
   with lazy mprotect-based commit

These are the fundamental data structures that hold waveform data during capture
and for post-capture display/measurement.

## Class: Slab

**Namespace**: `Saleae::Graph::Slab`

A Slab is a contiguous memory region allocated via `mmap(NULL, size, PROT_READ|PROT_WRITE, MAP_PRIVATE|MAP_ANONYMOUS, 0, 0)`.

### Slab struct layout (24 bytes)

| Offset | Size | Field         | Description                          |
|--------|------|---------------|--------------------------------------|
| 0x00   | 8    | mAllocation   | Pointer to mmap'd region             |
| 0x08   | 8    | mSize         | Size of the mmap'd region            |
| 0x10   | 1    | mIsSuspended  | Whether MADV_DONTNEED has been issued|

### Key functions

| ELF VA     | Function                                  | Description                                |
|------------|-------------------------------------------|--------------------------------------------|
| 0x2d06dc0  | Slab::Slab(size_t)                        | Allocates via mmap(NULL, size, PROT_RW, MAP_PRIVATE\|MAP_ANON) |
| 0x2d06f00  | Slab::~Slab()                             | Calls munmap(mAllocation, mSize)           |
| 0x2d07020  | Slab::Suspend()                           | Calls madvise(ptr, size, MADV_DONTNEED)    |
| 0x2d07180  | Slab::Resume()                            | Clears mIsSuspended flag                   |

Slab::Suspend() uses `madvise(MADV_DONTNEED)` (value 4) to release physical pages
while keeping the virtual mapping. This allows the OS to reclaim memory without
needing to munmap/mmap. Slab::Resume() just clears the suspended flag -- the
kernel will fault in fresh zero pages on next access.

Assertions:
- `munmap(mAllocation, mSize) == 0` -- in destructor, fatal on failure
- `slab allocation already suspended` -- thrown if Suspend() called twice
- `slab allocation is not suspended` -- thrown if Resume() without Suspend()

## Class: Slab::Manager

**Namespace**: `Saleae::Graph::Slab::Manager`

A freelist-based pool of fixed-size Slabs. All slabs in a Manager have the same size.

### Manager struct layout (partial, ~80+ bytes)

| Offset | Size | Field           | Description                                    |
|--------|------|-----------------|------------------------------------------------|
| 0x00   | 8    | mSlabSize       | Fixed size for all slabs in this pool          |
| 0x08   | 8    | mTotalAllocated | Running total of allocated bytes               |
| 0x10   | 40   | mMutex          | std::mutex protecting the free list            |
| 0x38   | 24   | mFreeList       | std::vector<Slab> of available slabs           |
| 0x38   | 8    |   .begin        | Start pointer                                  |
| 0x40   | 8    |   .end          | Used end pointer                               |
| 0x48   | 8    |   .capacity     | Allocated end pointer                          |

### Key functions

| ELF VA     | Function                          | Description                                |
|------------|-----------------------------------|--------------------------------------------|
| 0x2d07250  | Slab::Manager::Allocate()         | Pop from free list, or mmap a new slab     |
| 0x2d07320  | Slab::Manager::Return(Slab)       | Push slab back to free list (mutex-locked) |

### Allocation flow

1. `Manager::Allocate()` adds mSlabSize to mTotalAllocated
2. Checks if free list (vector at offset 0x38) has entries
3. If free list not empty: pops last entry (vector back + pop_back)
4. If free list empty: calls `Slab::Slab(mSlabSize)` which mmaps a new region
5. If returned slab was suspended: calls Resume() (clears MADV_DONTNEED flag)

Return flow:
1. `Manager::Return(Slab)` asserts `slab.Size() == mSlabSize`
2. Subtracts mSlabSize from mTotalAllocated
3. Locks mutex
4. Pushes slab onto free list vector (with possible realloc via vector push_back)
5. Unlocks mutex

The free list vector stores Slab structs (24 bytes each: ptr, size, suspended_flag).
When a slab is returned, its suspended flag may be set (via madvise DONTNEED)
to release physical pages while keeping the slab in the pool for reuse.

## Class: DequeArena

**Namespace**: `Saleae::Graph::DequeArena`

An arena allocator that provides bump-pointer allocation from Slabs. Tracks
allocated regions as a doubly-linked list of slab-backed segments.

### DequeArena struct layout (~56 bytes)

| Offset | Size | Field            | Description                                    |
|--------|------|------------------|------------------------------------------------|
| 0x00   | 8    | mSlabManager     | Pointer to shared Slab::Manager                |
| 0x10   | 8    | mList.sentinel   | Sentinel node for intrusive doubly-linked list  |
| 0x18   | 8    | mList.sentinel+8 | (prev/next of sentinel)                        |
| 0x20   | 8    | mCount           | Number of slab segments in list                |
| 0x28   | 8    | mFreeStartPointer| Current bump pointer (start of free region)    |
| 0x30   | 8    | mFreeEndPointer  | End of current slab's free region              |

Each linked-list node is a 40-byte (0x28) heap allocation:

| Offset | Size | Field      | Description                           |
|--------|------|------------|---------------------------------------|
| 0x00   | 8    | prev       | Previous node                         |
| 0x08   | 8    | next       | Next node                             |
| 0x10   | 8    | slab.ptr   | Slab allocation pointer               |
| 0x18   | 8    | slab.size  | Slab size                             |
| 0x20   | 1    | slab.susp  | Suspended flag                        |

### Key functions

| ELF VA     | Function                                         | Description                       |
|------------|--------------------------------------------------|-----------------------------------|
| 0x2d07b70  | DequeArena::AllocateContiguousExtend(size_t)     | Alloc new slab, bump-allocate     |
| 0x2d07850  | DequeArena::FreeBefore(void*)                    | Free slabs before given address   |
| 0x2d07a00  | DequeArena::FreeAfter(void*)                     | Free slabs after given address    |
| 0x2d07780  | DequeArena::Clear()                              | Return all slabs to manager       |
| 0x2d076f0  | DequeArena::ExtendFromManager()                  | Get new slab, add to list         |
| 0x2d07b60  | DequeArena::Size()                               | count * slab_manager->slab_size   |

### Allocation strategy

`AllocateContiguousExtend(size)`:
1. Validates size does not exceed single slab capacity (throws invalid_argument)
2. Allocates a new Slab from the Manager
3. Sets mFreeStartPointer and mFreeEndPointer to the new slab range
4. Inserts new slab node into the linked list (at head)
5. Increments count
6. Bump-allocates `size` bytes from the new slab
7. Asserts `mFreeEndPointer - mFreeStartPointer >= size`

For the fast path (inline, not visible in decompiled output), if the current
slab has enough space (`mFreeEndPointer - mFreeStartPointer >= size`), it just
bumps mFreeStartPointer by size. AllocateContiguousExtend is the slow path.

### Deallocation strategy

`FreeBefore(addr)`:
1. Walks the slab linked list to find which slab contains the address
2. Asserts `containing_slab != mSlabs.end()`
3. For all slabs before the containing one: returns them to the Manager
4. Updates linked list, decrements count

`FreeAfter(addr)`: same but frees slabs after the containing address.

This is a classic ring-buffer-friendly pattern: data is appended to the front,
and old data is freed from the back (or vice versa), allowing the pre-trigger
buffer to trim old samples while new ones arrive.

## Class: FixedByteDeque

**Namespace**: `Saleae::Graph::FixedByteDeque`

A virtual-memory-backed circular buffer that reserves a large virtual address
range via mmap but only commits pages on demand via mprotect.

### FixedByteDeque struct layout (~56 bytes)

| Offset | Size | Field               | Description                                 |
|--------|------|---------------------|---------------------------------------------|
| 0x00   | 8    | mAllocation         | Base of mmap'd virtual region               |
| 0x08   | 8    | mAllocationEnd      | End of virtual region                       |
| 0x10   | 8    | mCapacity           | Total virtual size (mAllocationEnd - mAllocation) |
| 0x18   | 8    | mHead               | Logical head index (deque start)            |
| 0x20   | 8    | mTail               | Logical tail index (deque end)              |
| 0x28   | 8    | mCommittedStart     | Start of committed (mprotect'd) region      |
| 0x30   | 8    | mCommittedEnd       | End of committed region                     |

All indices are logical (monotonically increasing). Physical addresses are
computed as `mAllocation + (index % mCapacity)`.

### Page commit granularity

The commit granularity is 0x200000 (2 MB), matching Linux huge page (THP)
alignment. This is visible in EnsureCommitted:

```
uVar3 = (param_2 & 0xffffffffffe00000) + 0x200000;  // round up to 2MB
```

### Key functions

| ELF VA     | Function                                | Description                           |
|------------|-----------------------------------------|---------------------------------------|
| 0x2d07eb0  | FixedByteDeque::~FixedByteDeque()       | munmap(mAllocation, mCapacity)        |
| 0x2d07fd0  | FixedByteDeque::EnsureCommitted(size_t) | mprotect pages into committed range   |
| 0x2d08250  | FixedByteDeque::PopFront(size_t)        | Advance head, madvise old pages       |
| 0x2d08520  | FixedByteDeque::PopBack(size_t)         | Retreat tail, madvise old pages       |
| 0x2d08130  | mprotect_commit(void*, size_t)          | mprotect(ptr, size, PROT_READ\|PROT_WRITE) |
| 0x2d08360  | madvise_decommit(void*, size_t)         | madvise(ptr, size, MADV_DONTNEED)     |

### Virtual memory strategy

1. **Reserve**: mmap a large region (PROT_READ|PROT_WRITE, MAP_PRIVATE|MAP_ANONYMOUS)
2. **Commit on demand**: EnsureCommitted() calls mprotect(PROT_READ|PROT_WRITE) on
   2MB-aligned chunks as the deque grows
3. **Decommit on pop**: PopFront/PopBack call madvise(MADV_DONTNEED) on 2MB-aligned
   chunks that are no longer needed
4. **Circular indexing**: head/tail indices grow monotonically, physical offset is
   `index % capacity`

The 2MB alignment is very deliberate -- it matches Linux Transparent Huge Pages
(THP) granularity. Using THP for large sample buffers significantly reduces TLB
pressure during high-throughput streaming.

Error condition: "deque ran out of virtual address space" -- thrown when the
deque wraps around and the tail catches up to the head (capacity exceeded).

## Class: FixedDeque<T>

**Namespace**: `Saleae::Graph::FixedDeque<T>`

A typed wrapper around FixedByteDeque that provides element-level access.
The primary instantiation seen is `FixedDeque<PackedWaveformData*>`, which
stores pointers to waveform data blocks.

The Iterator type maintains an index and pointer into the backing store,
with wrap-around logic:
```cpp
// operator--() for Iterator:
assert(mIndex > 0);  // "mIndex > 0" assertion at line 0x18d of fixed_deque.h
mIndex--;
if (mPointer == backing_store->begin()) {
    mPointer = backing_store->wrap_end();  // wrap around
}
mPointer--;
```

## Integration with Capture Pipeline

### WaveformDataCollection

The packed_data_collection classes are used by `WaveformDataCollection` (source:
`waveform_data_collection.cpp`) to store captured waveform data. The key
consumer is `WaveformDataCollection::Writer::PushBack()`:

1. Serializes raw waveform data into the DequeArena (0x90 byte entries)
2. Writes indices into a FixedByteDeque (0x10 byte entries for pointers, 0x8 for data ptrs)
3. Computes min/max summary statistics at multiple power-of-2 reduction levels
   (16, 32, 64, 128, 256, 512 samples) for efficient waveform rendering
4. Statistics are stored in a separate DequeArena (0x7C bytes per stats block)

The PushBack function operates on these buffer regions at fixed offsets from
the WaveformDataCollection object:

| Offset | Arena/Deque         | Element Size | Purpose                           |
|--------|---------------------|-------------|-----------------------------------|
| 0x100  | FixedByteDeque      | 0x10        | Sample index entries              |
| 0x200  | FixedByteDeque      | 0x08        | Data pointer entries              |
| 0x2B0  | DequeArena          | 0x90        | Raw waveform data blocks          |
| 0x480  | DequeArena          | variable    | Serialized waveform data          |
| 0x500  | DequeArena          | 0x7C        | Min/max summary statistics        |

### Memory trimming

`FUN_01c5cfd0` (at ELF VA 0x1b5cfd0) is the collection trimmer. It:
1. Acquires two mutexes (at offsets 0x300 and 0x280 of the collection)
2. Calculates how many entries to trim from front and back
3. Calls FixedByteDeque::PopFront/PopBack to advance indices
4. Calls DequeArena::FreeBefore/FreeAfter to release slabs
5. Notifies listeners via callback (for UI updates)
6. Checks for overflow condition: "attempted to erase more data from collection than exists"

This trimming function is the mechanism by which the pre-trigger buffer
discards old samples. As new data arrives, the oldest data beyond the
pre-trigger duration is freed.

### Suspend/resume for memory pressure

When memory is tight, the Slab::Manager can be told to suspend (madvise
MADV_DONTNEED) free slabs, allowing the OS to reclaim physical pages
without tearing down the virtual mappings. On resume, the first access
to the pages will fault in fresh zeros.

## Summary of memory allocation strategy

```
                    shared_ptr<Slab::Manager>
                    (one per device/capture)
                           |
          +----------------+----------------+
          |                |                |
     DequeArena       DequeArena       DequeArena
     (waveform data)  (statistics)    (raw data)
          |                |                |
     Slab pool        Slab pool        Slab pool
     [mmap regions]   [mmap regions]   [mmap regions]

     FixedByteDeque    FixedByteDeque
     (index entries)   (pointer entries)
     [single large mmap, mprotect commit, madvise decommit]
```

Key design choices:
- **mmap + madvise** instead of malloc: gives direct control over physical page
  lifecycle, critical for a streaming system that may buffer gigabytes
- **Fixed slab sizes**: eliminates fragmentation within a capture session
- **2MB page alignment**: optimizes for THP, reducing TLB pressure
- **Separate arenas**: waveform data, statistics, and indices are in different
  arenas, allowing independent trim/growth
- **Linked-list slab tracking**: enables O(1) trim-from-front and trim-from-back,
  essential for the pre-trigger ring buffer
- **Suspend/resume**: graceful memory pressure response without losing allocator state

## Confidence

- **High**: The struct layouts, allocation strategy, and memory management
  primitives (mmap, munmap, mprotect, madvise) are directly visible in the
  decompiled code and confirmed by assertion strings.
- **Medium**: The exact field names and some offset interpretations are inferred
  from usage patterns and assertion messages rather than debug symbols.
- **Low**: The integration with the trigger engine's pre-trigger buffer is
  inferred from the trimming logic and DequeArena::FreeBefore/FreeAfter
  semantics, not directly confirmed.
