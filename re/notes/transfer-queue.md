# TransferQueue Class -- Async USB Bulk Transfer Management

Analysis date: 2026-03-09
Binary: libgraph_server_shared.so (48MB, stripped, ELF 64-bit x86-64)
Source: Saleae Logic 2 v2.4.40
Source file: `/workspace/monorepo/graph-io/devices/usb_device/src/LinuxUsbDevice.cpp`


## Summary

`TransferQueue` is an anonymous-namespace class in `LinuxUsbDevice.cpp` that
manages a pool of pre-allocated `libusb_transfer` objects for high-throughput
asynchronous USB bulk streaming. It is the core mechanism that keeps EP2 IN
(0x82) saturated during capture, maintaining multiple in-flight transfers to
mask USB latency and ensure continuous data flow from the FPGA.

The class implements dynamic queue capacity reduction: when a
`libusb_submit_transfer` call fails, the queue shrinks rather than aborting
the entire capture. This provides graceful degradation under resource pressure.


## Constructor Signature

```cpp
(anonymous namespace)::TransferQueue::TransferQueue(
    libusb_device_handle &handle,   // param 1: libusb device handle
    UsbEndpoint endpoint,            // param 2: target endpoint (EP2 IN)
    uint32_t queue_depth,            // param 3: max simultaneous transfers
    uint32_t transfer_size,          // param 4: bytes per transfer buffer
    uint64_t timeout_ms,             // param 5: per-transfer timeout
    uint64_t max_transfer_bytes      // param 6: total byte cap (or 0=unlimited)
)
```

### Parameter Analysis

**Parameter 1: `libusb_device_handle&`** -- The open USB device handle.
Passed by reference, shared with the rest of the USB code.

**Parameter 2: `UsbEndpoint`** -- Specifies the bulk endpoint to use. For
capture data streaming this is EP2 IN (0x82). The `UsbEndpoint` type is a
Saleae abstraction that wraps the raw endpoint number.

**Parameter 3: `uint32_t` (queue_depth / queue_capacity)** -- Maximum number
of simultaneous in-flight transfers. This is the "queue_capacity" referenced
in the assertion `queue_capacity > 0` and the log string `queue_size={},
queue_capacity={}`. When `libusb_submit_transfer` fails, this value is
dynamically reduced: "Failed to submit USB transfer, reducing queue capacity
to {}".

**Parameter 4: `uint32_t` (transfer_size)** -- Size of each individual USB
bulk transfer buffer in bytes. This is the value computed by
`LogicDevice::ComputeTransferSize(U64)` and logged as "Starting bulk read
thread, transfer_size={}, timeout={}".

**Parameter 5: `uint64_t` (timeout_ms)** -- Per-transfer timeout in
milliseconds. Logged alongside transfer_size in the "Starting bulk read
thread" message. When a transfer times out, the error path
"BulkReadThread: Timeout waiting for transfer to complete." is triggered.

**Parameter 6: `uint64_t` (max_transfer_bytes)** -- Likely a total byte
limit for the entire streaming session, or a secondary timeout/limit. Could
also be a minimum guaranteed completion count. Exact semantics need Ghidra
decompilation to confirm.

### Evidence for Parameter Mapping

- Constructor is called from `ReadStream_QueueBuffersThread`, which logs
  `transfer_size` and `timeout` -- these match params 4 and 5.
- The assertion `queue_capacity > 0` confirms param 3 controls pool size.
- The `transfer_queue.queue_size < transfer_queue.queue_capacity` assertion
  confirms both `queue_size` (current in-flight count) and `queue_capacity`
  (max pool size) are tracked as members.


## Internal State

From assertion strings and log messages, the TransferQueue maintains:

| Member | Type | Purpose |
|--------|------|---------|
| `queue_size` | uint32_t | Current number of in-flight transfers |
| `queue_capacity` | uint32_t | Maximum simultaneous transfers (can shrink) |
| `transfer_size` | uint32_t | Bytes per transfer buffer |
| `mBuffer` | pointer | Buffer for each transfer (`transfer.mBuffer != nullptr`) |
| `mComplete` | int | Completion flag per transfer (`transfer.mComplete == 0/1`) |

The assertion `transfer_queue.queue_size < transfer_queue.queue_capacity`
appears in the submit path, enforcing that we never exceed the pool limit.

