# USB 2.0 Fallback Handling and Speed Negotiation

Analysis date: 2026-03-10
Binary: libgraph_server_shared.so (48MB, stripped, ELF 64-bit x86-64)
Source: Saleae Logic 2 v2.4.40
Task: TASK-0071
Method: Ghidra headless decompilation, objdump disassembly, string cross-reference, RIP-relative LEA scanning
Depends on: TASK-0070 (transfer queue sizing), TASK-0090 (sample rate options)


## Summary

The Saleae driver adapts to USB 2.0 vs 3.0 at multiple levels: transfer block
alignment, FPGA bitstream chunk size, sample rate availability, channel
capability restrictions, GPIF state machine selection, and bandwidth allocation.
The USB speed is queried once during device initialization and stored as a
3-value enum. All speed-dependent decisions branch on this stored value.

The key theme: USB 2.0 devices get smaller USB transfer blocks, slower maximum
sample rates, reduced analog channel availability (Pro 16 loses analog on
channels 8-15), and smaller FPGA programming chunks. But the same firmware
and FPGA bitstream are used regardless of USB speed.


## USB Speed Enum

Stored at `UsbDevice+0x20` (4 bytes, uint32). Accessed by `FUN_02d9ac00`
(a trivial 4-byte load + return).

| Value | Speed            | Max Bitrate | Block Size | Notes |
|-------|------------------|-------------|------------|-------|
| 0     | USB 1.x/Full     | Unknown     | 4096       | Falls to USB 2.0 defaults |
| 1     | USB 2.0 High     | See table   | 4096       | 480 Mbps theoretical |
| 2     | USB 3.0 Super    | See table   | 16384      | 5 Gbps theoretical |

The speed field is set during device initialization (exact setter location
not pinpointed -- UsbDevice+0x20 is populated by the LinuxUsbDevice
constructor or connection handler, likely via libusb_get_device_speed mapped
to this enum). The assertion `"Invalid USB speed"` fires if the value is >= 3,
confirming only values 0-2 are valid.


## GetUsbSpeed Per-Device Implementations (Vtable Slot 6)

All three implementations are structurally identical, differing only in
the forced-speed default value. Each is 27 bytes.

### LogicGraduateDevice (Logic 8) -- ELF VA 0x1c3d970

```c
int GetUsbSpeed(Device *this) {
    if (*(char *)(this + 0xa0) != 0) {  // forced_speed flag
        return 1;  // USB 2.0 High Speed
    }
    return UsbDevice_GetSpeed(this->usb_device);  // offset 0x90
}
```

### LogicStudentDevice (Logic 4) -- ELF VA 0x1c56080

```c
int GetUsbSpeed(Device *this) {
    if (*(char *)(this + 0xa0) != 0) {
        return 1;  // USB 2.0 High Speed (same as Logic 8)
    }
    return UsbDevice_GetSpeed(this->usb_device);
}
```

### LogicProfessionalDevice (Pro 8/16) -- ELF VA 0x1c415e0

```c
int GetUsbSpeed(Device *this) {
    if (*(char *)(this + 0xa0) != 0) {
        return 2;  // USB 3.0 Super Speed
    }
    return UsbDevice_GetSpeed(this->usb_device);
}
```

**Key difference**: Non-Pro devices default to USB 2.0 (speed=1) when
the forced flag is set, while Pro devices default to USB 3.0 (speed=2).
When not forced, all devices query the actual negotiated speed from the
UsbDevice object.

The forced_speed flag at device offset 0xa0 is a boolean. When set, the
device returns its "expected" speed without querying hardware. This is used
for fake/simulated devices and possibly for the USB 2.0 power limit override.


## UsbDevice Speed Accessor -- ELF VA 0x2d9ac00

```asm
mov 0x20(%rdi), %eax    ; return this->speed (4 bytes at offset 0x20)
ret
```

Trivial accessor returning the stored speed enum value. This is the function
called by all GetUsbSpeed implementations when the forced flag is not set.


## Device Type Accessor -- ELF VA 0x2d9ae80

```asm
mov 0x28(%rdi), %eax    ; return this->device_type (4 bytes at offset 0x28)
ret
```

