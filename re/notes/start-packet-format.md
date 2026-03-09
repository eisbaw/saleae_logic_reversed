# Start Packet Binary Format

Analysis date: 2026-03-09
Binary: libgraph_server_shared.so (48MB, stripped, ELF 64-bit x86-64)
Source: Saleae Logic 2 v2.4.40
Task: TASK-0036


## Summary

The "start packet" is not a standalone USB packet -- it is a per-channel
header embedded within the interleaved EP2 IN bulk data stream. Each data
chunk begins with a 4-byte header word that carries channel identification,
a start/data discriminator bit, and a config_id. When the start bit is set,
two additional fields follow: a 32-bit device timestamp and a 64-bit host
time value.

The start packet establishes the beginning of a channel's data stream.
The host rejects data for a channel that has not received its start packet:
"Received digital data without ever receiving a start packet".


## Discovery Method

1. String search in .rodata for "Got start packet", "Expected start packet",
   and "Received digital data without ever receiving a start packet"
2. RIP-relative LEA scan to locate code referencing those strings:
   - "Got start packet" (0x376ec7) -> code at VA 0x2d51fdc
   - "Received digital data without start packet" (0x3f4808) -> VA 0x2d47324
   - "Expected start packet" (0x41fca8) -> VA 0x2d523c9
