# Digital Data Compression and USB 2.0 Bandwidth Handling

Analysis date: 2026-03-11
Binary: libgraph_server_shared.so (48MB, stripped, ELF 64-bit x86-64)
Source: Saleae Logic 2 v2.4.40
Task: TASK-0114


## Summary

**There is no on-device data compression.** The FPGA streams raw,
uncompressed sample data over USB. Bandwidth constraints are handled
entirely through sample rate limiting -- the software restricts the
available sample rates based on the USB speed and number of enabled
channels so that the raw data rate never exceeds the USB link capacity.

The PRD's mention of "on-device data compression (USB 2.0 bandwidth
constraint)" was a hypothesis based on the assumption that compression
would be needed. This investigation found no evidence of any compression
in the FPGA-to-host data path. Instead, the architecture uses three
complementary mechanisms:

1. **Rate limiting at configuration time** -- the device rejects sample
   rate / channel count combinations that would exceed USB bandwidth
2. **Analog downsampling** -- a host-side `AnalogDownsamplerNode` reduces
   analog data rates after capture when needed for display
3. **12-bit ADC packing** -- analog samples are packed at 12-bit density
   (not padded to 16-bit), which is a fixed encoding, not compression


## Evidence Against Device-Side Compression

### String search: no compression primitives

Searching `libgraph_server_shared.so` for compression-related strings
(`compress`, `decompress`, `rle`, `run length`, `packed`, `delta`,
`encode`, `decode`) found only:

- **BZ2_bzCompress/Decompress** -- bzip2 library (statically linked, used
  by BerkeleyDB, not USB data)
- **lzma_block_compressed_size** -- LZMA library (used by embedded Python
  or data export)
- **SSL_COMP/zlib** -- SSL/zlib compression (network, not USB data)
- **rocksdb.compression** -- RocksDB internal compression
- **rlecode_hqx/rledecode_hqx** -- Python BinHex RLE module
- **Saleae::Digital::Detail::AppendRle** -- host-side RLE storage for
  digital samples (post-capture, not USB wire format)
- **Orphaned RLE code at start** -- error message in digital store
  deserialization (disk format, not USB)

None of these are in the USB data receive path.

### Decompilation: no decompression in data pipeline

The USB data pipeline is:

```
EP2 IN -> TransferQueue -> OnDataBuffer -> ContractReadData
    -> DisaggregatorNode -> per-channel data
```

Decompilation of every function in this chain (documented in
disaggregator-interleaving.md and data-interleaving.md) shows:

- `ContractReadData` removes bus-width padding bytes (1-in-N selection),
  not decompression
- `DisaggregatorNode` splits interleaved data into per-channel streams
  using fixed byte offsets, not decompression
- The Pro device deinterleaver unpacks 12-bit ADC values using shift/mask
  operations, not decompression
- Digital data is 1 bit per sample, 8 samples per byte, uncompressed

There is no variable-length decoding, no dictionary lookup, no run-length
decoding, and no entropy decoding anywhere in the USB data receive path.

### Host-side RLE is storage, not transport

The `AppendRle` function in `Saleae::Digital::Detail` encodes digital
data for host-side storage. This runs AFTER disaggregation and operates
on already-decoded per-channel digital data. It converts a stream of
bit values into run-length encoded transitions for efficient storage
and display. This is a post-processing optimization, not a USB
transport compression.


## How USB 2.0 Bandwidth is Actually Handled

### Mechanism 1: Sample Rate Limiting

The primary mechanism is restricting available sample rates based on USB
speed. The `GetInterfaceMaximumBitRate()` function (Ghidra VA 0x1d4bac0,
source: DeviceSettings.cpp line 0x36f) returns a lookup-table value
indexed by USB speed:

```
USB Speed Enum    Speed Name        Max Bit Rate (bps)    Effective MB/s
---------------------------------------------------------------------------
0                 Full Speed         6,000,000              0.75
1                 High Speed       300,000,000             37.5
2                 Super Speed    3,000,000,000            375.0
```

Lookup table at Ghidra VA 0x5748f0 (ELF VA 0x4748f0):
```
Offset  LE u64 value    Decimal
0x00    0x005B8D80      6,000,000      (USB 1.1 Full Speed)
0x08    0x11E1A300      300,000,000    (USB 2.0 High Speed)
0x10    0xB2D05E00      3,000,000,000  (USB 3.0 Super Speed)
```

These values represent the usable data throughput after USB protocol
overhead. USB 2.0 Hi-Speed has a raw signaling rate of 480 Mbps, but
due to protocol overhead (SOF packets, NAK/retry, inter-packet gaps),
the effective bulk transfer throughput is approximately 300 Mbps
(37.5 MB/s).

### Sample Rate Validation