Returns the device type enum (0=Logic, 1=Logic16, 4=Logic4, 5=Logic8,
6=Pro8, 7=Pro16). Used in WriteConfigurationData and DownloadBitstream
to select Pro vs non-Pro code paths.


## Transfer Block Alignment (from TASK-0070, confirmed)

ComputeTransferSize (ELF VA 0x1c50230) divides bandwidth by 240 and rounds
up to the nearest block boundary:

| USB Speed | Block Size | Alignment |
|-----------|-----------|-----------|
| USB 2.0   | 0x1000 (4096 bytes)  | 4 KB |
| USB 3.0   | 0x4000 (16384 bytes) | 16 KB |

AdjustTransferSize (ELF VA 0x1c4fc50) caps the result:
- Default max: 0x190000 = 1,638,400 bytes (~1.56 MB)
- USB 2.0 aligned: 1,638,400 (already 4K-aligned)
- USB 3.0 aligned: 1,622,016 (99 x 16384)
- Override: `SALEAE_USB_TRANSFER_LIMIT` environment variable


## FPGA Bitstream Programming Chunk Sizes

### DownloadBitstream (ELF VA 0x1c53f20, slot [6] of FpgaDeviceFeatures)

The chunk size depends on device type AND USB speed:

**Non-Pro devices** (type != 6 and != 7):
- Chunk size: always 62 bytes (0x3E)
- Independent of USB speed

**Pro devices** (type 6 or 7):
- USB 2.0: chunk size = 508 bytes (0x1FC)
- USB 3.0: chunk size = 1020 bytes (0x3FC)
- Formula: `(speed == 2) << 9 | 0x1FC`

### WriteConfigurationData (ELF VA 0x1c544b0)

**Non-Pro path** (fire-and-forget):
```
EP1 OUT: [0x7F] [len_8bit] [data...]     (2 + data bytes)
```
- Max data: 62 bytes (assert: data_size <= 0x3E)
- Total packet: 64 bytes max = USB 2.0 EP1 max packet size
- No response read

**Pro path** (with response verification):
```
EP1 OUT: [0x7F] [len_lo] [len_hi] [data...]   (3 + data bytes)
EP1 IN:  [0x00]                                 (1 byte, success)
```
- Max data USB 2.0: 509 bytes (assert: data_size <= 0x1FD)
- Max data USB 3.0: 1021 bytes (assert: data_size <= 0x3FD)
- Total packet USB 2.0: 512 bytes max
- Total packet USB 3.0: 1024 bytes max
- Response read: expects 0x00, throws "Failed to write configuration data" otherwise

**Correction to previous documentation**: Earlier notes stated chunk sizes
of 62 vs 509. The actual breakdown is more nuanced:
- Non-Pro: always 62 bytes data (regardless of USB speed)
- Pro USB 2.0: 508 bytes data per chunk
- Pro USB 3.0: 1020 bytes data per chunk

The assertion message "does not support lengths larger than 62/509" is
misleading -- the 509 limit applies only to Pro devices at USB 2.0. At
USB 3.0, Pro devices can send up to 1021 bytes per chunk.

Source: `/workspace/monorepo/graph-io/devices/logic_interface/src/LogicAnalyzerDevice.cpp`
Assertion: `"void FpgaDeviceFeatures::WriteConfigurationData(const U8 *, U32)"` at line 0x4BF


## Sample Rate Impact

### Bandwidth-Gated Rate Selection

`GetInterfaceMaximumBitRate()` (ELF VA 0x1c4a0c0, inlined into
`GetDigitalOnlyPerformanceOptions` at ELF VA 0x1c49ec0) uses a lookup table
at ELF VA 0x5748f0, indexed by the USB speed enum (0-2):

```c
uint64_t max_bitrate = TABLE_5748f0[device_settings->usbSpeed];  // 3 entries
```

The table values appear XOR-obfuscated in .rodata (raw bytes:
`52aae1ccd6cc55aae8cc45aaffff4caad9cce2cc54aaffff`) and could not be
extracted statically. The algorithm gates sample rates by:

```
rate * num_active_channels <= max_bitrate
```

Any rate exceeding the bandwidth limit is excluded from the available options.