The assertions `transfer.mBuffer != nullptr` and `transfer.mComplete == 1`
suggest each transfer object has at least these two tracked fields, likely
in a per-transfer metadata struct.


## Lifecycle

### Construction (Capture Start)

```
LogicDevice::StartSampling
  -> ComputeTransferSize(U64)    [compute optimal buffer size]
  -> SendStartCommand             [tell FX3 to begin data flow]
  -> ReadStream_QueueBuffersThread starts
     -> TransferQueue::TransferQueue(handle, endpoint, depth, size, timeout, max)
     -> allocate DMA buffers via libusb_dev_mem_alloc (zero-copy)
     -> allocate USB 3.0 streams via libusb_alloc_streams
     -> submit initial batch of transfers via libusb_submit_transfer
     -> enter event loop: libusb_handle_events_timeout_completed
```

### Steady State (During Capture)

```
Event loop:
  libusb_handle_events_timeout_completed  [blocks until transfer completes]
    -> on_libusb_transfer_complete        [callback for each completed transfer]
       -> check transfer->status
       -> if LIBUSB_TRANSFER_COMPLETED:
            deliver buffer to OnDataBuffer callback
            resubmit transfer (libusb_submit_transfer)
       -> if error:
            handle per error type (see Error Handling)
```

The callback `on_libusb_transfer_complete` is a static function in the
anonymous namespace:
```
void (anonymous namespace)::on_libusb_transfer_complete(libusb_transfer *)
```

### Destruction (Capture Stop)

```
StopBulkReadStream
  -> "Stopping bulk read stream {}, block_until_stopped={}"
  -> "stopping stream"
  -> "Cancelling transfers"
  -> for each in-flight transfer:
       "Cancelling {} id={} transfer_count={}"
       libusb_cancel_transfer
  -> wait for all cancellations to complete
  -> "Freeing transfer {}, total_complete={}"
  -> free DMA buffers via libusb_dev_mem_free
  -> free USB 3.0 streams via libusb_free_streams
  -> "Done with usb stream, {} successful transfers"
```


## Thread Model

TransferQueue operates in a dedicated thread:

```
static void LinuxUsbDevice::ReadStream_QueueBuffersThread(
    UsbDevice::BulkReadStream &stream,
    (anonymous namespace)::TransferQueue &queue,
    std::atomic<ReadStreamError> &error_state
)
```

Three parameters:
1. `BulkReadStream&` -- The stream object that owns this thread
2. `TransferQueue&` -- The transfer pool to drive
3. `std::atomic<ReadStreamError>&` -- Shared error state for cross-thread
   error reporting

The thread contains a lambda (`$_1`) that is likely the inner event loop
or a cleanup handler.

### Synchronization

The thread communicates with the main thread via:
- `std::atomic<ReadStreamError>` for error state propagation
- Condition variable signaling (from mutex/condvar strings in context)
- "BulkReadThread: Waiting for memory to be available and for the queue to
  have availability" -- blocks when memory pressure prevents new submissions
- "BulkReadThread: Waiting for read requests to be queued" -- blocks when
  no transfers are ready to submit
- "locking mTaskQueueMutex" -- a mutex protects the task queue


## Error Handling

### Transfer Completion Status Codes

The `on_libusb_transfer_complete` callback handles these libusb status codes:

| Status | String in Binary | Action |
|--------|-----------------|--------|
| `LIBUSB_TRANSFER_COMPLETED` | "LIBUSB_SUCCESS / LIBUSB_TRANSFER_COMPLETED" | Deliver data, resubmit |
| `LIBUSB_TRANSFER_ERROR` | "LIBUSB_TRANSFER_ERROR" | Set ReadStreamError |
| `LIBUSB_TRANSFER_TIMED_OUT` | "LIBUSB_TRANSFER_TIMED_OUT" | Log timeout, may retry |
| `LIBUSB_TRANSFER_CANCELLED` | "LIBUSB_TRANSFER_CANCELLED" | Expected during stop, cleanup |
| `LIBUSB_TRANSFER_STALL` | "LIBUSB_TRANSFER_STALL" | Clear halt (`libusb_clear_halt`), retry |
| `LIBUSB_TRANSFER_NO_DEVICE` | "LIBUSB_TRANSFER_NO_DEVICE" | Fatal: device disconnected |
| `LIBUSB_TRANSFER_OVERFLOW` | "LIBUSB_TRANSFER_OVERFLOW" | Data lost, report to stream |

