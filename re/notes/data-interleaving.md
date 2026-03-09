# USB Data Interleaving and Disaggregation Architecture

Analysis date: 2026-03-09
Binary: libgraph_server_shared.so (48MB, stripped, ELF 64-bit x86-64)
Source: Saleae Logic 2 v2.4.40
Tasks: TASK-0035, TASK-0038


## Summary

All enabled channels -- both digital and analog -- share a single USB EP2 IN
(0x82) bulk transfer stream. The data is **sample-interleaved at the byte
level**, with each "sample row" consisting of a fixed number of bytes that
spans all enabled channels. The `DisaggregatorNode` graph kernel splits this
interleaved byte stream into per-channel data using layout metadata provided
by `DisaggregatorSettingsData`.

The interleaving format is determined by the FPGA's output data bus width
(the "GPIF width") and the number/type of enabled channels. The FX3
microcontroller's GPIF II interface transfers data from the FPGA in
fixed-width bus words (16-bit or 32-bit), and these words contain
interleaved samples from all active channels.

Key findings:

1. **Byte-level sample interleaving**: Each USB transfer contains N "rows"
   of a fixed width. Each row contains one sample-period's worth of data
   for all enabled channels, laid out contiguously.

2. **Two bus width modes**: The GPIF data bus between FPGA and FX3 operates
   in either 16-bit or 32-bit mode, selected based on the number of
   enabled analog channels. The string `"No valid analog and gpif width
   combo given"` confirms this selection logic.

3. **ContractReadData / ExpandWriteData**: These functions handle the
   case where the physical bus width is wider than the actual data needed.
   `ContractReadData` removes padding bytes from wider-than-needed bus
   transfers (compacting the data). `ExpandWriteData` adds padding bytes
   for writing configuration data back to the FPGA. This confirms that
   the FPGA always outputs data at a fixed bus width, even when fewer
   channels are active.

4. **Digital data**: Packed 8 samples per byte. The assertion
   `rawdata.size() * 8 == encoding_state.num_samples` confirms this.
   The constraint `channel_count == 2 || channel_count == 4` indicates
   digital channels are always captured in groups of 2 or 4.

5. **Analog data**: 12-bit ADC values stored as int16_t (2 bytes per
   sample). After disaggregation, analog channels produce
   `VectorData<int16_t>`.

6. **Per-packet stats**: When stats collection is enabled,
   `SamplesPerPacketMultiple_WithStats` (256 samples) alignment is
   enforced, with stats metadata appended to each packet. The
   `mAnalogStats` field contains per-packet statistics.


## Data Flow Architecture

```
FPGA                    FX3 (GPIF II)           Host (USB)
 |                         |                       |
 | 16/32-bit parallel bus  |                       |
 | (sample-interleaved)    |                       |
 +------------------------>+                       |
                           | EP2 IN bulk xfer      |
                           +---------------------->|
                                                   |
                                          TransferQueue
                                                   |
                                          OnDataBuffer
                                                   |
                                      [ContractReadData]
                                      (if bus width > needed)
                                                   |
                                       DisaggregatorNode
                                        /     |     \
                                       /      |      \
                              Digital ch  Analog ch  Analog ch
                              OwnedSpan   VectorData  VectorData
                              <uint8_t>   <int16_t>   <int16_t>
                                  |           |           |
                           DigitalCreator  AnalogCalibFilter  ...
                                  |           |
                            DigitalStore   AnalogCreator
                                              |
                                         AnalogStore
```


## DisaggregatorNode

### Type Hierarchy (from RTTI)

```
Saleae::Graph::DisaggregatorNode         -- the kernel itself
Saleae::Graph::DisaggregatorSettingsData  -- channel layout description
Saleae::Graph::DisaggregatorNodeState2   -- runtime state
Saleae::Graph::DisaggregatorActions::ConfigureGlitchFilters  -- action
```

### Input Pipes

The DisaggregatorNode registers pipe handlers for three data types (confirmed
by mangled template instantiation names):