The `GetDigitalOnlyPerformanceOptions` and `GetAnalogAdvancedPerformanceOptions`
functions (source: DeviceSettings.cpp) iterate over candidate sample rates
and reject any where:

```
sample_rate_hz * num_channels * bytes_per_sample > max_bit_rate
```

For digital-only captures:
```
sample_rate * num_digital_channels <= max_bit_rate
```

For mixed analog+digital captures, the check also considers the analog
sample rate, analog channel count, and ADC bytes per sample. The function
`DeviceCalculateMixRate` (ELF VA 0x1c19fa0, source: DeviceHelpers.cpp)
computes the digital-to-analog interleave ratio using GCD, and the total
bandwidth is the sum of digital and analog data rates.

### Bandwidth Tiers

The `GetAnalogAdvancedPerformanceOptions` function builds a 5-level
bandwidth tier table from the max bit rate:

```
Tier    Fraction    For USB 2.0 (300 Mbps)    For USB 3.0 (3 Gbps)
---------------------------------------------------------------------
0       100%        300 Mbps (37.5 MB/s)      3000 Mbps (375 MB/s)
1        80%        240 Mbps (30 MB/s)        2400 Mbps (300 MB/s)
2        60%        180 Mbps (22.5 MB/s)      1800 Mbps (225 MB/s)
3        40%        120 Mbps (15 MB/s)        1200 Mbps (150 MB/s)
4        20%         60 Mbps (7.5 MB/s)        600 Mbps (75 MB/s)
```

Each tier represents a progressively more conservative bandwidth budget.
The UI presents sample rate options within these tiers, allowing the user
to trade channel count for sample rate within each budget level.

### Mechanism 2: USB Transfer Size Adaptation

The `ComputeTransferSize` function (Ghidra VA 0x1d3c0a0, source:
LogicDevice.cpp line 0x161) dynamically sizes USB bulk transfer buffers
based on the data rate:

```c
// Pseudocode from decompilation
uint ComputeTransferSize(uint64_t total_data_rate) {
    if (total_data_rate / 8 < 200000) {
        return 0x1000;  // 4 KB minimum for low rates
    }
    double rate_mbps = (total_data_rate / 8) / 1000000.0;
    int multiplier = (int)(rate_mbps * 5.21);
    if (multiplier < 1 || multiplier > 125) {
        error("Invalid multiplier");
    }
    return multiplier * 0x1000;  // 4 KB to 512 KB
}
```

The `AdjustTransferSize` function (Ghidra VA 0x1d4fc50, source:
LogicAnalyzerDevice.cpp line 0x129) then:

1. Checks for `SALEAE_USB_TRANSFER_LIMIT` environment variable override
2. Queries USB speed via virtual call (speed 0/1/2)
3. Aligns to USB speed-dependent block boundary:
   - USB 2.0 High Speed: align to 0x1000 (4 KB)
   - USB 3.0 Super Speed: align to 0x4000 (16 KB)
4. Caps transfer size to 0x190000 (1,638,400 bytes) default maximum
5. Logs "ideal transfer size N bytes exceeds limit M, capping transfer
   size" when the computed size exceeds the cap

### Mechanism 3: hasLimitedBandwidth Flag

The `/hasLimitedBandwidth` field is a boolean property on each device
descriptor (stored at struct offset +0x59, serialized as a JSON boolean).
It is read from the device capabilities JSON alongside `/channelCapabilities`,
`/sampleRateOptions`, `/digitalThresholdOptions`, `/ledType`, and
`/isPhysicalDevice`.

This flag indicates that the device is operating over a bandwidth-limited
connection (USB 2.0 instead of USB 3.0). When set, the UI restricts the
available sample rate options to those that fit within the USB 2.0
bandwidth budget. This is a configuration-time flag, not a runtime
compression toggle.

### Mechanism 4: LowSpeedDivider / ContractReadData

The `_LowSpeedDivider` field at offset `this+0x19c` controls bus-width
contraction in `ContractReadData` (Ghidra VA 0x1d3bad0, source:
LogicDevice.cpp line 0x16e):

```
Divider    Action
1          No-op (data width matches bus width)
2          Keep every 2nd byte (discard 1 padding byte per data byte)
4          Keep every 4th byte (discard 3 padding bytes per data byte)
8          Keep every 8th byte (discard 7 padding bytes per data byte)
other      ERROR: "_LowSpeedDivider is not a valid size"
```

This handles the case where at low sample rates, the FPGA outputs data
on a wider bus than needed. For example, if only 1 digital channel is
active on a 32-bit GPIF bus, 3 out of every 4 bytes are padding.
ContractReadData strips this padding in-place before the data enters
the disaggregator. This is bus-width adaptation, not compression.

### Mechanism 5: Analog Downsampling (Host-Side)