### Analog Rate Arrays (Pro Devices)

Two analog rate arrays exist in .rodata, differing by one entry:

**USB 3.0 array** (ELF VA 0x471a64, 11 entries):
```
100 MHz, 50 MHz, 40 MHz, 25 MHz, 20 MHz, 10 MHz, 8 MHz, 5 MHz, 4 MHz, 2 MHz, 1 MHz
```

**USB 2.0 array** (ELF VA 0x472260, 10 entries):
```
50 MHz, 40 MHz, 25 MHz, 20 MHz, 10 MHz, 8 MHz, 5 MHz, 4 MHz, 2 MHz, 1 MHz
```

The 100 MHz analog entry is only available on USB 3.0. At USB 2.0,
100 MHz x 12 bits x N channels would exceed the available bandwidth.

### Digital Rate Array (Same for Both USB Speeds)

```
500 MHz, 250 MHz, 125 MHz, 100 MHz, 50 MHz, 25 MHz, 20 MHz,
12.5 MHz, 10 MHz, 6.25 MHz, 5 MHz, 4 MHz, 2.5 MHz, 2 MHz, 1 MHz
```

15 entries. Higher rates are automatically filtered out by the bandwidth check,
but the rate array itself is shared between USB 2.0 and 3.0.


## Channel Capability Restrictions (Logic Pro 16)

### GetDeviceSettings (ELF VA 0x1c418c0)

For Logic Pro 16 (16 channels), the channel configuration depends on USB
speed and a `limitedBandwidthFlag` at device offset 0x51:

**USB 3.0** OR **limitedBandwidthFlag set**:
- All 16 channels: type 1 (toggleable, both digital and analog)

**USB 2.0 AND limitedBandwidthFlag NOT set**:
- Channels 0-7: type 1 (toggleable, both digital and analog)
- Channels 8-15: type 3 (digital-only)

The expression `(7 < channel_index) * 2 + 1` computes the channel type:
- channels 0-7: `0 * 2 + 1 = 1` (full capability)
- channels 8-15: `1 * 2 + 1 = 3` (digital-only)

This means on USB 2.0, Logic Pro 16 loses analog capability on the upper
8 channels. Logic Pro 8 is not affected since it only has 8 channels total.


## Bandwidth Allocation (Analog Advanced Tier)

For Pro devices, `GetAnalogAdvancedPerformanceOptions` (ELF VA 0x1c4a250)
creates 5 bandwidth allocation groups. The budget for each group is derived
from the USB max bitrate:

```
tier[0] = maxBitRate          (100%)
tier[1] = maxBitRate * 8 / 10 (80%)
tier[2] = maxBitRate * 6 / 10 (60%)
tier[3] = maxBitRate * 4 / 10 (40%)
tier[4] = maxBitRate / 5      (20%)
```

At USB 2.0, the maxBitRate is significantly lower, so each tier has a
proportionally smaller budget. This means:
- Fewer simultaneous analog + digital channels at high rates
- Maximum achievable rates are lower
- More groups may be empty (no valid rate combination fits the budget)


## GPIF State Machine Selection

An initializer function (_INIT_55 at ELF VA 0x1dd9630) builds three lookup
tables mapping {device_type, bus_width, usb_speed} to GPIF state machine
indices. The GPIF (General Programmable Interface) on the FX3 handles
the physical data transfer between FPGA and USB bus.

### Main GPIF Configuration Table (27 entries)

| Device    | Bus Width | USB Speed | GPIF State |
|-----------|-----------|-----------|------------|
| Logic     | 4-bit     | USB 2.0   | 0          |
| Logic     | 4-bit     | USB 3.0   | 2          |
| Logic     | 8-bit     | USB 3.0   | 2          |
| Logic     | 14-bit    | Either    | 3          |
| Logic16   | 4-bit     | USB 2.0   | 0          |
| Logic16   | 4-bit     | USB 3.0   | 1          |
| Logic16   | 8-bit     | USB 3.0   | 2          |
| Logic16   | 14-bit    | Either    | 3          |
| Logic4    | 4-bit     | Either    | 2          |
| Logic4    | 14-bit    | Either    | 3          |
| Logic8    | 4-bit     | USB 2.0   | 0          |
| Logic8    | 4-bit     | USB 3.0   | 2          |
| Logic8    | 8-bit     | USB 3.0   | 2          |
| Logic8    | 14-bit    | Either    | 3          |
| Pro 8/16  | 4-bit     | Either    | 2          |
| Pro 8/16  | 14-bit    | Either    | 3          |