1. `OwnedSpan<uint8_t>` -- raw interleaved USB bytes (the main data input)
2. `DisaggregatorSettingsData` -- channel layout (must arrive BEFORE data)
3. `CaptureCompleteData` -- end-of-capture signal

The error string `"DisaggregatorNode requires DisaggregatorSettingsData before
any other data"` confirms that settings must be received first.

### Output Pipes

Per channel, the disaggregator emits:
- Digital channels: `OwnedSpan<uint8_t>` -> then converted to `DigitalData`
- Analog channels: `VectorData<int16_t>` (12-bit ADC values in 16-bit container)

### Glitch Filters

The disaggregator supports per-channel digital glitch filtering:
- Configured via `ConfigureGlitchFilters` action
- `"Cannot configure glitch filters after node has received data"` --
  filters must be configured before data arrives
- `"Tried to configure glitch filter for invalid channel: "` -- validates
  channel index
- Pulse width constraints: `mPulseWidth == 5 || (mPulseWidth % 10 == 0)`
- Range: `[digitalLimit_MinGlitchFilterLength, digitalLimit_MaxGlitchFilterLength]`


## DisaggregatorSettingsData Structure

### Construction

`GetDisaggregatorSettings` is the factory function:

```cpp
std::shared_ptr<DisaggregatorSettingsData>
    Saleae::Graph::LogicDeviceNode::GetDisaggregatorSettings(
        Saleae::Graph::GraphTime)
```

This is called from `LogicDeviceNode` (source path:
`/workspace/monorepo/graph-io/nodes/logic_device_node/src/logic_device_node/logic_device_node.cpp`).

The function takes a `GraphTime` parameter, suggesting the settings may
vary during a capture session (e.g., if the configuration changes mid-stream).

### Inferred Fields (Hypothesis)

Based on what the disaggregator needs to split interleaved data, the
`DisaggregatorSettingsData` struct likely contains:

| Field (hypothesized) | Type | Purpose |
|---------------------|------|---------|
| totalBytesPerRow | uint32_t | Total bytes per interleaved sample row |
| channelEntries | vector | Per-channel layout entries |
|  .channelIndex | uint32_t | Output channel index |
|  .byteOffset | uint32_t | Byte offset within the sample row |
|  .byteWidth | uint32_t | Number of bytes per sample for this channel |
|  .isAnalog | bool | Whether this is an analog or digital channel |
| samplesPerChunk | uint32_t | Number of sample rows per chunk |
| statsEnabled | bool | Whether per-chunk stats are appended |

**Confidence: Hypothesis** -- The exact field names and layout require Ghidra
decompilation of `GetDisaggregatorSettings`. The above is inferred from:
- The need to know where each channel's data sits in each row
- The digital/analog type distinction for output pipe selection
- The chunk size for buffer management
- The stats flag for skipping stats bytes at chunk boundaries

### Serialization

The type has both a `DummyConverter` (no-op serialization) and a
`SerializeConverter` for `DisaggregatorNodeState2`, suggesting:
- `DisaggregatorSettingsData` is passed by shared_ptr, not serialized
  (DummyConverter means it is not serialized to JSON for IPC)
- `DisaggregatorNodeState2` IS serialized (for save/restore of node state)


## GPIF Width and Data Bus Format

### FX3 GPIF II Interface

The Cypress FX3 uses GPIF II (General Programmable Interface II) to
connect to the FPGA. GPIF II supports 16-bit and 32-bit bus widths.

Evidence:
- `"No valid analog and gpif width combo given"` -- selection logic
  exists that chooses the GPIF width based on the analog channel config
- `"Enabling GPIF, Flush timeout set to {}ns (every {} ticks @ 100Mhz)"`
  -- GPIF is explicitly enabled with timing configuration
- Register `0x0020` (GPIF_ENABLE) controls the GPIF interface
- Register `0x0024` (GPIF_FLUSH_TIMEOUT) sets the flush timeout

### Width Selection (Hypothesis)

The GPIF width is likely selected as follows:
- **16-bit mode**: When only digital channels are enabled, or when few
  analog channels are active. Each bus word carries 2 bytes of data.