The `AnalogDownsamplerNode` (source: analog_downsampler_node.cpp) performs
host-side downsampling of analog data. It is configured via
`SetAnalogDownsampleRate` and operates in the graph pipeline after the
capture engine.

The `DownsampleMode` enum has at least two values:
- Normal downsampling (averaging or decimation)
- MinMax mode (preserving min and max values in each window)

The assertion `new_sampling_config.mDownsampleMode != DownsampleMode::MinMax`
in ProcessPendingConfiguration indicates that MinMax mode is not always
available.

This downsampling is a display/rendering optimization, not a USB transport
mechanism. The full-rate data is captured over USB uncompressed; downsampling
happens afterward for efficient display of long captures.


## FPGA Constants Extracted

From `SetAnalogChannelConfiguration` / `ValidateConfig` at Ghidra VA
0x2e64640 (source: mso_fpga.cpp line 0x256):

### Altera FPGA

| Constant | Value | Hex |
|----------|-------|-----|
| MinSamplesPerPacket_Altera | 64 | 0x40 |
| MaxSamplesPerPacket_Altera | 6016 | 0x1780 |
| MaxTriggerBufferSamples_Altera | 805,290,944 | 0x2FFFC000 |
| MinTriggerBufferIndex_Altera | -805,290,944 | -0x2FFFC000 |

### Xilinx FPGA

| Constant | Value | Hex |
|----------|-------|-----|
| MinSamplesPerPacket_Xilinx | 256 | 0x100 |
| MaxSamplesPerPacket_Xilinx | 7680 | 0x1E00 |
| MaxTriggerBufferSamples_Xilinx | 1,073,725,440 | 0x3FFFC000 |
| MinTriggerBufferIndex_Xilinx | -1,073,725,440 | -0x3FFFC000 |

### Common

| Constant | Value |
|----------|-------|
| SamplesPerPacketMultiple_WithStats | 256 |
| MinTriggerBufferSamples | 8 |
| MaxTriggerBufferIndex | 0xBFFF (49151) |

FPGA type is determined by a byte flag at the device settings struct: 0
for Altera path, nonzero for Xilinx path (function `FUN_02e62780` checks
`(uVar6 & 0xfe) == 0`).


## USB Speed Detection

The `IsFirmwareDownloaded` function (Ghidra VA 0x2e9b410) detects USB
speed by examining endpoint descriptors:

```
For each endpoint in the device:
    if direction == OUT and maxPacketSize > 0x200 (512):
        speed = 2 (Super Speed / USB 3.0)
    elif direction == OUT and maxPacketSize > 0x40 (64):
        speed = 1 (High Speed / USB 2.0)
    else:
        speed = 0 (Full Speed / USB 1.1)
```

The speed enum maps to USB standard max packet sizes:
- Full Speed: max 64 bytes per bulk packet
- High Speed: max 512 bytes per bulk packet
- Super Speed: max 1024 bytes per bulk packet


## 12-Bit ADC Packing (Not Compression)

The analog data uses 12-bit packed encoding rather than 16-bit padded
encoding. This is a fixed encoding scheme determined by the FPGA/ADC
hardware, not variable-length compression:

- **16-bit GPIF mode**: 4 samples packed in 6 bytes (12 bits each)
- **32-bit GPIF mode**: 8 samples packed in 12 bytes (12 bits each,
  unpacked via SSE psraw instructions)

This achieves a 25% space savings compared to 16-bit-per-sample encoding
(12/16 = 75% of the naive size). But this is a fixed, deterministic
encoding -- every sample always takes exactly 1.5 bytes. The deinterleaver
knows the exact byte offsets without needing any length headers or
variable-length decoding.


## Implications for the Rust Implementation

1. **No decompression needed** -- the Rust USB data receiver does not need
   any decompression logic. Raw USB bulk transfer data contains interleaved
   sample data that can be split by fixed byte offsets.

2. **Sample rate validation is critical** -- the driver must enforce the
   same bandwidth checks as the original software. Requesting a sample
   rate that exceeds USB bandwidth will cause FIFO overflows in the FPGA,
   not graceful degradation.

3. **Transfer size should be adaptive** -- use the same formula as
   ComputeTransferSize to set USB bulk transfer buffer sizes based on the
   configured data rate.

4. **USB speed detection** -- the driver must detect USB 2.0 vs 3.0 and
   adjust available sample rates accordingly. This should be done at
   device enumeration time by examining endpoint max packet sizes.

5. **SALEAE_USB_TRANSFER_LIMIT** -- honor this environment variable for
   debugging, same as the original software.


## Confidence Assessment

