# Transfer Queue Sizing and ComputeTransferSize Analysis

Analysis date: 2026-03-10
Binary: libgraph_server_shared.so (48MB, stripped, ELF 64-bit x86-64)
Source: Saleae Logic 2 v2.4.40
Task: TASK-0070
Method: Ghidra headless decompilation, vtable relocation analysis, string xref tracing


## Summary

The transfer size and queue depth are computed uniformly across all FX3/FPGA
device classes. There is a single shared `ComputeTransferSize` implementation
(vtable slot 11) and a single shared `AdjustTransferSize` function. Device
differences come from the USB speed function (vtable slot 6), which determines
the block alignment size.

There are NO per-device ComputeTransferSize overrides. The formula is:

```
bandwidth_bps = sum(active_digital_channels * digital_sample_rate_hz)
              + sum(active_analog_channels * analog_sample_rate_hz * bits_per_sample)
bandwidth_Bps = bandwidth_bps / 8

ideal_size = ceil(bandwidth_Bps / 240)
block_size = 4096  (USB 2.0) or 16384 (USB 3.0)
transfer_size = round_up(ideal_size, block_size)
transfer_size = min(transfer_size, env_limit & ~(block_size - 1))
transfer_size = max(transfer_size, block_size)

queue_depth = min(128, max_transfer_bytes / transfer_size)
timeout = 1000 ms
max_transfer_bytes = 5,000,000,000 (5 GB, effectively unlimited)
```


## ComputeTransferSize (Vtable Slot 11)

ELF VA: 0x1c50230 (Ghidra: 0x1d50230)
Source: `/workspace/monorepo/graph-io/devices/logic_interface/src/LogicAnalyzerDevice.cpp`
Assertion string: `"virtual U32 LogicDevice::ComputeTransferSize(U64)"`

This is a **shared implementation** in vtable slot 11, used by all device
classes:

| Device | Vtable Slot 11 Address |
|--------|----------------------|
| LogicGraduateDevice (Logic 8) | 0x01c50230 |
| LogicStudentDevice (Logic 4) | 0x01c50230 |
| LogicProfessionalDevice (Logic Pro 8/16) | 0x01c50230 |
| LogicProfessional8Device | 0x01c50230 |
| LogicProfessional16Device | 0x01c50230 |

All five device classes share the same function pointer. No device has a
per-device override of ComputeTransferSize.

### Decompiled Logic

```cpp
// ELF VA 0x1c50230 -- ComputeTransferSize(this, bandwidth_bits_per_sec)
uint32_t LogicDevice::ComputeTransferSize(uint64_t bandwidth_bps) {
    uint64_t bytes_per_sec = bandwidth_bps / 240;    // divide by 0xf0
    int usb_speed = this->GetUsbSpeed();             // vtable slot 6

    uint32_t block_size = 0x1000;  // 4096 bytes (USB 2.0)
    if (usb_speed == 2) {
        block_size = 0x4000;       // 16384 bytes (USB 3.0)
    }

    // Round up to nearest multiple of block_size
    uint32_t blocks = (bytes_per_sec / block_size) + 1
                    - ((bytes_per_sec % block_size) == 0 ? 1 : 0);
    uint32_t transfer_size = blocks * block_size;

    return AdjustTransferSize(this, transfer_size);
}
```

### The Division by 240

The bandwidth is divided by 240 (0xf0) to get a "target transfer size"
in bytes. This means each transfer carries approximately 1/240th of a
second of data. At 240 transfers per second, with USB micro-frame
scheduling, this keeps the USB pipe saturated while maintaining reasonable
latency.

240 = number of target USB completions per second. The log message
"expected update rate: {}/s" in StartSampling confirms this interpretation.

### Block Size Alignment

