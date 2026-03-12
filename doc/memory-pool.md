# MemoryPool Architecture (Decompiled)

Source: `/workspace/monorepo/graph-io/devices/usb_device/src/usb_device/memory_pool.cpp`

Binary: `libgraph_server_shared.so` from Saleae Logic 2 v2.4.40

## Overview

`Saleae::MemoryPool` is a thread-safe, fixed-chunk-size buffer pool used by
the streaming thread to supply buffers for USB bulk transfers. It manages a
free list of pre-allocated byte buffers, recycles returned buffers, and
blocks callers when the pool is exhausted.

The pool is created once per capture session and shared between the buffer
submission thread (ReadStream_QueueBuffersThread) and the transfer completion
callback. Buffers flow: pool -> submit -> USB HW -> complete -> consumer ->
pool (recycled via shared_ptr custom deleter).

## Struct Layout

Reconstructed from decompiled constructor at ELF VA 0x2d9d770 (Ghidra
0x2e9d770) and accessor methods:

| Offset | Type | Name | Purpose |
|--------|------|------|---------|
| +0x00 | shared_ptr internals | | Pool shared ownership data |
| +0x10 | std::mutex | mMutex | Protects free list and counters |
| +0x38 | void** | mFreeList.begin | Start of free list array |
| +0x40 | void** | mFreeList.end | Current top of free list (push/pop here) |
| +0x48 | void** | mFreeList.capacity | End of allocated free list storage |
| +0x50 | u32 | mConfiguration.mChunkSize | Bytes per buffer chunk |
| +0x54 | u32 | config field 2 | (copied from Configuration) |
| +0x58 | u32 | mConfiguration.mMaxChunks | Max buffers (asserted > 0) |
| +0x5C | u32 | config field 4 | (copied from Configuration) |
| +0x59-0x61 | | config bytes | Additional configuration data |
| +0x60 | u64 | mMaxFreeChunks | Free list cap (when enabled) |
| +0x68 | bool | mEnableFreeListCap | Whether to limit free list size |
| +0x70 | (inline) | mAllocator inline storage | std::function inline buffer |
| +0x90 | std::function* | mAllocator | Custom allocator callback |
| +0xA0 | u64 | mAllocatedCount | Number of outstanding chunks |

## Configuration Struct

```cpp
struct MemoryPool::Configuration {
    uint32_t field0;          // offset 0x00
    uint32_t field1;          // offset 0x04
    uint32_t mMaxChunks;      // offset 0x08 (asserted > 0)
    uint32_t field3;          // offset 0x0C
    // bytes at 0x09-0x18: additional config
    void* mAllocator;         // offset 0x40 (std::function, 0x20 bytes)
};
```

The constructor asserts `mConfiguration.mMaxChunks > 0` (line 9 of
memory_pool.cpp).

## Methods

### Allocate (ELF 0x2d9da80, 254 bytes)

```
shared_ptr<uint8_t[]> MemoryPool::Allocate()
```

1. Lock mMutex.
2. If free list non-empty (mFreeList.end != mFreeList.begin):
   - Pop buffer from end of free list (LIFO).
   - Unlock mutex.
3. If free list empty:
   - If mAllocatedCount == mMaxChunks: return nullptr (null shared_ptr).
   - Increment mAllocatedCount.
   - Unlock mutex.
   - Allocate new buffer: `operator new(mChunkSize)`.
4. Wrap buffer in shared_ptr with custom deleter (lambda `$_0`).
5. The deleter calls Deallocate() when the shared_ptr ref count drops to 0.

### Deallocate / Return (ELF 0x2d9dbc0, 515 bytes)

```
void MemoryPool::Deallocate(void* buffer)
```

1. Acquire weak_ptr lock on pool (pool may be destroyed while buffers
   outstanding).
2. If pool is dead: `operator delete(buffer)`, return.
3. Lock mMutex.
4. If free list cap enabled AND free list size >= mMaxFreeChunks:
   - Decrement mAllocatedCount.
   - `operator delete(buffer)` (shrink pool).
5. Else: push buffer onto free list (grows vector if needed).
6. Unlock mutex.
7. If pool was previously empty (free list was empty AND allocated == max),
   notify condition variable via allocator callback.

### HasAvailable (ELF 0x2d9da00, 62 bytes)

```
bool MemoryPool::HasAvailable()
```

1. Lock mMutex.
2. If free list non-empty: return true.
3. If free list empty: return (mAllocatedCount < mMaxChunks).
4. Unlock mutex.

Used by ReadStream_QueueBuffersThread in the wait loop condition.

### FreeCount (ELF 0x2d9da40, 49 bytes)

```
size_t MemoryPool::FreeCount()
```

Returns number of buffers currently in the free list.

### Destructor (ELF 0x2d9d970, 132 bytes)

Iterates free list, calls `operator delete` on each buffer. Cleans up
allocator std::function.

## Streaming Thread Integration

In `ReadStream_QueueBuffersThread` (ELF 0x2d94ae0):

```
loop {
    lock(mutex);
    while true {
        // Wait with 200ms timeout (from Configuration.timeout * 1e6 ns)
        if HasAvailable() && queue_size < queue_capacity:
            break;
        if stop_requested || error:
            break;
        condvar.timed_wait(200ms);
        // Every 5th iteration, log "Waiting for memory..."
    }
    unlock(mutex);

    if stop || error: break;

    // Acquire buffer from pool
    buffer = MemoryPool::Allocate();
    assert(buffer != nullptr);  // "transfer.mBuffer != nullptr"

    // Fill libusb_transfer struct
    transfer.buffer = buffer;
    transfer.length = transfer_size;
    transfer.callback = on_libusb_transfer_complete;

    // Submit
    result = libusb_submit_transfer(transfer);
    if result != 0:
        if first_failure: reduce queue_capacity
        else: throw
}
```

The shared_ptr custom deleter ensures that when the transfer completion
callback finishes processing the data, the buffer automatically returns
to the MemoryPool. This creates a closed-loop recycling system.

## Pool Sizing

From the streaming thread setup:
- `mMaxChunks` = queue_depth (typically 128)
- `mChunkSize` = transfer_size (computed from bandwidth, 4KB to 1.6MB)
- Total pool memory = up to 128 * 1.6 MB = 204.8 MB

The pool allocates lazily: buffers are created on first use via
`operator new` (not mmap). The `munmap(mAllocation, mCapacity)` assertion
strings found in the binary belong to `FixedByteDeque`, not MemoryPool.

## nusb Equivalence

nusb's `Queue` + `RequestBuffer::reuse` provides the same functionality:

| MemoryPool | nusb equivalent |
|------------|-----------------|
| Allocate() returns buffer | `RequestBuffer::new(size)` or `RequestBuffer::reuse(vec, size)` |
| Custom deleter recycles | `completion.data` returns the Vec, caller reuses it |
| HasAvailable() blocks submission | `queue.pending() < n_transfers` controls submission |
| Free list (LIFO) | Vec allocation reuse via `RequestBuffer::reuse` |
| mMaxChunks = queue depth | Caller controls `n_transfers` count |

The key difference: MemoryPool uses a centralized pool with shared_ptr
reference counting for automatic return. nusb uses explicit buffer reuse
in the submit loop (`RequestBuffer::reuse(completion.data, size)`).

**No separate pool is needed in our Rust implementation.** The nusb Queue
pattern already handles buffer lifecycle. Our transfer_queue.rs constants
(QUEUE_DEPTH=128, transfer_size formula) are sufficient to replicate the
original behavior.