### Dynamic Queue Capacity Reduction

When `libusb_submit_transfer` fails, the queue degrades gracefully instead
of aborting:

```
"Failed to submit USB transfer, reducing queue capacity to {}"
```

This means if a system is under resource pressure (e.g., low DMA buffer
availability, too many concurrent transfers), the TransferQueue reduces its
`queue_capacity` rather than failing the entire capture. This allows capture
to continue at reduced throughput.

### Stall Recovery

```
"detected endpoint stall"
"USB Stall"
```

When a STALL condition is detected, `libusb_clear_halt` is called on the
endpoint to recover. This is a standard USB error recovery mechanism where
the host sends a CLEAR_FEATURE(ENDPOINT_HALT) to the device.

### Timeout Handling

```
"BulkReadThread: Timeout waiting for transfer to complete."
```

Per-transfer timeouts prevent the thread from blocking indefinitely on a
single transfer. The timeout value is passed as parameter 5 to the
constructor.

### Event Loop Error Recovery

```
"libusb_handle_events failed: %s, cancelling transfer and retrying"
```

This is from the internal libusb event loop. If `libusb_handle_events` fails,
the current transfer is cancelled and retried. This provides resilience
against transient kernel-level USB errors.

### Device Closure Edge Cases

```
"Device handle closed while transfer was still being processed,
 but the device is still connected as far as we know"
"A cancellation for an in-flight transfer hasn't completed but
 closing the device handle"
"A cancellation hasn't even been scheduled on the transfer for
 which the device is closing"
```

These strings reveal careful handling of the race condition between device
closure and in-flight transfer completion. The code tracks whether
cancellations have been issued and completed before proceeding with cleanup.

### Transfer Size Mismatch

```
"USB Transfer Error, expected transfer size={}, actual transfer size={}"
```

Completed transfers are validated: the actual number of bytes transferred
must match the expected transfer size. A mismatch indicates a protocol
error or short transfer.


## Transfer Size Computation

The transfer size is computed before TransferQueue construction by the
virtual method `LogicDevice::ComputeTransferSize(U64)`:

```
virtual U32 LogicDevice::ComputeTransferSize(U64)
```

Each device type has its own override:
- `LogicDevice` (base)
- `LogicGraduateDevice` (Logic 8)
- `LogicStudentDevice` (Logic 4)
- `LogicProfessionalDevice` (Logic Pro 8/16)

### Computation Factors

1. **USB speed**: detected via `libusb_get_device_speed`
   - "USB Full Speed" (12 Mbps)
   - "USB High Speed" (480 Mbps, USB 2.0)
   - "USB Super Speed" (5 Gbps, USB 3.0)

2. **Sample rate and channel count**: more channels or higher rates require
   larger transfer buffers to amortize USB overhead.

3. **USB bandwidth constraints**: available bandwidth limits maximum size.

### Transfer Size Adjustment

```
"ideal transfer size {}"          -- initial computed value
"AdjustTransferSize"              -- applies further constraints
"Transfer size is: {}"            -- final value
", capping transfer size."        -- when bounded by a maximum
```

### Environment Override

```
"SALEAE_USB_TRANSFER_LIMIT"
"SALEAE_USB_TRANSFER_LIMIT used to limit transfer size to {}"
```

The `SALEAE_USB_TRANSFER_LIMIT` environment variable caps the transfer size.
This is a debugging/tuning knob that overrides the computed optimal size.


## Memory Management

### MemoryPool

Transfer buffers are managed by `Saleae::MemoryPool`:

```
Saleae::MemoryPool::MemoryPool(Saleae::MemoryPool::Configuration)
```

Source: `/workspace/monorepo/graph-io/devices/usb_device/src/usb_device/memory_pool.cpp`

The MemoryPool provides:
- Pre-allocated buffer regions via `mmap` (confirmed by `munmap(mAllocation, mCapacity) == 0`)
- Committed/uncommitted memory tracking (`committed_pivot >= mAllocation && committed_pivot < mAllocationEnd`)
- Buffer size validation (`transfer.mBuffer != nullptr`)
- Memory-pressure-aware allocation (the "Waiting for memory to be available"
  path blocks when the pool is exhausted)