- **32-bit mode**: When multiple analog channels are enabled (each
  analog sample is 2 bytes, so 32 bits can carry 2 analog samples or
  1 analog + 2 digital bytes).

**Confidence: Hypothesis** -- Needs Ghidra decompilation of the width
selection logic near the "No valid analog and gpif width combo" error.


## Interleaving Pattern

### Sample-Interleaved at Byte Level

The data is interleaved at the sample level, not at the chunk or block
level. This means:

```
For a configuration with 2 digital channels (D0, D1) and 2 analog
channels (A0, A1):

Byte layout per sample row (hypothesis):
  [D0_D1_byte] [A0_lo] [A0_hi] [A1_lo] [A1_hi]
  |-- 1 byte --|--- 2 bytes ---|--- 2 bytes ---|

Repeated for every sample in the chunk:
  Row 0: [D0D1] [A0_lo] [A0_hi] [A1_lo] [A1_hi]
  Row 1: [D0D1] [A0_lo] [A0_hi] [A1_lo] [A1_hi]
  ...
  Row N: [D0D1] [A0_lo] [A0_hi] [A1_lo] [A1_hi]
  [optional stats bytes]
```

### Evidence for Sample-Level Interleaving

1. **DisaggregatorNode receives raw bytes and emits per-channel data**:
   If data were block-interleaved (all ch0 data, then all ch1 data),
   the disaggregator would just need to split at block boundaries. But
   the existence of `ContractReadData` (which removes per-row padding)
   implies row-by-row processing.

2. **`ContractReadData(U8 *)`**: This function operates on a buffer and
   "contracts" it -- removing bytes. This is consistent with removing
   per-row padding from a bus-width-aligned interleaved stream. If data
   were block-interleaved, there would be no need for per-row contraction.

3. **`channel_count == 2 || channel_count == 4`**: Digital channels come
   in groups of 2 or 4, matching the GPIF bus width (16 or 32 bits).
   This constraint makes sense for byte-packed sample rows where digital
   samples fit into aligned bus words.

4. **`rawdata.size() * 8 == encoding_state.num_samples`**: After
   disaggregation, each digital channel's data is exactly 1 bit per
   sample packed into bytes. This means the disaggregator extracts
   specific bits from each sample row.

5. **FPGA architecture**: FPGAs naturally produce sample-interleaved data
   because all channels are sampled simultaneously by the same clock.
   The FPGA's output FIFO would contain one sample row per clock cycle.

6. **FakeReadThread(U32, U32)**: The two parameters to the simulation
   thread are likely (num_channels, samples_per_chunk) or
   (total_row_width, num_rows). The simulation generates data in the
   same format as the real FPGA.

7. **ModifyFakeData signature**: `LogicDevice::ModifyFakeData(U8*, U32,
   U64, U32, U32, BitState, ChunkedArray<U64>*)` takes a buffer pointer,
   buffer size, sample offset, samples per chunk, number of channels,
   initial bit state, and transition timestamps. The fact that it needs
   "number of channels" to write fake data into a buffer confirms that
   the buffer format depends on the channel layout (i.e., interleaved).

### Analog Channel Placement

For Pro devices with AD9637 or ADS528x (12-bit ADCs), each analog sample
is 2 bytes (int16_t). The analog channels are placed in the sample row
after the digital channel bytes.

The `"No valid analog and gpif width combo given"` error from
`LogicStudentDevice` (Logic 4) suggests that not all combinations of
analog channel count and bus width are valid. The Logic 4 likely has
restrictions on which channels can be enabled simultaneously.

### Per-Packet Stats

When stats are enabled:
- Each chunk of `mSamplesPerChunk` rows is followed by stats metadata
- `SamplesPerPacketMultiple_WithStats = 256` -- alignment requirement
- `offset_into_stats_bytes < data.mAnalogStats.size_bytes()` -- stats
  are accessed by byte offset
- `data.mAnalogStats.size() > 0` -- stats vector is non-empty
- Stats contain overflow counters (`cur_overflow`, `prev_overflow`)
  and possibly timing information


## Data Contraction / Expansion

