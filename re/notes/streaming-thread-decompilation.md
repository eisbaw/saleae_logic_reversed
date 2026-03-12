# ReadStream_QueueBuffersThread Decompilation Analysis

Analysis date: 2026-03-12
Binary: libgraph_server_shared.so (48MB, stripped, ELF 64-bit x86-64)
Source: Saleae Logic 2 v2.4.40
Task: TASK-0131
Method: Ghidra headless decompilation at known addresses + string xref

## Decompiled Functions

All decompiled output is in `re/ghidra/decompiled-streaming-thread.txt` and
`re/ghidra/decompiled-streaming-thread2.txt`.

| Function | ELF VA | Ghidra VA | Size | Purpose |
|----------|--------|-----------|------|---------|
| FUN_02e93900 | 0x2d93900 | 0x2e93900 | 3865 bytes | BulkReadStream_Read outer event loop |
| FUN_02e94ae0 | 0x2d94ae0 | 0x2e94ae0 | 2579 bytes | ReadStream_QueueBuffersThread inner submit loop |
| FUN_02e95650 | 0x2d95650 | 0x2e95650 | 110 bytes | on_libusb_transfer_complete callback |
| FUN_02e92690 | 0x2d92690 | 0x2e92690 | 594 bytes | SendControlTransfer (xref for bytes_transferred check) |


## Architecture: Two-Thread Model

The Saleae streaming implementation uses TWO threads working together:

### Thread 1: "BulkReadStream_Read" (FUN_02e93900) -- Outer Event Loop

This is the consumer thread that processes completed transfers:

```
1. Create TransferQueue (allocate queue_depth * 40 bytes metadata)
2. Spawn Thread 2 (ReadStream_QueueBuffersThread)
3. Loop:
   a. Lock mutex
   b. Wait on condition variable (200ms poll timeout)
      - Exit conditions: error, stop requested, data available
      - Log "BulkReadThread: Waiting for read requests to be queued" every 5th wait
   c. Unlock mutex
   d. Call libusb_handle_events_timeout_completed(ctx, timeout, &completed_flag)
      - timeout = transfer_timeout_ms (1000ms)
   e. If completed_flag set:
      - Increment total_complete counter (local_140)
      - Extract buffer pointer from transfer metadata (offset +0x08)
      - Extract actual_length from libusb_transfer (offset +0x18)
      - Clear buffer from slot: set shared_ptr to null
      - Lock mutex, advance ring index: (index + 1) % queue_depth, decrement in_flight
      - Unlock mutex, notify condition variable
      - Deliver (buffer, actual_length) to OnDataBuffer callback via vtable+0x30
   f. If timeout (no completion within timeout_ms):
      - Check if any data was received previously (local_98 flag)
      - If first timeout: report error code 1 (Timeout)
      - If data was received but stale: report error code 2 or 1 based on
        elapsed time vs stream->timeout threshold
      - Report to ReadStreamError via FUN_02e9aa10
   g. If libusb_handle_events returned error:
      - Log "failure while waiting for transfer" with error details
      - If error == -7 (LIBUSB_ERROR_TIMEOUT): check staleness, report timeout
      - Otherwise: report error code 5 (Fatal)
   h. If transfer->status != 0 (from libusb_transfer struct at +0x10):
      - Log "transfer failure: {status}"
      - Report error code 5 (Fatal)
4. On exit (stop or error):
   a. Deliver final error state to callback
   b. Lock/unlock mutex, notify condition variable
   c. Join Thread 2
   d. Log "Cancelling transfers"
   e. For each transfer in queue (0..queue_capacity):
      - Log "Cancelling {i} id={id} transfer_count={count}"
      - Call libusb_cancel_transfer
      - If cancel returns 0: wait for completion via libusb_handle_events_timeout_completed
      - If cancel returns -5 (LIBUSB_ERROR_NOT_FOUND): already completed, skip
      - If cancel returns other error: log "libusb_cancel_transfer failed"
      - Log "Freeing transfer {i}, total_complete={count}"
      - Call libusb_free_transfer
   f. Log "Done with usb stream, {total_complete} successful transfers"
   g. Cleanup: destroy condition variables, free metadata array, free thread state
```

### Thread 2: "ReadStream_QueueBuffersThread" (FUN_02e94ae0) -- Submit Loop

This is the producer thread that allocates buffers and submits transfers:

```
1. Initialize:
   a. Extract TransferQueue parameters from parent struct
   b. Allocate MemoryPool (via FUN_02e968b0 with config struct)
   c. Allocate libusb_transfer objects (one per queue_capacity)
      - Each slot: store libusb_transfer*, set transfer_id, set mComplete=1
      - If libusb_alloc_transfer fails and i==0: assert("i > 0") -- fatal
      - If libusb_alloc_transfer fails and i>0: reduce queue_capacity to i
2. Submit loop:
   a. Lock mutex
   b. Wait on condition variable:
      - Exit conditions: stop requested, error reported, OR
        (memory available AND queue_size < queue_capacity)
      - Memory check: FUN_02e9da00() (MemoryPool::HasAvailable)
      - Log "BulkReadThread: Waiting for memory to be available..." every 5th wait
      - Memory starvation detection: if memory unavailable for too long,
        record timestamp and set a "starvation" flag
   c. Assert: transfer_queue.queue_size < transfer_queue.queue_capacity
   d. Inner submit loop (while memory available and queue not full):
      i.   Compute slot index: (queue_size + offset) % queue_capacity
      ii.  Assert: transfer.mComplete == 1 (slot must be done from previous use)
      iii. Set mComplete = 0
      iv.  Acquire buffer from MemoryPool (FUN_02e9da80)
      v.   Assert: transfer.mBuffer != nullptr
      vi.  Get endpoint number from parent struct
      vii. Get endpoint address byte (FUN_02e9aab0)
      viii. Fill libusb_transfer:
           - dev_handle = parent->handle (offset +0x00)
           - endpoint = address_byte (offset +0x09)
           - type = 2 (LIBUSB_TRANSFER_TYPE_BULK, offset +0x0a)
           - timeout = 0 (offset +0x0c, no per-transfer timeout!)
           - buffer = acquired_buffer (offset +0x30, field 6)
           - length = transfer_size (offset +0x14)
           - user_data = &transfer_slot (offset +0x28, field 5)
           - callback = on_libusb_transfer_complete (offset +0x20, field 4)
      ix.  Call libusb_submit_transfer
      x.   If submit succeeds: increment queue_size, check memory again
      xi.  If submit fails with -11 (LIBUSB_ERROR_BUSY):
           - First time only: reduce queue_capacity to current queue_size
           - Log "Failed to submit USB transfer, reducing queue capacity to {}"
      xii. If submit fails with other error:
           - Log "libusb_submit_transfer failed: {error}"
           - Throw exception (fatal, aborts the thread)
   e. Unlock mutex, notify condition variable
   f. If first submission round: check if device is still connected
      via callback at lVar23+0x60 (vtable+0x30). If device gone,
      report error code 4 (DeviceDisconnected)
   g. Log "Queue thread ending" on exit
```

### Thread 3: "on_libusb_transfer_complete" (FUN_02e95650) -- Callback

Called by libusb event handling from Thread 1's context:

```
1. Get user_data from libusb_transfer (offset +0x28)
2. Assert: user_transfer->mComplete == 0 (must not be already marked complete)
3. Set user_transfer->mComplete = 1
4. Increment user_transfer->transfer_count (offset +0x20)
```

The callback is intentionally minimal. It only marks the transfer as complete.
All actual data processing happens in Thread 1 after
`libusb_handle_events_timeout_completed` returns.


## Transfer Metadata Layout (40 bytes per entry)

```
Offset  Size  Field
0x00    8     libusb_transfer* pointer
0x08    8     buffer shared_ptr (data pointer)
0x10    8     buffer shared_ptr (control block / weak count)
0x18    4     mComplete flag (0=in-flight, 1=done)
0x1C    4     transfer_id (sequential counter per slot)
0x20    8     transfer_count (total completions on this slot)
```


## ReadStreamError Enum Values (from FUN_02e9aa10 call patterns)

| Code | Meaning | Trigger |
|------|---------|---------|
| 1 | Timeout | No data received within timeout_ms |
| 2 | StaleTimeout | Data was received previously but stopped |
| 4 | DeviceDisconnected | Device connectivity check failed |
| 5 | Fatal | libusb error, transfer failure, submit failure |


## Key Implementation Details

### No Per-Transfer Timeout

The libusb_transfer timeout field is set to 0 (no timeout). Timeout is handled
at a higher level by the outer thread (Thread 1) using
`libusb_handle_events_timeout_completed` with a 1-second timeout.

### Memory Pool Integration

The submit thread (Thread 2) acquires buffers from a MemoryPool before
submitting. If the pool is exhausted, the thread blocks until buffers are
returned (from completed transfers in Thread 1). This provides backpressure.

### Dynamic Queue Capacity Reduction

Only happens on LIBUSB_ERROR_BUSY (-11), and only on the first occurrence.
The queue_capacity is reduced to the current queue_size (number of
successfully submitted transfers). This gracefully handles systems where
the kernel URB limit is hit.

### Cancellation Order

When stopping, transfers are cancelled sequentially (0..queue_capacity),
NOT in reverse order. Each cancellation is followed by waiting for its
completion before proceeding to the next. This ensures all DMA buffers
are safe to free.

### Ring Buffer Index

The completion ring uses modular arithmetic: `(current + 1) % queue_depth`.
This allows the outer thread to process completions in order regardless
of which transfer completes first (libusb guarantees in-order completion
for bulk transfers on the same endpoint).


## Comparison with Our nusb Approach

### Current State (capture.rs)

Our `CaptureStream` uses synchronous, single-transfer reads:
```rust
fn read_chunk(&mut self, transport: &dyn UsbTransport) -> Result<Option<DataChunk>> {
    let n = transport.bulk_transfer_in(EP2_CAPTURE_IN, &mut self.read_buf)?;
    // parse and return
}
```