### DMA Buffers (Zero-Copy)

When available, `libusb_dev_mem_alloc` is used to allocate DMA-capable
buffers. This enables zero-copy transfers where the USB host controller
DMAs directly into the application's buffer, avoiding a kernel-to-userspace
copy.

Symbols present:
- `libusb_dev_mem_alloc` @ 0x2dbbaa0
- `libusb_dev_mem_free` @ 0x2dbbad0

Fallback: If DMA allocation fails, standard memory allocation is used.

### USB 3.0 Bulk Streams

For USB 3.0 connections, bulk streams are allocated:
- `libusb_alloc_streams` @ 0x2dbb960
- `libusb_free_streams` @ 0x2dbba10
- `libusb_transfer_set_stream_id` @ 0x2dbfc70
- `libusb_transfer_get_stream_id` @ 0x2dbfc80

USB 3.0 bulk streams allow multiple parallel streams on a single endpoint,
further increasing throughput by enabling the host controller to pipeline
transfers more efficiently.


## ReadStreamError Enum

The error state is communicated via `std::atomic<ReadStreamError>`:

```
UsbDevice::ReadStreamError_impl::ReadStreamError
```

Error callbacks are registered per device type (from mangled symbols):
- `LogicDevice` binds `ReadStreamError` callback
- `Logic16Device` binds `ReadStreamError` callback
- `LogicGraduateDevice` binds `ReadStreamError` callback
- `LogicProfessionalDevice` binds `ReadStreamError` callback

Additional error types for MSO devices:
- `MsoInterruptReadStreamError` -- interrupt endpoint stream errors
- `MsoBulkReadStreamError` -- bulk endpoint stream errors

The `TransferSubmissionFailure` string suggests a specific error enum value
for when `libusb_submit_transfer` fails.


## Data Flow Through TransferQueue

```
          +-- libusb_submit_transfer ----+
          |                              |
          v                              |
     USB Host Controller             TransferQueue
     (DMA to buffer)                  queue_size tracking
          |                              |
          v                              |
     libusb event loop              on_libusb_transfer_complete
     (libusb_handle_events)              |
          |                         +----+----+
          v                         |         |
     completion callback       Deliver    Resubmit
          |                    buffer     transfer
          v                      |            |
     OnDataBuffer                v            v
          |                 Graph         Back to
          v                 pipeline      USB HW
     DisaggregatorNode
          |
          v
     Per-channel data
```

### Flow Summary

1. TransferQueue submits N transfers (queue_depth) via `libusb_submit_transfer`
2. USB host controller DMAs data from device into pre-allocated buffers
3. libusb event loop (`libusb_handle_events_timeout_completed`) detects completion
4. `on_libusb_transfer_complete` callback fires
5. If successful: deliver buffer to `OnDataBuffer`, then resubmit the transfer
6. If error: handle per status code, potentially reduce queue capacity
7. Loop continues until `StopBulkReadStream` is called


## Relationship to BulkReadStream

`UsbDevice::BulkReadStream` is the higher-level abstraction that owns the
`TransferQueue`:

```
UsbDevice::BulkReadStream
  |- mReadStreamId (optional, presence asserted via !mReadStreamId.has_value())
  |- TransferQueue (the actual transfer pool)
  |- ReadStreamError atomic (shared with thread)
  |
  +-- ReadStream_QueueBuffersThread (spawned thread)
       |- drives the TransferQueue event loop
       |- delivers data via callbacks
```

Lifecycle methods:
- `BulkReadStream_Read` -- initiate a read
- `BulkReadStream_Request` -- request data from the stream
- `StopBulkReadStream` -- stop the stream, cancel transfers

Log messages during lifecycle:
```
"Stopping bulk read stream {}, block_until_stopped={}"
```

The `block_until_stopped` parameter controls whether the stop is synchronous
(waits for all cancellations) or asynchronous (signals stop and returns).


## MSO Device Streams

MSO devices have two separate stream types:
1. **Bulk stream** (same as Logic Pro) -- for sample data
2. **Interrupt stream** -- for firmware event notifications

