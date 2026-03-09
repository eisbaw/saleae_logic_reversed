# WithStats Packet Metadata Format

Analysis date: 2026-03-09
Binary: libgraph_server_shared.so (48MB, stripped, ELF 64-bit x86-64)
Source: Saleae Logic 2 v2.4.40
Task: TASK-0037
Dependencies: TASK-0036 (start packet format), TASK-0023 (capture constants)


## Summary

When stats collection is enabled (the `stats` flag in `SamplingConfigurationData`),
each data chunk in the USB stream is followed by a stats metadata section. The
stats are **appended** to the sample data, not prepended. The stats section has
a mandatory 20-byte header followed by optional overflow extension data.

The stats parser was found at VA 0x2d41510 in `ParseMsoWaveformData`, called
from `HandleWaveformData` in `oscilloscope_instrument.cpp` (line ~647). While
this specific code path is for the MSO oscilloscope, the same FPGA framework
(`MsoFpga` in `mso_fpga.cpp`) generates the stats for all FX3-based devices
including Logic Pro 8/16, Logic 8, and Logic 4. The `ValidateConfig` function
in `mso_fpga.cpp` enforces the `SamplesPerPacketMultiple_WithStats = 256`
alignment constraint for all device types.

For Logic devices (as opposed to MSO), the `DisaggregatorNode` strips the
stats bytes from each chunk and stores them in `mAnalogStats`. The per-channel
sample data is then passed downstream without the stats suffix.


## Discovery Method

1. Located format string `"id={}, start={}, end={}, sample_count={}, config_id={},
   ts={}, software={}, cur_overflow={}, prev_overflow={}, samples={}"` at VA
   0x368fe8 in `.rodata`.

2. Found single code reference via RIP-relative LEA scan: VA 0x2d44da4 in
   `HandleWaveformData` (oscilloscope_instrument.cpp, line 0x287 = 647).

3. Traced call chain: `HandleWaveformData` (VA 0x2d44b80) calls the stats
   parser at VA 0x2d41510 (identified in TASK-0036 as `ParseMsoWaveformData`).

4. Disassembled the stats parser to extract the exact read sequence, struct
   layout, and conditional paths.

5. Identified helper functions:
   - `read_u32(stream, advance)` at VA 0x2d42050
   - `read_u64(stream, advance)` at VA 0x2d421d0
   - `read_u16(stream, advance)` at VA 0x2d42350
   - `skip(stream, count)` at VA 0x2d424d0

6. Cross-referenced with overflow detection in `Logic2FpgaDevice::PrintErrors`
   (VA ~0x1c1fd00, source: `Logic2FpgaDevice.cpp`, line 0xa9 = 169).


## Key Code Regions

| Region | VA Range | Source File | Purpose |
|--------|----------|-------------|---------|
| Stats parser | 0x2d41510 - 0x2d4193a | mso_data.cpp | ParseMsoWaveformData: parses stats section |
| HandleWaveformData | 0x2d44b80 - 0x2d45900 | oscilloscope_instrument.cpp | Calls stats parser, uses result |
| read_u32 helper | 0x2d42050 - 0x2d420b2 | mso_data.cpp | Read 4 bytes LE from stream |
| read_u64 helper | 0x2d421d0 - 0x2d42234 | mso_data.cpp | Read 8 bytes LE from stream |
| read_u16 helper | 0x2d42350 - 0x2d423b2 | mso_data.cpp | Read 2 bytes LE from stream |
| skip helper | 0x2d424d0 - 0x2d42508 | mso_data.cpp | Advance stream cursor |
| PrintErrors | ~0x1c1fd00 | Logic2FpgaDevice.cpp | Overflow detection for Logic devices |
| ValidateConfig | 0x2d64640 | mso_fpga.cpp | Enforces WithStats alignment |


## Stats Section Wire Format

### Position Within Chunk

```
|<-- sample data (mSamplesPerChunk * bytes_per_sample_row) -->|<-- stats -->|
```

Stats are appended at the END of each chunk. The total chunk size on the wire
is: `(mSamplesPerChunk * row_width_bytes) + stats_bytes`.

When stats are enabled:
- `mSamplesPerChunk` must be a multiple of 256 (`SamplesPerPacketMultiple_WithStats`)
- `(mPreTriggerLength + mPostTriggerLength)` must also be a multiple of 256

### Mandatory Stats Header (20 bytes, always present)

```
Offset  Size  Wire Bytes  Read Function      Description
------  ----  ----------  ----------------   -----------
0x00    4     [B0..B3]    read_u32(s, 4)     Packet counter (sequential ID)
0x04    6     [B4..B9]    read_u64(s, 6)     48-bit timestamp (packed in u64, LE)
0x0a    2     [Ba..Bb]    read_u16(s, 2)     Flags bitfield (see below)
0x0c    8     [Bc..B13]   read_u64(s, 8)     Host/graph time (u64, LE)
------  ----
Total: 20 bytes
```