### ContractReadData

`void LogicDevice::ContractReadData(U8 *)` operates on a raw USB buffer
and removes padding bytes. This is needed when the GPIF bus width is wider
than the actual data per sample row.

Example: If only 1 digital channel is enabled (1 bit per sample, packed
into bytes), but the GPIF operates in 32-bit mode, each bus word would
contain 1 byte of useful digital data and 3 bytes of padding. Contract
removes the 3 padding bytes per word, producing a compact byte stream.

### ExpandWriteData

`void LogicDevice::ExpandWriteData(U8 *)` is the inverse -- it adds
padding bytes to match the GPIF bus width when writing configuration
data back to the FPGA.

### Relationship to Disaggregation

ContractReadData runs BEFORE the DisaggregatorNode (it operates on the
raw USB buffer in OnDataBuffer). The disaggregator then processes the
compacted data where each sample row contains only useful bytes.

**Confidence: Hypothesis** -- The exact ordering needs Ghidra
decompilation of `OnDataBuffer` to confirm whether contraction happens
before or after disaggregation, or whether the disaggregator handles
the bus-width padding internally.


## Device-Specific Behavior

### Logic 4 (LogicStudentDevice)

- `ModifyFakeData` has an extra U32 parameter: `void
  LogicStudentDevice::ModifyFakeData(U32, U8*, U32, U64, U32, U32,
  BitState, ChunkedArray<U64>*)` -- the extra parameter may be a
  channel group index or a bus width selector
- `"No valid analog and gpif width combo given"` -- width validation
  is most constrained on Logic 4
- `"digital channel cannot be enabled"` -- some digital channels may
  not be available on Logic 4
- Source: `LogicStudentDevice.cpp`

### Logic 8 (LogicGraduateDevice)

- Uses base class `ContractReadData` / `ExpandWriteData`
- `FakeReadThread` is in the base `Logic2FpgaDevice` class
- Source: `LogicGraduateDevice.cpp`

### Logic Pro 8/16 (LogicProfessionalDevice)

- More analog channels (8 or 16)
- Wider interleaved rows (more bytes per sample)
- AD9637 or ADS528x ADC (both 12-bit)
- Source: `LogicProfessionalDevice.cpp`

### Legacy Logic 16 (Logic16Device)

- Separate `FakeReadThread()` (no parameters, different format)
- FX2-based, completely different data format
- Out of scope for this analysis


## AnalogDeserializer

The string `"AnalogDeserializer"` exists in the binary near the
`LogicStudentDevice.cpp` source path. This may be a helper class or
function that handles the byte-to-int16_t conversion for analog
channel data after disaggregation. The relationship between
`AnalogDeserializer` and `DisaggregatorNode` is unclear -- it may be:

1. A component within the disaggregator that handles analog byte-pair
   extraction
2. A legacy component replaced by the disaggregator's analog output
3. A separate post-disaggregation stage

The `WaveformDeserializer` string also exists and is likely MSO-specific
(out of scope).


## Confidence Assessment

| Finding | Confidence | Sources |
|---------|-----------|---------|
| All channels share EP2 IN | **Confirmed** | Binary strings, pipeline architecture |
| DisaggregatorNode splits interleaved data | **Confirmed** | RTTI, pipe handler registrations, error strings |
| DisaggregatorSettingsData describes layout | **Confirmed** | GetDisaggregatorSettings function, pipe handler |
| Settings must arrive before data | **Confirmed** | Error string in binary |
| Digital: 8 samples/byte, groups of 2 or 4 | **Confirmed** | Assertion strings |
| Analog: int16_t (2 bytes/sample) | **Confirmed** | Template instantiations, pipe handlers |
| Sample-interleaved (not block-interleaved) | **High** | ContractReadData, ModifyFakeData signature, FPGA architecture |
| GPIF width is 16-bit or 32-bit | **High** | "No valid analog and gpif width combo", GPIF registers |
| ContractReadData removes bus-width padding | **High** | Function name, ExpandWriteData inverse, LogicDevice context |
| Per-packet stats appended after sample rows | **High** | mAnalogStats assertions, WithStats alignment |
| DisaggregatorSettingsData struct layout | **Hypothesis** | Inferred from functional requirements |
| GPIF width selection logic | **Hypothesis** | "No valid combo" error, needs decompilation |
| ContractReadData runs before disaggregator | **Hypothesis** | Logical ordering, needs decompilation |