| USB Speed | Speed Code | Block Size | Rationale |
|-----------|-----------|------------|-----------|
| Full Speed (12 Mbps) | 0 | 4096 (0x1000) | Not explicitly handled, falls to default |
| High Speed (480 Mbps, USB 2.0) | 1 | 4096 (0x1000) | 4K alignment matches USB 2.0 max packet multiple |
| Super Speed (5 Gbps, USB 3.0) | 2 | 16384 (0x4000) | 16K alignment matches USB 3.0 burst size multiple |

The block size determines the alignment granularity. Transfer sizes are
always an exact multiple of the block size. This prevents partial-packet
USB transfers and aligns with the host controller's DMA requirements.


## AdjustTransferSize

ELF VA: 0x1c4fc50 (Ghidra: 0x1d4fc50)
Source: `/workspace/monorepo/graph-io/devices/logic_interface/src/LogicAnalyzerDevice.cpp`
Lines ~0x129-0x13f (from embedded line numbers in assertion metadata)

### Decompiled Logic

```cpp
uint32_t AdjustTransferSize(LogicDevice* device, uint32_t computed_size) {
    if (device->is_fake_device) {  // offset 0xa0, bool
        return computed_size;  // no adjustment for fake/simulated devices
    }

    // Check SALEAE_USB_TRANSFER_LIMIT environment variable
    uint32_t max_limit = 0x190000;  // 1,638,400 bytes (default cap)
    const char* env = getenv("SALEAE_USB_TRANSFER_LIMIT");
    if (env != nullptr) {
        long env_val = strtoll(env, nullptr, 10);
        if (env_val != 0) {
            log("SALEAE_USB_TRANSFER_LIMIT used to limit transfer size to {}", env_val);
            max_limit = (uint32_t)env_val;
            if (max_limit == 0) return 0;
        }
    }

    // Round down limit to block_size boundary
    int usb_speed = device->GetUsbSpeed();  // vtable slot 6
    uint32_t block_size = (usb_speed == 2) ? 0x4000 : 0x1000;
    uint32_t aligned_limit = max_limit & ~(block_size - 1);
    if (aligned_limit == 0) {
        aligned_limit = block_size;
    }

    // Cap transfer size if it exceeds the limit
    if (aligned_limit < computed_size) {
        log("ideal transfer size {} bytes exceeds limit {}, capping transfer size.");
        computed_size = aligned_limit;
    }

    return computed_size;
}
```

### Default Transfer Size Limit

**0x190000 = 1,638,400 bytes = 1.56 MB**

This is the default maximum transfer buffer size. It can be overridden by
setting the `SALEAE_USB_TRANSFER_LIMIT` environment variable.

After alignment:
- USB 2.0: max = 1,638,400 & ~0xFFF = 1,638,400 (already 4K aligned)
- USB 3.0: max = 1,638,400 & ~0x3FFF = 1,622,016 = 99 * 16384

### Environment Variable Override

The `SALEAE_USB_TRANSFER_LIMIT` environment variable allows users to
override the computed transfer size. This is a debugging/tuning knob.
The value is parsed as a decimal integer via `strtoll()`.


## USB Speed Detection (Vtable Slot 6)

Each device class has its own GetUsbSpeed method that returns:
- 1 = USB 2.0 High Speed (480 Mbps)
- 2 = USB 3.0 Super Speed (5 Gbps)

| Device | ELF VA | Default (Forced) | Notes |
|--------|--------|-----------------|-------|
| LogicGraduateDevice (Logic 8) | 0x1c3d970 | 1 (High Speed) | Returns 1 if this->forced_speed flag is set |
| LogicStudentDevice (Logic 4) | 0x1c56080 | 1 (High Speed) | Same implementation as Graduate |
| LogicProfessionalDevice (Pro 8/16) | 0x1c415e0 | 2 (Super Speed) | Returns 2 if forced_speed flag is set |

When the forced_speed flag at offset 0xa0 is NOT set, all devices call
`FUN_02e9ac00(this->usb_device)` to query the actual negotiated USB speed
via `libusb_get_device_speed`.