All multi-byte values are little-endian (x86-64 native reads, no byte-swap).

### Flags Bitfield (u16 at wire offset 0x0a)

```
Bit 15 14 13 12 11 10  9  8  7  6  5  4  3  2  1  0
    |  |  |  |  |  |  |  |  |<--- low byte (8 bits) -->|
    |  |  |  |  |  |  |  +-- flag_h (struct offset 0x07)
    |  |  |  |  |  |  +----- overflow_meta (struct offset 0x06)
    |  |  |  |  |  +-------- flag_f (struct offset 0x05)
    |  |  |  |  +----------- flag_e (struct offset 0x04)
    |  |  |  +-------------- overflow_data (struct offset 0x03)
    |  |  +----------------- flag_c (struct offset 0x02)
    |  +-------------------- flag_b (struct offset 0x00)
    +----------------------- flag_a (struct offset 0x01)
```

Key flags:
- **Bit 9** (`overflow_meta`): When set, 16 bytes of overflow metadata follow
  the mandatory header.
- **Bit 12** (`overflow_data`): When set (and bit 9 is also set), overflow
  data blocks follow the overflow metadata.
- **Bit 10** (`flag_f`): Controls whether the debug log line is emitted
  (the format string with id/start/end/etc. is only logged when this flag
  is set).
- **Low byte** (bits 7:0): Channel index or status byte, stored at struct
  offset 0x08.

The remaining flags (bits 8, 11, 13, 14, 15) are extracted but their semantic
meaning is not yet determined. They are stored as individual booleans in the
parsed struct.


### Conditional: Overflow Metadata (16 bytes, when bit 9 set)

When `overflow_meta` (bit 9 of the flags u16) is set:

```
Offset  Size  Action
------  ----  ------
0x14    16    skip(stream, 0x10)  -- 16 bytes of overflow metadata
```

These 16 bytes are skipped by the parser (not individually parsed). The
stream cursor position before the skip is saved at struct offset 0x24, and
the constant 8 is stored at struct offset 0x20 (indicating the presence and
entry size of overflow data).

If `overflow_meta` is NOT set, struct offset 0x20 is set to 0 (no overflow).


### Conditional: Overflow Data Blocks (when bits 9 AND 12 set)

When both `overflow_meta` (bit 9) and `overflow_data` (bit 12) are set, the
remaining bytes after the 16-byte overflow metadata are interpreted as
overflow data blocks.

Each block is exactly **636 bytes**:
- **512 bytes** of sample data (256 samples * 2 bytes/sample for analog)
- **124 bytes** of per-block overflow statistics

```
n_blocks = remaining_bytes / 636

Processing:
  1. Compute sample_count = n_blocks * 256    -> stored at struct[0x28]
  2. Save stream position                     -> stored at struct[0x2c]
  3. skip(stream, n_blocks * 512)             -- skip sample data
  4. Compute stats_size = n_blocks * 124      -> stored at struct[0x30]
  5. Save stream position                     -> stored at struct[0x34]
  6. skip(stream, n_blocks * 124)             -- skip per-block stats
```

The division by 636 uses a compiler-optimized multiplication pattern:
- Magic constant: 0x670b453b92840671
- `n_blocks = (remaining * magic) >> 72` (mulhi + shift right by 8)

An exact-division check using the magic constant 0xb5efe63d2eb11b5f
(multiplicative inverse of 159 mod 2^64, with ror 2) validates that
`remaining_bytes` is cleanly divisible. If not, an `__cxa_throw` is invoked
with the error `"Invalid MSO waveform received, invalid data length"`.


### Alternate Path: No Overflow, Simple Sample Count (when bits 9=0 OR 12=0)

When bit 12 is NOT set (regardless of bit 9):

```
remaining_bytes = stream.end - stream.position
sample_count = remaining_bytes / 2            -> stored at struct[0x28]
data_offset = stream.position                 -> stored at struct[0x2c]
struct[0x30] = 0                              (no overflow stats)
```

This path handles the common case where no overflow occurred: the remaining
bytes are pure sample data at 2 bytes per sample.


## Output Struct Layout (ParseMsoWaveformData Result)