```
"Starting interrupt read stream for ep {:x} {:x}"
"Failed to find MSO interrupt endpoint"
"Failed to find MSO bulk endpoint"
```

The interrupt stream uses `libusb_interrupt_transfer` (not the async API),
and has its own error type `MsoInterruptReadStreamError`.


## Confidence Assessment

| Finding | Confidence | Sources |
|---------|-----------|---------|
| TransferQueue is anonymous namespace in LinuxUsbDevice.cpp | **Confirmed** | Mangled symbol `_GLOBAL__N_1::TransferQueue` |
| Constructor takes 6 parameters (handle, ep, u32, u32, u64, u64) | **Confirmed** | Demangled signature string in binary |
| queue_size and queue_capacity are tracked members | **Confirmed** | Assertion `transfer_queue.queue_size < transfer_queue.queue_capacity` |
| Dynamic capacity reduction on submit failure | **Confirmed** | "Failed to submit USB transfer, reducing queue capacity to {}" |
| on_libusb_transfer_complete is the callback | **Confirmed** | Demangled function name string |
| ReadStream_QueueBuffersThread is the driver thread | **Confirmed** | Demangled static method signature |
| DMA buffers via libusb_dev_mem_alloc | **Confirmed** | Exported symbols at known addresses |
| USB 3.0 streams via libusb_alloc_streams | **Confirmed** | Exported symbols at known addresses |
| Transfer size computed by virtual ComputeTransferSize | **Confirmed** | Demangled virtual method signature |
| MemoryPool from memory_pool.cpp manages buffers | **Confirmed** | Source path string + MemoryPool constructor string |
| Per-transfer mBuffer and mComplete members | **Confirmed** | Assertion strings `transfer.mBuffer != nullptr`, `transfer.mComplete == 1` |
| Param 3 = queue depth, param 4 = transfer size | **High** | Log strings correlate transfer_size with constructor call |
| Param 5 = timeout | **High** | Log "transfer_size={}, timeout={}" correlates with two params |
| Param 6 = max bytes or secondary limit | **Hypothesis** | Two uint64_t params, one is timeout, other is unknown |


## Open Questions

1. **Exact queue_depth default value**: What is the typical number of
   simultaneous in-flight transfers? Needs Ghidra decompilation of the
   call site where TransferQueue is constructed.

2. **Parameter 6 semantics**: Is it a total byte limit for the session,
   a secondary timeout, or something else? The two uint64_t parameters
   need Ghidra analysis to distinguish.

3. **Capacity reduction floor**: What is the minimum queue_capacity the
   dynamic reduction can reach? Is it 1, or does it have a floor?

4. **Transfer resubmission policy**: After a STALL or TIMEOUT, is the
   same transfer buffer resubmitted, or is a fresh buffer allocated?

5. **USB 3.0 stream count**: How many USB 3.0 bulk streams are allocated?
   The `libusb_alloc_streams` call needs its count parameter extracted.

6. **MemoryPool Configuration struct**: What fields does the Configuration
   struct contain? (initial_size, max_size, alignment, use_dma?)

7. **ComputeTransferSize exact formulas**: The device-specific transfer
   size computation formulas need extraction from Ghidra decompilation
   per device type.


## Ghidra Analysis Targets

The `TraceTransferQueue.py` Ghidra script targets these for extraction:

1. **TransferQueue constructor body**: Extract struct member offsets and
   parameter-to-member assignments to confirm the parameter mapping.

2. **on_libusb_transfer_complete body**: Extract the switch/if-chain on
   `transfer->status` to map all handled status codes to their actions.

3. **ReadStream_QueueBuffersThread body**: Extract the event loop
   structure, blocking conditions, and error propagation paths.

4. **ComputeTransferSize bodies**: Extract the arithmetic and constants
   used to compute transfer sizes per device type.

5. **TransferQueue construction call site**: Extract the actual values
   passed for queue_depth, transfer_size, timeout, and max_bytes.


## Source Code References

```
/workspace/monorepo/graph-io/devices/usb_device/src/LinuxUsbDevice.cpp
/workspace/monorepo/graph-io/devices/usb_device/src/UsbDevice.cpp
/workspace/monorepo/graph-io/devices/usb_device/src/usb_device/memory_pool.cpp
```