This means the default behavior for non-Pro devices is to assume USB 2.0
(block_size=4096), while Pro devices assume USB 3.0 (block_size=16384).
The actual speed is overridden at runtime when the device is connected.


## Bandwidth Calculation

ELF VA: 0x1c4bbb0 (Ghidra: 0x1d4bbb0)
Source: `/workspace/monorepo/graph-io/devices/logic_interface/src/DeviceSettings.cpp`

The bandwidth is computed from the active channel configuration in
DeviceSettings. The function iterates over the channel list and sums:

```
total_bandwidth_bits = 0

for each channel in channel_config:
    if channel is digital:
        if channel.enabled:
            total_bandwidth_bits += digital_sample_rate_hz
    if channel is analog:
        if channel.enabled:
            total_bandwidth_bits += analog_sample_rate_hz * bits_per_sample

return total_bandwidth_bits
```

### Channel Type Encoding

The channel configuration array contains 0x1c-byte (28-byte) entries:
```
offset +0x08: channel_type (int32)  -- 0=disabled?, 1=digital, other=analog
offset +0x0c: sub_type (int32)     -- determines counting logic
offset +0x14: flag (int32)         -- used for conditional counting
offset +0x18: analog_type (int32)  -- determines analog counting
```

Channels with `channel_type == 1` are skipped (disabled/special).
Channels with `sub_type == 1` are counted conditionally based on `flag == 0`.
Channels with `sub_type == 3` are excluded.

### Sample Rate Sources

| Field | Offset in DeviceSettings | Purpose |
|-------|--------------------------|---------|
| digital_sample_rate_hz | 0xa4 (int32) | Digital sample rate |
| analog_sample_rate_hz | 0xb4 (int32) | Analog physical sample rate |
| analog_sample_rate_flag | 0xb0 (bool) | Analog rate available |
| analog_rate_valid | 0xb2 (bool) | Whether to use physical rate |
| digital_rate_valid | 0xb1 (bool) | Whether digital rate is valid |
| bits_per_sample | 0x68 (int32) | Bits per analog sample |

The total bandwidth is in bits per second. For the transfer size formula,
this gets divided by 8 to get bytes/sec, then divided by 240.

### Example Calculations

**Logic Pro 16, 8 digital @ 500 MHz + 8 analog @ 50 MHz, 12-bit:**
```
digital_bw = 8 * 500,000,000 = 4,000,000,000 bits/sec
analog_bw = 8 * 50,000,000 * 12 = 4,800,000,000 bits/sec
total_bw = 8,800,000,000 bits/sec
bytes_per_sec = total_bw / 8 = 1,100,000,000 bytes/sec
ideal_size = 1,100,000,000 / 240 = 4,583,333 bytes
block_size = 16384 (USB 3.0)
transfer_size = ceil(4,583,333 / 16384) * 16384 = 280 * 16384 = 4,587,520
AdjustTransferSize caps at 1,622,016 (0x190000 aligned to 16K)
final transfer_size = 1,622,016 bytes
```

**Logic 8, 8 digital @ 25 MHz, USB 2.0:**
```
digital_bw = 8 * 25,000,000 = 200,000,000 bits/sec
bytes_per_sec = 25,000,000 bytes/sec
ideal_size = 25,000,000 / 240 = 104,166 bytes
block_size = 4096 (USB 2.0)
transfer_size = ceil(104,166 / 4096) * 4096 = 26 * 4096 = 106,496
< 1,638,400 limit, no capping
final transfer_size = 106,496 bytes
```

**Logic 4, 4 digital @ 12 MHz, USB 2.0:**
```
digital_bw = 4 * 12,000,000 = 48,000,000 bits/sec
bytes_per_sec = 6,000,000 bytes/sec
ideal_size = 6,000,000 / 240 = 25,000 bytes
block_size = 4096 (USB 2.0)
transfer_size = ceil(25,000 / 4096) * 4096 = 7 * 4096 = 28,672
final transfer_size = 28,672 bytes
```


## TransferQueue Construction Parameters