```
Offset  Size  Type     Description
------  ----  ------   -----------
0x00    1     bool     flag_b (bit 14 of flags u16)
0x01    1     bool     flag_a (bit 15 of flags u16)
0x02    1     bool     flag_c (bit 13 of flags u16)
0x03    1     bool     overflow_data (bit 12 of flags u16)
0x04    1     bool     flag_e (bit 11 of flags u16)
0x05    1     bool     flag_f / debug_log (bit 10 of flags u16)
0x06    1     bool     overflow_meta (bit 9 of flags u16)
0x07    1     bool     flag_h (bit 8 of flags u16)
0x08    1     uint8    channel / status (low byte of flags u16)
0x09    3     padding
0x0c    4     uint32   packet_id (sequential counter)
0x10    8     uint64   timestamp_48 (48-bit FPGA timestamp in u64)
0x18    8     uint64   host_time (host/graph time)
0x20    8     uint64   overflow_entry_size (8 when overflow present, 0 otherwise)
0x28    4     uint32   sample_count (or computed sample start)
0x2c    4     uint32   data_offset (stream position of sample data)
0x30    4     uint32   overflow_stats_size (n_blocks * 124, or 0)
0x34    4     uint32   overflow_stats_offset (stream position)
```

Total struct size: 0x38 (56 bytes).


## Relationship to Format String

The format string `"id={}, start={}, end={}, sample_count={}, config_id={},
ts={}, software={}, cur_overflow={}, prev_overflow={}"` is used in the
HandleWaveformData log at line 647 of oscilloscope_instrument.cpp.

Mapping to struct fields (based on argument setup at VA 0x2d44ce6-0x2d44d9f):

| Format arg | Struct offset | Wire origin |
|-----------|---------------|-------------|
| id | 0x0c | Packet counter (u32 at wire offset 0x00) |
| start | 0x28 | Computed sample start |
| end | 0x00 | Computed sample end |
| sample_count | 0x01 | Computed or flag-derived |
| config_id | 0x05 | Flag bit 10 (meaning unclear for this purpose) |
| ts | 0x10 | 48-bit FPGA timestamp (wire offset 0x04) |
| software | 0x0c | Possibly overlaps with id |
| cur_overflow | 0x28 | Computed overflow count |
| prev_overflow | 0x07 | Flag bit 8 |

Note: This mapping is approximate. The format library uses a complex
argument-passing convention, and the exact mapping of struct fields to
format arguments requires deeper analysis of the `fmt`-style formatter at
VA 0x2d4cef0. The struct layout and wire format are definitive; the argument
naming is less certain.


## Stats in the Logic Pro / Logic 8 Path

For Logic devices (Logic Pro 16, Logic Pro 8, Logic 8, Logic 4), the data
path is:

```
FPGA -> FX3 EP2 IN -> LogicDeviceNode::OnDataBuffer
  -> DisaggregatorNode -> DigitalCreatorNode / AnalogCreatorNode
```

The DisaggregatorNode strips the stats bytes from each chunk and stores them
in the `mAnalogStats` field of the data object (evidenced by the assertion
`"offset_into_stats_bytes < data.mAnalogStats.size_bytes()"` at VA 0x2d03a82).

The FPGA generates the same stats format for all FX3-based devices because:
1. All devices share the `MsoFpga` / `mso_fpga.cpp` framework
2. The `ValidateConfig` function at VA 0x2d64640 applies
   `SamplesPerPacketMultiple_WithStats = 256` for all device types
3. The overflow detection in `Logic2FpgaDevice::PrintErrors` uses the same
   `cur_overflow` / `prev_overflow` mechanism

The FPGA-side overflow types detected from stats:
- `"Digital Fifo overflow"` (VA ref 0x1c1fe02)
- `"Analog Fifo overflow"` (referenced nearby)
- `"Master Fifo overflow"` (referenced nearby)
- `"digital fsm error"` (FPGA state machine error)


## Relationship to Chunk Format

The complete on-wire format for a single chunk (when stats enabled):

```
+------------------------------------------------------+
| Chunk Header (4 bytes)                                |
|   [7:0]  channel                                      |
|   [8]    software flag                                |
|   [9]    is_start                                     |
|   [31:16] config_id                                   |
+------------------------------------------------------+
| If is_start: Start Packet Fields (12 bytes)           |
|   timestamp (u32) + time (u64)                        |
+------------------------------------------------------+
| Sample Data (mSamplesPerChunk * row_width bytes)      |
|   Interleaved per-channel data                        |
+------------------------------------------------------+
| Stats Section (20+ bytes, when stats enabled)         |
|   Mandatory header (20 bytes)                         |
|   Optional overflow metadata (16 bytes)               |
|   Optional overflow data blocks (N * 636 bytes)       |
+------------------------------------------------------+
```

The chunk header and start packet parsing are documented in
`re/notes/start-packet-format.md` (TASK-0036).


## Confidence Assessment