Key observations:
- **GPIF state 0**: Used for USB 2.0 + narrow bus (4-bit) on legacy/basic devices
- **GPIF state 1**: Used only for Logic16 at USB 3.0 + 4-bit bus
- **GPIF state 2**: Standard mode for USB 3.0 or wider bus configurations
- **GPIF state 3**: Used for 14-bit wide bus (analog data path), speed-independent
- **Pro devices always use state 2 or 3**: Never fall back to state 0

The GPIF state determines how data is packed onto the USB bus. State 0 is
the most bandwidth-limited mode, suitable for USB 2.0 high-speed with narrow
data path.


## SetOverrideUsb2PowerLimit

Registered as a graph kernel request handler on `DeviceListNode`:

```
Saleae::Graph::ConvenienceKernel::RegisterRequestHandler<
    Saleae::Graph::DeviceListNode,
    Saleae::Graph::SetOverrideUsb2PowerLimit
>
```

JSON field: `/overrideUsb2PowerLimit`

This is a command from the Electron frontend to override the USB 2.0
power limit. The actual handler function could not be traced due to
indirect dispatch through the ConvenienceKernel template system. The
handler likely modifies the `limitedBandwidthFlag` at device offset 0x51
or the `forced_speed` flag at offset 0xa0.

The purpose: USB 2.0 ports supply max 500 mA vs USB 3.0's 900 mA. If a
high-power device is connected to a USB 2.0 port, the driver may need to
further limit data rates to stay within the power budget. This override
allows the user to acknowledge the power limitation.

From the JS bundle, the `/hasLimitedBandwidth` field is sent to the
frontend to indicate whether the device is bandwidth-limited (USB 2.0).
The `/overrideUsb2PowerLimit` allows the user to change this behavior.


## USB 2.0 Default Sample Rate Presets (from JS Bundle)

When a device connects at USB 2.0 speed, the JS frontend applies conservative
default presets:

| Device     | Default Digital | Default Analog | Notes |
|------------|----------------|----------------|-------|
| Logic 4    | 12 MHz         | 6 MHz          | Same as USB 3.0 (bandwidth sufficient) |
| Logic 8    | 100 MHz        | 2.5 kHz        | Severely reduced analog |
| Logic Pro 8| 100 MHz        | 3.125 kHz      | Very low analog default |
| Logic Pro 16| 100 MHz       | 3.125 kHz      | Very low analog default |

At USB 3.0:
| Device     | Default Digital | Default Analog |
|------------|----------------|----------------|
| Logic Pro 8| 500 MHz        | 50 MHz         |
| Logic Pro 16| 500 MHz       | 50 MHz         |


## Complete USB 2.0 vs 3.0 Difference Summary

| Aspect | USB 2.0 | USB 3.0 |
|--------|---------|---------|
| Transfer block alignment | 4096 bytes | 16384 bytes |
| Max transfer size | 1,638,400 bytes | 1,622,016 bytes |
| FPGA chunk (non-Pro) | 62 bytes | 62 bytes (same) |
| FPGA chunk (Pro) | 508 bytes | 1020 bytes |
| FPGA header (Pro) | 3 bytes (opcode + 16-bit len) | 3 bytes (same format) |
| FPGA response (Pro) | Yes (read 0x00) | Yes (same) |
| Max analog rate (Pro) | 50 MHz | 100 MHz |
| Analog rate entries (Pro) | 10 | 11 |
| Pro 16 channels 8-15 | Digital-only | Full analog + digital |
| GPIF state (4-bit, basic) | 0 | 2 |
| GPIF state (4-bit, Pro) | 2 | 2 (same) |
| Default speed (non-Pro) | Assumed (forced=1) | Query hardware |
| Default speed (Pro) | Query hardware | Assumed (forced=2) |
| Power budget | 500 mA | 900 mA |
| Queue depth | 128 (same) | 128 (same) |
| Timeout | 1000 ms (same) | 1000 ms (same) |