This is fundamentally single-buffered: while we parse data, no USB transfer
is pending. At high sample rates, we will miss USB microframes and the
device's FIFO will overflow.

### nusb::transfer::Queue

nusb provides `Queue<RequestBuffer>` which is the direct equivalent of
Saleae's TransferQueue. It handles:
- Multiple outstanding transfers
- Buffer reuse (via `RequestBuffer::reuse`)
- Proper cancellation ordering (reverse order)
- Cancel-safe futures

### Recommended Architecture

```rust
pub struct CaptureStream {
    queue: nusb::transfer::Queue<RequestBuffer>,
    transfer_config: TransferConfig,  // from transfer_queue.rs
    // ... parsing state
}

impl CaptureStream {
    fn new(interface: &nusb::Interface, config: &CaptureConfig) -> Self {
        let tc = TransferConfig::from_capture(
            config.sample_rate,
            config.digital_channels.len() as u8,
            config.analog_channels.len() as u8,
            2, // bytes_per_sample for analog
            config.usb_speed,
        );

        let mut queue = interface.bulk_in_queue(EP2_CAPTURE_IN);

        // Pre-submit queue_depth transfers
        for _ in 0..tc.queue_depth {
            queue.submit(RequestBuffer::new(tc.transfer_size));
        }

        Self { queue, transfer_config: tc, /* ... */ }
    }

    fn read_chunk(&mut self) -> Result<Option<DataChunk>> {
        let completion = block_on(self.queue.next_complete());

        if let Err(e) = completion.status {
            match e {
                TransferError::Stall => {
                    self.queue.clear_halt()?;
                    // Re-submit and retry
                    self.queue.submit(RequestBuffer::reuse(completion.data, self.transfer_config.transfer_size));
                    return self.read_chunk();
                }
                TransferError::Cancelled => return Ok(None),
                _ => return Err(e.into()),
            }
        }

        let data = &completion.data;
        let chunk = parse_data_chunk(data, ...)?;

        // Re-submit the buffer for the next transfer
        self.queue.submit(RequestBuffer::reuse(completion.data, self.transfer_config.transfer_size));

        Ok(Some(chunk))
    }

    fn stop(&mut self) {
        self.queue.cancel_all();
        // Drain remaining completions
        while self.queue.pending() > 0 {
            let _ = block_on(self.queue.next_complete());
        }
    }
}
```

### Missing Edge Cases in Our Current Code

1. **No transfer queue**: Single-buffered reads guarantee FIFO overflow at
   any non-trivial sample rate. Must use nusb Queue with 128 outstanding
   transfers.

2. **No transfer size computation**: We hardcode 16 KiB. Must use
   `TransferConfig::from_capture()` which already correctly implements the
   Saleae formula (bandwidth/240, block-aligned, capped at 1.6 MB).

3. **No stall recovery**: Original clears halt and retries. Our code would
   just return an error.

4. **No timeout detection**: The original distinguishes between "never
   received data" (timeout=1) and "data stopped coming" (stale timeout=2).
   We should track whether we've received any data and report accordingly.

5. **No transfer size validation**: The original checks
   `bytes_transferred <= data_length` (actual <= expected). We should
   log warnings on short transfers.

6. **No graceful queue reduction**: On LIBUSB_ERROR_BUSY, the original
   reduces queue depth instead of failing. With nusb, this shouldn't be
   needed (nusb handles kernel limits internally), but we should handle
   submit failures gracefully.

7. **Buffer reuse**: nusb's `RequestBuffer::reuse()` avoids allocation.
   Currently we allocate a new Vec for every read.


## Confidence Assessment

| Finding | Confidence | Evidence |
|---------|-----------|----------|
| Two-thread model (outer events + inner submit) | **Confirmed** | pthread_create in FUN_02e93900, two distinct function bodies |
| on_libusb_transfer_complete only sets flag | **Confirmed** | Decompiled FUN_02e95650: only writes mComplete=1 and increments counter |
| Transfer metadata is 40 bytes per entry | **Confirmed** | `operator_new(queue_depth * 0x28)` and array access patterns |
| No per-transfer timeout (timeout=0 in struct) | **Confirmed** | `*(undefined4 *)((long)puVar9 + 0xc) = 0` in fill code |
| Queue capacity reduction only on BUSY (-11) | **Confirmed** | `if (iVar15 != -0xb)` check before reduction |
| Sequential cancellation (not reverse) | **Confirmed** | `do { ... uVar20 = uVar20 + 1; } while (uVar20 < local_80)` |
| Memory pool integration for backpressure | **High** | FUN_02e9da00/FUN_02e9da80 called before submit, condvar wait |
| Error codes 1/2/4/5 mapping | **High** | FUN_02e9aa10 called with integer constants at each error path |
| 200ms condition variable poll timeout | **Confirmed** | `local_90 = 200` before condvar wait loop |