| Finding | Confidence | Evidence |
|---------|-----------|----------|
| 20-byte mandatory stats header | **High** | Direct disassembly of read_u32(4) + read_u64(6) + read_u16(2) + read_u64(8) at VA 0x2d415f7-0x2d41697 |
| Flags bitfield is u16 with 8 individual flag bits | **High** | shr/and extraction pattern at VA 0x2d4162c-0x2d41684 |
| Bit 9 controls overflow metadata presence | **High** | `cmpb $0x0, 0x6(%r15)` at VA 0x2d4169b, branches to overflow path |
| Bit 12 controls overflow data presence | **High** | `cmpb $0x0, 0x3(%r15)` at VA 0x2d416c5/0x2d41887, branches to data blocks |
| Overflow blocks are 636 bytes (512 sample + 124 stats) | **High** | Division by 636 via magic 0x670b453b92840671 at VA 0x2d4171c, skip(n*512) + skip(n*124) |
| 16-byte overflow metadata block | **High** | `skip(stream, 0x10)` at VA 0x2d416c0 |
| Stats appended (not prepended) | **High** | Stats parsed after sample data in both MSO and Logic paths |
| Same format for Logic Pro and MSO | **High** | Shared MsoFpga framework, common ValidateConfig, common overflow detection |
| Little-endian byte order | **High** | x86-64 native reads, no byte-swap instructions |
| 48-bit timestamp packed in 64-bit field | **High** | read_u64(stream, 6) advances 6 bytes but reads 8-byte field (upper bits zero) |
| Exact format string argument mapping | **Hypothesis** | Complex fmt-library calling convention makes precise mapping uncertain |
| Flag semantic names | **Hypothesis** | Only bits 9, 10, 12 have clear functional roles; others need more context |


## Open Questions

1. **Flag semantics**: What do bits 8, 11, 13, 14, 15 of the flags u16
   represent? They are extracted and stored but their functional roles in the
   Logic device path are not yet traced.

2. **Overflow metadata content**: The 16 bytes at wire offset 0x14 (when
   bit 9 is set) are skipped without parsing. What do they contain? Likely
   overflow counters and timing data, but the exact layout is unknown.

3. **Per-block overflow stats**: The 124 bytes per overflow block are
   consumed but not individually parsed by the stats parser. Their internal
   format is unknown.

4. **Digital stats vs analog stats**: The `mAnalogStats` name suggests
   stats may only apply to analog channels. Do digital-only captures have
   stats? The `SamplesPerPacketMultiple_WithStats` alignment is checked for
   all captures, suggesting stats always apply when enabled.

5. **Stats enable/disable**: When exactly is the `stats` flag set in the
   sampling configuration? Is it always enabled for Logic Pro, or only for
   specific modes?

6. **Exact byte count for Logic devices**: For digital-only captures on
   Logic 8 (1 byte per 8 samples), is the stats section still 20 bytes?
   The sample_count formula `remaining / 2` in the no-overflow path
   assumes 2 bytes per sample (analog). Digital channels may use a
   different divisor or the stats section may differ.

7. **48-bit timestamp resolution**: What clock drives the 48-bit timestamp?
   What is the tick rate? The 6-byte read for a timestamp suggests the FPGA
   counter is 48 bits wide.


## Leads for Future Investigation

- **DisaggregatorNode stats stripping**: Decompile the DisaggregatorNode's
  data handler to see exactly how it separates stats bytes from sample data.
  This would confirm the stats position and size for the Logic device path.

- **FakeReadThread stats generation**: Decompile `Logic2FpgaDevice::FakeReadThread`
  to see how it generates fake stats. The simulation code must produce
  stats in the exact same format as the hardware.

- **PrintErrors decompilation**: The overflow handler at ~VA 0x1c1fd00 reads
  the stats fields to detect Digital/Analog/Master FIFO overflow. Decompiling
  it would reveal which specific bytes are `cur_overflow` and `prev_overflow`.


## References

- `re/notes/start-packet-format.md` -- chunk header and start packet (TASK-0036)
- `doc/protocol/capture-constants.md` -- SamplesPerPacketMultiple_WithStats = 256
- `re/notes/digital-capture-format.md` -- digital data pipeline overview
- `re/notes/data-interleaving.md` -- interleaving architecture, mAnalogStats
- `doc/protocol/digital-capture.md` -- established digital capture protocol


## Source Code References (from embedded paths)

```
/workspace/monorepo/graph-io/devices/mso_device/src/mso_device/mso_data.cpp
    -- ParseMsoWaveformData (stats parser)
/workspace/monorepo/graph-io/devices/mso_device/src/mso_device/oscilloscope_instrument.cpp
    -- HandleWaveformData (calls stats parser, logs format string)
/workspace/monorepo/graph-io/devices/logic_interface/src/Logic2FpgaDevice.cpp
    -- PrintErrors (overflow detection using stats fields)
/workspace/monorepo/graph-io/devices/mso_device/src/mso_device/mso_fpga.cpp
    -- ValidateConfig (WithStats alignment enforcement)
```