The TransferQueue is constructed in `ReadStream_QueueBuffersThread`
(ELF VA: 0x02d93900, Ghidra: 0x02e93900) in `LinuxUsbDevice.cpp`.

### Parameters Passed from StartSampling

Each device's StartSampling (vtable slot 10) packs a BulkReadStream
configuration struct:

| Field | Value | Source |
|-------|-------|--------|
| transfer_size | ComputeTransferSize result | Vtable slot 11, stored at device-specific offset |
| timeout_ms | 1000 | Hardcoded constant |
| max_transfer_bytes | 5,000,000,000 | Hardcoded constant (5 GB) |

The transfer_size is stored at different offsets per device class:
- LogicGraduateDevice (Logic 8): `this + 0x288` (param_1[0x51])
- LogicStudentDevice (Logic 4): `this + 0x1a0` (param_1[0x34])
- LogicProfessionalDevice (Pro 8/16): `this + 0x28c`

### Queue Depth Calculation

Inside `ReadStream_QueueBuffersThread`:

```cpp
uint32_t queue_depth = 128;  // default maximum = 0x80
uint64_t depth_from_bytes = max_transfer_bytes / transfer_size;
if (depth_from_bytes < 128) {
    queue_depth = (uint32_t)depth_from_bytes;
}
```

With max_transfer_bytes = 5,000,000,000 and typical transfer sizes
of 28K-1.6M, the depth_from_bytes is always > 128, so the queue depth
is effectively always **128 outstanding transfers**.

### Transfer Metadata Size

Each transfer in the queue occupies 0x28 (40) bytes of metadata:

```cpp
void* transfers = new uint8_t[queue_depth * 0x28];
memset(transfers, 0, queue_depth * 0x28);
```

The transfer array is zero-initialized. Each 40-byte entry contains:
- Offset 0x00: libusb_transfer pointer (8 bytes)
- Offset 0x08: buffer shared_ptr (16 bytes)
- Offset 0x18: completion flag (4 bytes)
- Offset 0x1C: transfer ID counter (4 bytes)
- Offset 0x20: total_complete counter (8 bytes)

### Timeout Behavior

The per-transfer timeout (from the BulkReadStream config) controls how
long `libusb_handle_events_timeout_completed` waits:

```cpp
uint32_t timeout_ms = config->timeout;  // 1000 ms
struct timeval tv = {
    .tv_sec = timeout_ms / 1000,        // 1 second
    .tv_usec = (timeout_ms % 1000) * 1000  // 0 microseconds
};
libusb_handle_events_timeout_completed(ctx, &tv, &completed_flag);
```

If the timeout fires without completion, the thread logs
"BulkReadThread: Timeout waiting for transfer to complete." and
reports a timeout error.

### Event Loop

The main thread loop in ReadStream:

```
1. Lock mutex
2. Wait for data to be queued (condition variable, 200ms poll)
3. Unlock mutex
4. Call libusb_handle_events_timeout_completed with 1s timeout
5. On completion:
   - Increment total_complete counter
   - Update ring buffer index: next = (current + 1) % queue_depth
   - Decrement in_flight count
   - Notify condition variable
   - Deliver buffer to callback
6. On error: report via ReadStreamError
7. Loop back to 1
```


## Per-Device StartSampling Flow

### LogicGraduateDevice (Logic 8) -- vtable slot 10 @ 0x1c3e5b0

```
1. bandwidth = CalculateBandwidth(settings)
2. transfer_size = ComputeTransferSize(this, bandwidth)  // vtable slot 11
3. this->transfer_size_field = transfer_size
4. if fake_device: join thread, setup per-channel buffers, start fake read
5. else: test connection, configure FPGA, send start command via EP1
6. Pack {transfer_size, timeout=1000, max_bytes=5GB} into BulkReadStream config
7. Call UsbDevice->StartBulkReadStream(endpoint, config)
```

### LogicStudentDevice (Logic 4) -- vtable slot 10 @ 0x1c56810