## Open Questions

1. **Exact DisaggregatorSettingsData struct layout**: The field names,
   types, sizes, and offsets. Requires Ghidra decompilation of
   `GetDisaggregatorSettings`.

2. **Exact sample row format per device**: What is the byte layout for
   each device model at each channel configuration? The ModifyFakeData
   and FakeReadThread functions would reveal this.

3. **Stats format**: Exactly what bytes are in the per-chunk stats
   metadata? How many bytes of stats per chunk?

4. **Bit ordering within digital bytes**: Is bit 0 the first sample
   or the last? MSB-first or LSB-first packing?

5. **ContractReadData algorithm**: The exact contraction logic (which
   bytes are removed and how). Needs Ghidra decompilation.

6. **GPIF width for each device model**: Which devices use 16-bit vs
   32-bit GPIF, and how does the analog channel count affect this?

7. **Start packet format**: The exact binary layout of the start packet
   that precedes sample data (ch, timestamp, time, config_id fields).

8. **Analog sample byte order**: Are int16_t analog samples stored
   little-endian or big-endian in the USB stream? (Likely little-endian
   since the FX3 is an ARM core and x86 host is LE, but the FPGA may
   use big-endian.)


## Ghidra Analysis Targets

The `TraceDisaggregator.py` Ghidra script targets:

1. **GetDisaggregatorSettings body**: Extract struct construction to
   determine field layout of DisaggregatorSettingsData.

2. **DisaggregatorNode data handler**: The function that processes
   incoming OwnedSpan<uint8_t> and emits per-channel data. This
   contains the actual deinterleaving loop.

3. **ContractReadData / ExpandWriteData bodies**: Extract the
   byte-removal/addition algorithm to understand bus-width alignment.

4. **ModifyFakeData bodies** (both LogicDevice and LogicStudentDevice
   variants): Extract the data placement logic that writes known
   patterns into the interleaved buffer.

5. **FakeReadThread body**: Extract the simulation data generation
   loop that produces interleaved data in hardware format.

6. **GPIF width selection**: The code path that leads to "No valid
   analog and gpif width combo given" -- what combinations are tried.


## Source Code References (from embedded paths)

```
/workspace/monorepo/graph-io/nodes/logic_device_node/src/logic_device_node/logic_device_node.cpp
/workspace/monorepo/graph-io/devices/logic_interface/src/LogicDevice.cpp
/workspace/monorepo/graph-io/devices/logic_interface/src/Logic2FpgaDevice.cpp
/workspace/monorepo/graph-io/devices/logic_interface/src/LogicStudentDevice.cpp
/workspace/monorepo/graph-io/devices/logic_interface/src/LogicGraduateDevice.cpp
/workspace/monorepo/graph-io/devices/logic_interface/src/LogicProfessionalDevice.cpp
/workspace/monorepo/graph-io/devices/logic_interface/src/DeviceSettings.cpp
```


## New Leads (Backlog Candidates)

- **ContractReadData decompilation**: The exact byte-removal algorithm
  would reveal the bus-width alignment and sample row structure.
  Priority: high, as it directly determines the data format.

- **FakeReadThread decompilation**: The simulation data generator
  produces data identical to the real FPGA. Decompiling it would give
  us a reference implementation of the interleaving format.

- **GPIF width table**: There must be a lookup table or switch statement
  that maps (device_type, analog_channel_count) to GPIF width. Finding
  this would complete the bus format picture.

- **Start packet parser**: The "Got start packet: ch={} timestamp={},
  time={}, config_id={}" handler reveals the start packet format.
  This packet establishes the stream before data flows.

- **AnalogDeserializer**: May contain byte-ordering or bit-shifting
  logic that reveals how raw ADC bits are packed into int16_t.