## Rust Driver Implications

1. **Speed detection**: Call `libusb_get_device_speed()` during device
   initialization. Map the result to the Saleae enum (0, 1, or 2).
   Store once, use everywhere.

2. **Block alignment**: Use 4096 for speed <= 1, 16384 for speed == 2
   in ComputeTransferSize.

3. **FPGA programming**: Use separate chunk sizes:
   - Non-Pro: always 62 bytes data, 2-byte header `[0x7F][len]`
   - Pro: 508 bytes (USB 2.0) or 1020 bytes (USB 3.0), 3-byte header
     `[0x7F][len_lo][len_hi]`, with response read after each chunk

4. **Sample rate filtering**: Apply bandwidth check
   `rate * channels <= max_bitrate` before presenting rate options.
   Exclude 100 MHz analog rate on USB 2.0.

5. **Channel capabilities**: For Logic Pro 16 on USB 2.0, mark channels
   8-15 as digital-only unless the limitedBandwidthFlag overrides this.

6. **GPIF state machine**: Select the correct GPIF configuration based
   on {device_type, bus_width, speed}. This is sent to the FX3 firmware
   during initialization.

7. **Power management**: Implement SetOverrideUsb2PowerLimit to allow
   users to acknowledge USB 2.0 power constraints.


## Confidence Assessment

| Finding | Confidence | Evidence |
|---------|-----------|----------|
| GetUsbSpeed returns 1 (Logic4/8) or 2 (Pro) when forced | **Confirmed** | Ghidra decompilation of 3 functions, all 27 bytes |
| UsbDevice speed at offset 0x20 | **Confirmed** | Disassembly of accessor at 0x2d9ac00 |
| Block size 4096/16384 | **Confirmed** | Constants in ComputeTransferSize (0x1d50230) |
| FPGA chunk 62/508/1020 | **Confirmed** | Disassembly of DownloadBitstream (0x1c53f20) |
| WriteConfigurationData packet format | **Confirmed** | Disassembly of 0x1c544b0 with header construction |
| Pro response verification on config write | **Confirmed** | EP1 IN read with 0x00 check in Pro path |
| 100 MHz analog USB 3.0 only | **High** | Two .rodata arrays differ by 100 MHz entry |
| Pro 16 loses analog on channels 8-15 at USB 2.0 | **Confirmed** | Decompilation of GetDeviceSettings (0x1d418c0) |
| GPIF state table with 27 entries | **Confirmed** | Decompilation of _INIT_55 (0x1dd9630) |
| Bandwidth table at 0x5748f0 is XOR-obfuscated | **High** | Raw bytes clearly non-sensical as uint64 values |
| SetOverrideUsb2PowerLimit is graph kernel request | **High** | Mangled symbol with ConvenienceKernel template |
| limitedBandwidthFlag at device offset 0x51 | **High** | Referenced in GetDeviceSettings decompilation |


## Key Virtual Addresses

| ELF VA | Ghidra VA | Function |
|--------|-----------|----------|
| 0x1c3d970 | 0x1d3d970 | LogicGraduateDevice::GetUsbSpeed |
| 0x1c56080 | 0x1d56080 | LogicStudentDevice::GetUsbSpeed |
| 0x1c415e0 | 0x1d415e0 | LogicProfessionalDevice::GetUsbSpeed |
| 0x2d9ac00 | 0x2e9ac00 | UsbDevice speed accessor (offset 0x20) |
| 0x2d9ae80 | 0x2e9ae80 | UsbDevice device type accessor (offset 0x28) |
| 0x1c50230 | 0x1d50230 | ComputeTransferSize (shared, vtable slot 11) |
| 0x1c4fc50 | 0x1d4fc50 | AdjustTransferSize |
| 0x1c49ec0 | 0x1d49ec0 | GetDigitalOnlyPerformanceOptions (contains GetInterfaceMaximumBitRate) |
| 0x1c4a250 | 0x1d4a250 | GetAnalogAdvancedPerformanceOptions |
| 0x1c53f20 | 0x1d53f20 | FpgaDeviceFeatures::DownloadBitstream |
| 0x1c544b0 | 0x1d544b0 | FpgaDeviceFeatures::WriteConfigurationData |
| 0x1c418c0 | 0x1d418c0 | LogicProfessionalDevice::GetDeviceSettings |
| 0x5748f0  | 0x6748f0  | Max bit rate table (3 x uint64, obfuscated) |
| 0x1dd9630 | 0x1ed9630 | _INIT_55 (GPIF state machine table initializer) |