Same structure as Graduate. Transfer size stored at offset 0x1a0.
Additionally checks `this->is_pattern_mode` flag (offset 0x3c * 8).

### LogicProfessionalDevice (Pro 8/16) -- vtable slot 10 @ 0x1c41eb0

Same core flow as Graduate, with additional logging:
```
1. bandwidth = CalculateBandwidth(settings)
2. transfer_size = ComputeTransferSize(this, bandwidth)
3. this->transfer_size_field = transfer_size
4. log("Transfer size is: {}", transfer_size)
5. log("bandwidth requirement is: {}", bandwidth / 8)  // bytes/sec
6. update_rate = (bandwidth / 8) / transfer_size
7. log("expected update rate: {}/s", round(update_rate))
8. chunk_duration = 1000.0 / update_rate
9. log("expected chunk duration: {} ms", round(chunk_duration))
10. Configure FPGA, send COMMAND_START_READ via EP1
11. Assert !mReadStreamId.has_value()
12. Pack {transfer_size, timeout=1000, max_bytes=5GB}
13. Call UsbDevice->StartBulkReadStream(endpoint, config, callbacks)
```

Source file: `LogicProfessionalDevice.cpp` (from assertion at line 0x1a6).


## SetOverrideUsb2PowerLimit

ELF VA references:
- String "SetOverrideUsb2PowerLimit" at 0x355882
- Mangled symbol `Saleae::Graph::SetOverrideUsb2PowerLimit` in binary

This is a graph kernel request handler registered on `DeviceListNode`.
From the mangled symbols:

```
Saleae::Graph::ConvenienceKernel::RegisterRequestHandler<
    Saleae::Graph::DeviceListNode,
    Saleae::Graph::SetOverrideUsb2PowerLimit
>
```

This allows the Electron frontend to send a command to override the
USB 2.0 power limit. The override likely sets the `forced_speed` flag
at offset 0xa0 in the device object, which causes GetUsbSpeed (slot 6)
to return the forced value (1 or 2) instead of querying the hardware.

This is used when a device is connected to a USB 2.0 port but the user
wants to limit the data rate to avoid overloading the USB hub's power
budget. Setting `usb2_power_limit` forces USB 2.0 mode even if the
hardware negotiated USB 3.0.


## Vtable Slot Summary

For the Logic2FpgaDevice hierarchy (Logic 4, Logic 8, Logic Pro 8/16):

| Slot | Offset | Function | Shared? |
|------|--------|----------|---------|
| 0 | 0x00 | Destructor (D0) | Per-device |
| 1 | 0x08 | Destructor (D1) or GetPublicKey | Per-device |
| 2 | 0x10 | Unknown | Per-device (Grad/Student differ from Pro) |
| 3 | 0x18 | Shared: calls vtable[12] | Yes (0x1c4f910) |
| 4 | 0x20 | GetForcedSpeedFlag -> bool at +0xa0 | Yes (0x1c4e7c0) |
| 5 | 0x28 | GetSomeConfig -> bytes at +0xa8 | Yes (0x1c4fa90) |
| 6 | 0x30 | **GetUsbSpeed** | Per-device (key difference) |
| 7 | 0x38 | Unknown | Per-device |
| 8 | 0x40 | Unknown | Per-device |
| 9 | 0x48 | **GetDeviceSettings / PerformanceOptions** | Per-device |
| 10 | 0x50 | **StartSampling(DeviceSettings&)** | Per-device |
| 11 | 0x58 | **ComputeTransferSize(U64)** | Yes (0x1c50230) |
| 12 | 0x60 | Unknown (called by slot 3) | Per-device |
| 13 | 0x68 | **GetDeviceSettings** | Per-device |
| 14 | 0x70 | Shared for Pro, different for Student | Mixed |
| 15 | 0x78 | Unknown | Per-device |
| 16 | 0x80 | Unknown | Per-device (Grad/Pro only, not Student) |


## Confidence Assessment