| Finding | Confidence | Evidence |
|---------|-----------|----------|
| No device-side compression | **High** | Exhaustive string search, full decompilation of USB data pipeline, no decompression code found |
| Rate limiting is the primary bandwidth mechanism | **Confirmed** | GetInterfaceMaximumBitRate lookup table + performance options validation |
| USB speed lookup table values (6M/300M/3G) | **Confirmed** | Binary read of ELF VA 0x4748f0, cross-referenced with decompiled access pattern |
| USB speed detection via max packet size | **Confirmed** | Decompilation of IsFirmwareDownloaded (VA 0x2e9b410) |
| ComputeTransferSize formula | **High** | Decompilation of VA 0x1d3c0a0, formula matches observed log messages |
| AdjustTransferSize alignment (4KB/16KB) | **High** | Decompilation of VA 0x1d4fc50 |
| hasLimitedBandwidth is a boolean flag | **Confirmed** | Stored at offset +0x59 (1 byte), serialized from JSON |
| ContractReadData is bus-width padding removal | **Confirmed** | Previous decompilation (disaggregator-interleaving.md) |
| 12-bit packing is fixed encoding, not compression | **Confirmed** | Previous decompilation of Pro deinterleaver |
| FPGA constants (min/max samples per packet) | **Confirmed** | Direct extraction from ValidateConfig assertions |
| Host-side RLE is storage-only | **Confirmed** | AppendRle in Saleae::Digital::Detail namespace, runs after disaggregation |
| AnalogDownsamplerNode is post-capture | **High** | Graph pipeline ordering, "Cannot set downsampler rate after execution has begun" |
| Bandwidth tier table (100/80/60/40/20%) | **High** | Decompilation of GetAnalogAdvancedPerformanceOptions |
| 5-tier bandwidth model | **High** | Loop `lVar16 != 5` in GetAnalogAdvancedPerformanceOptions |


## Open Questions

1. **Exact bandwidth formula for mixed mode** -- The
   `GetAnalogAdvancedPerformanceOptions` function has complex logic for
   combining digital and analog bandwidth. The exact formula for how
   digital sample rate, analog sample rate, and channel counts combine
   needs more detailed decompilation.

2. **What triggers the 5-tier system vs single-tier** -- Digital-only
   captures seem to use a single bandwidth check, while mixed
   analog+digital captures use the 5-tier system. The switching logic
   needs confirmation.

3. **ComputeTransferSize overrides** -- `ComputeTransferSize` is virtual.
   Do `LogicStudentDevice`, `LogicGraduateDevice`, or
   `LogicProfessionalDevice` override it with different formulas?

4. **Default transfer limit** -- The default cap of 0x190000 (1,638,400)
   bytes in AdjustTransferSize -- is this always the default, or does it
   vary by device type?


## Key Virtual Addresses

| Function | ELF VA | Ghidra VA | Source |
|----------|--------|-----------|--------|
| GetInterfaceMaximumBitRate | 0x1c4bac0 | 0x1d4bac0 | DeviceSettings.cpp:0x36f |
| GetDigitalOnlyPerformanceOptions | 0x1c49ec0 | 0x1d49ec0 | DeviceSettings.cpp:0x38f |
| GetAnalogAdvancedPerformanceOptions | 0x1c4a250 | 0x1d4a250 | DeviceSettings.cpp:0x3e5 |
| ComputeTransferSize | 0x1c3c0a0 | 0x1d3c0a0 | LogicDevice.cpp:0x161 |
| AdjustTransferSize | 0x1c4fc50 | 0x1d4fc50 | LogicAnalyzerDevice.cpp:0x129 |
| ContractReadData | 0x1c3bad0 | 0x1d3bad0 | LogicDevice.cpp:0x16e |
| IsFirmwareDownloaded (speed detect) | 0x1d9b410 | 0x2e9b410 | -- |
| ValidateConfig / SetAnalogChannelConfig | 0x1d64640 | 0x2e64640 | mso_fpga.cpp:0x256 |
| DeviceCalculateMixRate | 0x1c19fa0 | 0x1d19fa0 | DeviceHelpers.cpp |
| Bit rate lookup table | 0x4748f0 | 0x5748f0 | DeviceSettings.cpp (data) |


## Source Code References (from embedded paths)

```
/workspace/monorepo/graph-io/devices/logic_interface/src/DeviceSettings.cpp
/workspace/monorepo/graph-io/devices/logic_interface/src/LogicDevice.cpp
/workspace/monorepo/graph-io/devices/logic_interface/src/LogicAnalyzerDevice.cpp
/workspace/monorepo/graph-io/devices/mso_device/src/mso_device/component/mso_fpga.cpp
/workspace/monorepo/graph-io/devices/mso_device/src/mso_device/oscilloscope_instrument.cpp
/workspace/monorepo/graph-io/nodes/analog_downsampler_node/src/analog_downsampler_node.cpp
/workspace/monorepo/graph-io/devices/logic_interface/src/DeviceHelpers.cpp
```