## Source File References

```
/workspace/monorepo/graph-io/devices/logic_interface/src/LogicAnalyzerDevice.cpp
  -- ComputeTransferSize, AdjustTransferSize, WriteConfigurationData, DownloadBitstream

/workspace/monorepo/graph-io/devices/logic_interface/src/LogicGraduateDevice.cpp
  -- GetUsbSpeed (Logic 8)

/workspace/monorepo/graph-io/devices/logic_interface/src/LogicStudentDevice.cpp
  -- GetUsbSpeed (Logic 4)

/workspace/monorepo/graph-io/devices/logic_interface/src/LogicProfessionalDevice.cpp
  -- GetUsbSpeed (Pro), GetDeviceSettings

/workspace/monorepo/graph-io/devices/logic_interface/src/DeviceSettings.cpp
  -- GetInterfaceMaximumBitRate, GetDigitalOnlyPerformanceOptions,
     GetAnalogAdvancedPerformanceOptions, CalculateBandwidth

/workspace/monorepo/graph-io/devices/usb_device/src/UsbDevice.cpp
  -- Speed accessor, device type accessor
```


## Open Questions

1. **Exact max bitrate values**: The table at 0x5748f0 is XOR-obfuscated
   and could not be read statically. Runtime instrumentation or pcap
   analysis comparing known rates to the bandwidth check would reveal
   these values. Expected: USB 2.0 ~300-400 Mbps effective, USB 3.0
   ~3000-4000 Mbps effective.

2. **libusb speed mapping**: The exact mapping from `libusb_get_device_speed`
   return values (LIBUSB_SPEED_LOW=1, FULL=2, HIGH=3, SUPER=4,
   SUPER_PLUS=5) to the Saleae enum (0, 1, 2) is not confirmed.
   Expected: HIGH(3)->1, SUPER(4)->2, others->0.

3. **SetOverrideUsb2PowerLimit behavior**: The actual handler could not be
   traced through the ConvenienceKernel template dispatch. Does it modify
   the forced_speed flag, the limitedBandwidthFlag, or both? Does it
   trigger a device settings refresh?

4. **Non-Pro FPGA chunk independence from USB speed**: Non-Pro devices
   always use 62-byte chunks even at USB 3.0. Is this because non-Pro
   devices never negotiate USB 3.0, or is 62 bytes intentionally kept
   for compatibility? Logic 4/8 GetUsbSpeed defaults to USB 2.0 when
   forced, suggesting these devices typically run at USB 2.0.

5. **GPIF state 0 vs 2**: What is the actual difference between GPIF
   states 0 and 2? State 0 is only used for USB 2.0 + 4-bit bus on
   basic devices (Logic, Logic16, Logic8). This likely configures
   different burst sizes or DMA patterns in the FX3.

6. **Bus width selection**: The GPIF table includes 4-bit, 8-bit, and
   14-bit bus widths. How is the bus width determined? It may depend on
   the channel configuration (digital-only vs analog) and the number
   of active channels.


## References

- `re/notes/transfer-queue-sizing.md` -- ComputeTransferSize, AdjustTransferSize
- `re/notes/sample-rate-options.md` -- SampleRateOption struct, performance tiers
- `re/notes/device-settings.md` -- DeviceSettings layout, JS bundle constants
- `re/notes/fpga-features-vtable.md` -- FpgaDeviceFeatures vtable analysis
- `re/notes/device-class-hierarchy.md` -- Device class hierarchy
- `re/notes/ep1-command-protocol.md` -- EP1 command opcodes
- `re/notes/fpga-bitstream-protocol.md` -- FPGA programming protocol
- `re/notes/gpif-bus-width.md` -- GPIF bus width analysis