| Finding | Confidence | Evidence |
|---------|-----------|----------|
| ComputeTransferSize is shared (slot 11, all devices) | **Confirmed** | All 5 device vtables point to same address 0x1c50230 |
| Formula: bandwidth/240 rounded to block_size | **Confirmed** | Ghidra decompilation of 0x1c50230 |
| Block size: 4096 (USB2) / 16384 (USB3) | **Confirmed** | Constants 0x1000 and 0x4000 in decompiled code |
| Default max limit: 0x190000 (1,638,400) | **Confirmed** | Hardcoded constant in AdjustTransferSize |
| Queue depth default: 128 (0x80) | **Confirmed** | Hardcoded in ReadStream_QueueBuffersThread |
| Timeout: 1000ms | **Confirmed** | CONCAT44(1000, transfer_size) in all three StartSampling |
| max_transfer_bytes: 5,000,000,000 | **Confirmed** | `local_1a8 = 5000000000` in StartSampling |
| GetUsbSpeed returns 1 (Graduate/Student) or 2 (Professional) | **Confirmed** | Decompiled slot 6 functions show hardcoded returns |
| SALEAE_USB_TRANSFER_LIMIT env var override | **Confirmed** | `strtoll` call with env string in AdjustTransferSize |
| Transfer metadata size: 40 bytes per entry | **Confirmed** | `operator_new(queue_depth * 0x28)` |
| SetOverrideUsb2PowerLimit is a graph kernel request | **High** | Mangled symbol with ConvenienceKernel template |
| Bandwidth = sum(channels * rate * bits) | **High** | Decompiled FUN_01d4bbb0, assertion strings reference DeviceSettings.cpp |
| 240 = target transfers per second | **High** | Correlates with "expected update rate: {}/s" log in Professional StartSampling |


## Source File References

```
/workspace/monorepo/graph-io/devices/logic_interface/src/LogicAnalyzerDevice.cpp
  -- ComputeTransferSize, AdjustTransferSize

/workspace/monorepo/graph-io/devices/logic_interface/src/LogicDevice.cpp
  -- ExpandWriteData (originally misidentified as ComputeTransferSize base)

/workspace/monorepo/graph-io/devices/logic_interface/src/LogicGraduateDevice.cpp
  -- StartSampling, GetUsbSpeed

/workspace/monorepo/graph-io/devices/logic_interface/src/LogicStudentDevice.cpp
  -- StartSampling, GetUsbSpeed

/workspace/monorepo/graph-io/devices/logic_interface/src/LogicProfessionalDevice.cpp
  -- StartSampling, GetUsbSpeed, SendStartCommand

/workspace/monorepo/graph-io/devices/logic_interface/src/DeviceSettings.cpp
  -- CalculateBandwidth (FUN_01d4bbb0)

/workspace/monorepo/graph-io/devices/usb_device/src/LinuxUsbDevice.cpp
  -- ReadStream_QueueBuffersThread, TransferQueue constructor
```


## Open Questions

1. **CalculateBandwidth channel type encoding**: The exact channel type
   values (0, 1, 3, other) need correlation with known channel
   configurations from captured USB traffic.

2. **USB speed function FUN_02e9ac00**: This function queries the actual
   USB speed from libusb. The exact mapping from libusb speed values
   (1-5) to the Saleae speed codes (1-2) needs confirmation.

3. **Transfer size offset per device**: Graduate uses offset 0x288,
   Student uses 0x1a0, Professional uses 0x28c. These differ because
   the device objects have different sizes due to different member
   variables.

4. **The /240 constant significance**: While 240 correlates with the
   "expected update rate" log, the exact origin of this value is
   unclear. It might be: 240 Hz = 4 USB microframes * 60 FPS display,
   or it could be an empirically tuned value.

5. **Why 128 max queue depth**: The maximum of 128 outstanding transfers
   is likely a balance between memory usage (128 * transfer_size can be
   up to 200 MB) and throughput (enough to mask USB latency).