3. Function boundary analysis: the handler function starts at VA 0x2d51710
   (DigitalCreatorNode's VectorData<uint8_t> pipe handler, lambda $_1)
4. Identified the per-chunk parsing function at VA 0x2d41c00, called from
   the handler at VA 0x2d517d1
5. Disassembly of the parsing function to extract field sizes, positions,
   and bit masks

### Key Code Regions

| Region | VA Range | File Offset | Purpose |
|--------|----------|-------------|---------|
| Chunk header parser | 0x2d41c00 - 0x2d41dfa | 0x2d40c00 | Parses header + start packet fields |
| read_u32 helper | 0x2d42050 - 0x2d420b2 | 0x2d41050 | Reads 4 bytes from stream cursor |
| read_u64 helper | 0x2d421d0 - 0x2d42234 | 0x2d411d0 | Reads 8 bytes from stream cursor |
| DigitalCreatorNode handler | 0x2d51710 - 0x2d52700 | 0x2d50710 | Processes start packets + data |
| Per-packet stats parser | 0x2d41510 - 0x2d41a00 | 0x2d40510 | Different struct, stats metadata |


## Binary Layout: Chunk Header

Every data chunk in the interleaved EP2 IN stream begins with a 4-byte
header word, read as a little-endian uint32:

```
Byte 0    Byte 1         Byte 2    Byte 3
+--------+--------+------+--------+--------+
|channel | flags  |    config_id           |
|[7:0]   |[15:8]  |    [31:16]             |
+--------+--------+------+--------+--------+
```

### Header Fields (from parsing code at VA 0x2d41ced-0x2d41d0b)

| Bits | Width | Field | Description |
|------|-------|-------|-------------|
| [7:0] | 8 bits | channel | Channel number (0-3). Stored at output struct offset 0x03. Validated: `channel < 4` |
| [8] | 1 bit | software | Software-generated data flag. Stored at struct offset 0x02. Extracted: `(header >> 8) & 1` |
| [9] | 1 bit | is_start | Start packet discriminator. When set, start packet fields follow. Checked: `header & 0x200` |
| [15:10] | 6 bits | reserved | Likely zero. Not explicitly extracted |
| [31:16] | 16 bits | config_id | Configuration ID for stream synchronization. Stored at struct offset 0x00 as uint16. Extracted: `header >> 16` |

### Parsing Pseudocode (from VA 0x2d41c00)

```c
struct ChunkHeader {
    uint16_t config_id;    // offset 0x00
    uint8_t  software;     // offset 0x02
    uint8_t  channel;      // offset 0x03
    // 4 bytes padding
    uint32_t timestamp;    // offset 0x08 (only if is_start)
    // 4 bytes padding
    uint64_t time;         // offset 0x10 (only if is_start)
    uint8_t  has_start;    // offset 0x18
    // padding
    uint32_t data_offset;  // offset 0x20 (cursor position after header)
};

ChunkHeader parse_chunk_header(Stream *stream) {
    ChunkHeader h = {};

    uint32_t header = stream_read_u32(stream, 4);

    h.config_id = (uint16_t)(header >> 16);
    h.software  = (header >> 8) & 1;
    h.channel   = (uint8_t)(header & 0xFF);

    if (header & 0x200) {  // bit 9 = is_start
        h.has_start = 1;
        h.timestamp = stream_read_u32(stream, 4);
        h.time      = stream_read_u64(stream, 8);
    }

    h.data_offset = stream->position;

    // Align to 4-byte boundary
    uint32_t padding = (-stream->position) & 3;
    stream_skip(stream, min(padding, stream_remaining(stream)));

    return h;
}
```


## Binary Layout: Start Packet on Wire

When `is_start` (bit 9) is set in the header, the start packet occupies
16 bytes total on the wire:

```
Offset  Size  Field        Endianness
------  ----  -----------  ----------
0x00    4     header       little-endian uint32
0x04    4     timestamp    little-endian uint32 (device clock)
0x0C    8     time         little-endian uint64 (host/graph time)
------  ----
Total: 16 bytes
```

### Header Bit Layout (detail)

```
Bit:  31 30 29 28 27 26 25 24 23 22 21 20 19 18 17 16 15 14 13 12 11 10  9  8  7  6  5  4  3  2  1  0
      |<-------- config_id (16 bits) -------->|  |<-- reserved -->| |IS|SW| |<-- channel (8 bits) --->|
                                                                     ^   ^
                                                                     |   |
                                                          is_start --+   +-- software
```


## Binary Layout: Data Packet on Wire

When `is_start` is NOT set (bit 9 = 0), the header is followed directly
by sample data. The header is only 4 bytes:

```
Offset  Size  Field        Endianness
------  ----  -----------  ----------
0x00    4     header       little-endian uint32
0x04    N     sample_data  raw bytes (digital: 8 samples/byte)
------  ----
Total: 4 + N bytes
```

After the 4-byte header, any alignment padding needed to reach a 4-byte
boundary is skipped (0-3 bytes).


## Per-Channel Start Packet State

The DigitalCreatorNode maintains per-channel state to track whether each
channel has received its start packet. Up to 4 channels are supported.

Each channel's state is stored at a fixed offset within the node's internal
data structure, with a stride of 0x30 (48) bytes per channel:

```
Base offset in node: 0x4d8 (per channel, stride 0x30)

Offset  Size  Field
------  ----  ---------
+0x00   8     time_reference (double, initialized to 1.25e9 = 0x41d2a05f20000000)
+0x08   16    start_data (16 bytes: 128-bit copy of timestamp + time from start packet)
+0x18   4     config_id (uint32, copied from header's config_id field)
+0x1C   4     padding
+0x20   8     sample_counter (uint64, initialized to 0)
+0x28   1     has_start_packet (bool, set to 1 when start packet received)
+0x29   7     padding
```

Total per channel: 0x30 = 48 bytes
Total for 4 channels: 192 bytes


## Control Flow

```
USB EP2 IN bulk transfer arrives
    |
    v
LogicDeviceNode::OnDataBuffer
    |
    v
DisaggregatorNode (splits interleaved data by channel)
    |
    v
DigitalCreatorNode (lambda $_1 pipe handler, VA 0x2d51710)
    |
    +-- parse_chunk_header (VA 0x2d41c00)
    |       |
    |       +-- Reads 4-byte header word
    |       +-- Extracts channel, software, is_start, config_id
    |       +-- If is_start: reads timestamp (u32) and time (u64)
    |       +-- Aligns cursor to 4-byte boundary
    |
    +-- channel < 4? (VA 0x2d518f5)
    |       No -> error
    |
    +-- Device configured? (VA 0x2d518ff)
    |       No -> "Received digital data before device was configured"
    |
    +-- has_start_data? (VA 0x2d51c00)
    |       |
    |   Yes: Process start packet
    |       +-- Compare config_id with previous (VA 0x2d51c16)
    |       +-- Store timestamp+time in per-channel state
    |       +-- Store config_id
    |       +-- Set has_start_packet = true
    |       +-- Log "Got start packet: ch={} timestamp={}, time={}, config_id={}"
    |       |
    |   No: Process data packet
    |       +-- Check has_start_packet flag (VA 0x2d51ebd)
    |       |   No -> "Received digital data without ever receiving a start packet"
    |       +-- Proceed to create DigitalData from sample bytes
    |
    +-- Create DigitalData object (0x58 bytes allocated at VA 0x2d52092)
    |       +-- Set channel, config_id, software flag
    |       +-- Set timestamp/time from per-channel start state
    |       +-- Copy sample data
    |
    +-- Emit to downstream (DigitalStore)
```


## Constant: Time Reference 1.25e9

The double value `1.25e9` (1,250,000,000.0, encoded as 0x41d2a05f20000000)
is stored in each channel's time_reference field. This likely represents
a base clock frequency or timestamp reference in nanoseconds.

At a 5 GHz FPGA clock (200 ps per tick), 1.25e9 ticks = 0.25 seconds.
At a 125 MHz reference clock (8 ns per tick), 1.25e9 * 8ns = 10 seconds.

The exact interpretation requires USB capture validation.


## Relationship to Per-Packet Stats

A separate parsing function at VA 0x2d41510 handles the per-packet
statistics metadata that is appended to sample data chunks. This parser
reads:

1. uint32 (4 bytes) -- likely packet_id or sample_start
2. uint64 (6 bytes advance) -- likely a 48-bit timestamp packed in 64-bit
3. uint16 (2 bytes) -- bitfield with 8 individual flags (bits 15-8) + low byte
4. uint64 (8 bytes) -- another timestamp

The stats struct has 8 boolean flags extracted from bits 15 down to 8 of
the uint16. The flag at bit 9 of the stats word (stored at struct offset
+0x06) controls whether additional overflow/stats data follows.

This is a DIFFERENT structure from the chunk header start packet. The chunk
header (parsed at VA 0x2d41c00) precedes the interleaved sample data, while
the stats (parsed at VA 0x2d41510) are appended after sample data within
each chunk.


## Confidence Assessment

| Finding | Confidence | Evidence |
|---------|-----------|----------|
| 4-byte header with bitfield layout | **High** | Direct disassembly of shift/mask at VA 0x2d41ced-0x2d41d06 |
| Bit 9 = is_start discriminator | **High** | `test eax, 0x200` at VA 0x2d41d06, branches to start packet path |
| Bit 8 = software flag | **High** | `shr ecx, 8; and cl, 1` at VA 0x2d41cf8-0x2d41cfb |
| Bits [7:0] = channel | **High** | `mov [r15+3], al` at VA 0x2d41d02, later `cmp rax, 4` in caller |
| Bits [31:16] = config_id | **High** | `shr ecx, 0x10; mov [r15], cx` at VA 0x2d41cef-0x2d41cf2 |
| Start packet timestamp = uint32 | **High** | `call read_u32(stream, 4)` at VA 0x2d41d26, stored at [r15+8] |
| Start packet time = uint64 | **High** | `call read_u64(stream, 8)` at VA 0x2d41d37, stored at [r15+0x10] |
| Total start packet = 16 bytes | **High** | 4 (header) + 4 (timestamp) + 8 (time) |
| 4-byte alignment after header | **High** | `neg; and 3; cmovae` at VA 0x2d41d50-0x2d41d5d |
| Up to 4 channels | **High** | `cmp rax, 4; jae error` at VA 0x2d518f5 in caller |
| Per-channel stride = 0x30 | **High** | `lea rbx, [rax+rax*2]; shl rbx, 4` at VA 0x2d51bed |
| has_start_packet flag per channel | **High** | `cmp byte [r12+rbx+0x500], 0` at VA 0x2d51ebd |
| Little-endian byte order | **High** | x86-64 native reads, no byte-swap instructions |
| Separate stats parser at VA 0x2d41510 | **High** | Different struct layout, called from HandleWaveformData |


## Open Questions

1. **Exact config_id semantics**: Is config_id a monotonically increasing
   counter per capture session, or a hash of the capture configuration?
   The code compares it against a stored value to detect configuration
   changes mid-stream.

2. **Timestamp units**: What clock does the 32-bit timestamp represent?
   FPGA clock ticks, microseconds, or something else? Needs USB capture
   correlation.

3. **Time field semantics**: The 64-bit time field -- is this a host-side
   `std::chrono` timestamp, a graph time in nanoseconds, or something
   else? The log says "time" vs "timestamp", suggesting they are different
   clocks.

4. **Time reference constant 1.25e9**: What is this value used for?
   Is it a divisor, a baseline, or an initial timestamp? Needs Ghidra
   decompilation of the code that uses `[per_channel+0x4d8]`.

5. **Reserved bits [15:10]**: Are these always zero, or do some encode
   additional flags? No explicit extraction was observed.

6. **Alignment padding content**: Is the 0-3 bytes of alignment padding
   zeroed, or does it contain meaningful data?

7. **How does the FPGA generate the start packet?**: The start packet
   may be generated by the FPGA when capture begins, or it may be
   inserted by the FX3 firmware. The "software" flag (bit 8) suggests
   at least some packets can be software-generated on the host side.


## References

- `re/notes/digital-capture-format.md` -- overall digital data pipeline
- `re/notes/data-interleaving.md` -- USB data interleaving and disaggregation
- `re/notes/capture-start-stop.md` -- capture start/stop command sequence
- `doc/protocol/digital-capture.md` -- established protocol documentation
- `re/notes/ep1-command-protocol.md` -- EP1 command protocol (separate from EP2 data)
- `re/ghidra/scripts/TraceDigitalCapture.py` -- Ghidra script for pipeline analysis


## Source Code References (from embedded paths)

The DigitalCreatorNode handler is compiled from:
```
/workspace/monorepo/graph-io/nodes/logic_device_node/src/logic_device_node/logic_device_node.cpp
```

The related source files:
```
/workspace/monorepo/graph-io/devices/logic_interface/src/LogicDevice.cpp
/workspace/monorepo/graph-io/devices/logic_interface/src/Logic2FpgaDevice.cpp
```
