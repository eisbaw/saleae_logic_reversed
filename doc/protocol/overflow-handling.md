# Overflow Handling in EP2 Stats Stream

Status: High confidence (Ghidra decompilation of v2.4.40)
Source functions:
- `ParseMsoWaveformData` (ELF VA 0x2d41510, mso_data.cpp)
- `HandleWaveformData` (ELF VA 0x2d44b80, oscilloscope_instrument.cpp)
- `PrintErrors` (ELF VA 0x1c1fcb0, Logic2FpgaDevice.cpp)
- `WaveformDataCollection::Writer::PushBack` (ELF VA ~0x2d04f00, waveform_data_collection.cpp)
- `PackedWaveformGetStats` (ELF VA 0x2d037c0, waveform_data_collection.cpp)
- `ChunkIterator::GetStats` (ELF VA 0x2d03ab0, waveform_data_collection.cpp)


## Overview

When stats collection is enabled, each EP2 data chunk ends with a stats
section. The mandatory 20-byte stats header contains a flags bitfield. Two
flags control overflow data:

- **Bit 9** (`overflow_meta`): 16 bytes of overflow metadata follow the
  mandatory header.
- **Bit 12** (`overflow_data`): When set together with bit 9, overflow data
  blocks (636 bytes each) follow the overflow metadata.


## Overflow Metadata (16 bytes)

When bit 9 is set, 16 bytes follow the 20-byte mandatory stats header.
The parser stores `overflow_entry_size = 8` at the result struct, indicating
2 entries of 8 bytes each.

```
Offset  Size  Description
0x00    8     Previous overflow state (u64, LE)
0x08    8     Current overflow state (u64, LE)
```

**Important clarification**: The `cur_overflow` and `prev_overflow` names in
the HandleWaveformData format string refer to **single-bit flags** (bits 15
and 14 of the stats header flags u16), NOT to these 16 metadata bytes. The
metadata bytes are skipped entirely by ParseMsoWaveformData and not used by
HandleWaveformData. Overflow detection uses the 1-bit flags from the stats
header, not the 16-byte metadata block.

Argument mapping from HandleWaveformData format string call (VA 0x2e44d9f):
- `cur_overflow={}` -> `(long)&local_588 + 1` = struct byte[1] = **bit 15**
- `prev_overflow={}` -> `&local_588` = struct byte[0] = **bit 14**

### Confidence: High

The 16-byte total and 2x8 entry structure are **High confidence** (direct
decompilation). The fact that the parser skips them entirely (VA 0x2e416c0:
`skip(stream, 0x10)`) is **High confidence**. The single-bit overflow flag
mapping is **High confidence** (traced through HandleWaveformData argument
setup).


## Overflow Data Blocks (636 bytes each)

When both bits 9 and 12 are set, the remaining bytes after the 16-byte
metadata are organized as N blocks of exactly 636 bytes:

```
n_blocks = remaining_bytes / 636
```

If the remaining bytes are not evenly divisible by 636, the parser throws:
`"Invalid MSO waveform received, invalid data length"`

Each block contains:

```
Offset  Size  Description
0x000   512   Sample data (256 analog samples at 2 bytes each, int16 LE)
0x200   124   Per-block stats
```

Per-block processing:
1. `sample_count = n_blocks * 256` (stored at struct offset 0x28)
2. Sample data stream position saved (struct offset 0x2c)
3. Skip sample data: `skip(n_blocks * 512)`
4. `stats_size = n_blocks * 124` (struct offset 0x30)
5. Stats stream position saved (struct offset 0x34)
6. Skip stats: `skip(n_blocks * 124)`
7. Assert: `reader.BytesRemaining() == 0`

### Per-Block Stats (124 bytes) -- Multi-Resolution Min/Max Pyramid

The 124-byte per-block stats contain a **multi-resolution min/max summary
pyramid** for the 256 analog samples in the corresponding block. This is
used for fast waveform rendering at any zoom level without iterating all
samples.

Each entry is a packed `u32` containing two `int16` values:
`(max_i16 << 16) | (min_i16 & 0xFFFF)` (packed by `FUN_02ed1b60`).

The pyramid has 5 levels, computed by `Writer::PushBack`:

```
Level  Samples/Group  Entries/Block  Bytes  Stride (in sample bytes)
-----  -------------  -------------  -----  -------------------------
  0         16              16         64   0x20 (32 bytes)
  1         32               8         32   0x40 (64 bytes)
  2         64               4         16   0x80 (128 bytes)
  3        128               2          8   0x100 (256 bytes)
  4        256               1          4   0x200 (512 bytes)
-----                  -------------  -----
Total                       31        124   (= 0x7c bytes)
```

Each level computes min and max over pairs of consecutive int16 samples,
accumulating the overall minimum and maximum across the group. The result
is stored as a single packed u32 per group.

The consumer side uses `PackedWaveformGetStats` (ELF VA 0x2d037c0) which
takes a level parameter (0-4) and a block index. It accesses the pyramid
via a lookup table at data address `0x008942c0` containing per-level
offsets into the stats entries:

```c
// ChunkIterator::GetStats: recursive pyramid lookup
int entry_index = channel * DAT_008942c0[level] + block_index * 4;
int16 min = stats_data[entry_index];
int16 max = stats_data[entry_index + 1];
```

When a query spans multiple blocks, `ChunkIterator::GetStats` walks the
pyramid: it uses the coarsest level (level 4) for full blocks and recurses
to finer levels at the boundaries. This achieves O(log N) min/max lookups
instead of O(N) sample scans.

The assertion `written == expected` at `Writer::PushBack` (line 0x265 in
waveform_data_collection.cpp) verifies the output is exactly `n_blocks *
0x7c` bytes after writing all 5 levels.

### Confidence: High

The 124-byte size is **High confidence** (decompiled as `n_blocks * 0x7c`).
The multi-resolution min/max pyramid structure is **High confidence**
(decompiled from `Writer::PushBack` which generates the pyramid in 5
distinct loops with strides 0x20, 0x40, 0x80, 0x100, 0x200, and from
`PackedWaveformGetStats` which reads it back with level-based indexing).

The packed u32 format `(max << 16) | (min & 0xFFFF)` is **High confidence**
(the packing function `FUN_02ed1b60` is called at the end of each loop
iteration and stores min/max int16 values).


## No-Overflow Path

When bit 12 is NOT set (regardless of bit 9):

```
remaining_bytes = stream.end - stream.position
sample_count = remaining_bytes / 2
data_offset = stream.position
stats_size = 0
```

This handles the common case: remaining bytes are pure sample data at 2
bytes per analog sample.


## Error Types (from PrintErrors)

The `PrintErrors` function (ELF VA 0x1c1fcb0) checks a boolean array for
these error conditions:

| Array Index | Error String          |
|------------|-----------------------|
| 0          | Master Fifo overflow  |
| 1          | Digital Fifo overflow |
| 2          | Analog Fifo overflow  |
| 3          | analog fsm error      |
| 4          | digital fsm error     |
| 9          | bitslip 1 error       |
| 10         | bitslip 2 error       |

Note: indices 5-8 are not checked by PrintErrors.

The boolean array is populated by `FUN_01d21ac0` (ELF VA 0x1c21ac0), which
reads the last stats from the capture and unpacks overflow flags into the
boolean array. PrintErrors is called from two device-specific stop callbacks:

- `LogicGraduateDevice::StopCallback` (ELF VA 0x1c3f350, LogicGraduateDevice.cpp line 0x1a6)
  - Calls `FUN_01d21ac0(this)` to get overflow flags
  - Passes `bitslip_check = false` (param_2 = 0)
- `LogicProfessionalDevice::StopCallback` (ELF VA 0x1c42da0, LogicProfessionalDevice.cpp line 0x1b7)
  - Calls `FUN_01d21ac0(this)` to get overflow flags
  - Passes `bitslip_check = (*(int *)(this + 0xb0) == 7)` -- checks if device variant is Pro (type 7)


## Recovery Strategy

From `HandleWaveformData` decompilation:

1. Overflow is detected by comparing timestamps and overflow flags
2. When overflow occurs, the host logs the error via PrintErrors
3. The waveform data is still processed -- overflow blocks contain the
   recovered sample data that arrived during/after the overflow
4. The host does NOT abort capture on overflow -- it marks the gap and
   continues
5. Channel state tracking (`per-channel started/completed` flags) is
   maintained across overflow events

This is consistent with a streaming architecture where the host must handle
data loss gracefully rather than requiring retransmission (the device has no
on-board memory for replay).


## Wire Format Summary

```
+------------------------------------------------------+
| Stats Header (20 bytes, mandatory)                    |
|   packet_id(4) + timestamp_48(6) + flags(2)           |
|   + host_time(8)                                      |
+------------------------------------------------------+
| If flags bit 9: Overflow Metadata (16 bytes)          |
|   prev_overflow(8) + cur_overflow(8)                  |
+------------------------------------------------------+
| If flags bits 9+12: Overflow Blocks (N * 636 bytes)   |
|   For each block:                                     |
|     sample_data(512) + block_stats(124)               |
+------------------------------------------------------+
```


## References

- `re/notes/withstats-format.md` -- detailed stats parser analysis
- `re/ghidra/decompiled-overflow-handling.txt` -- raw decompiled C
- `re/ghidra/decompiled-waveform.txt` -- waveform persistence functions (Writer::PushBack, GetStats)
- `re/ghidra/decompiled-mso-overflow.txt` -- PrintErrors callers, GetStats consumers
- `crates/saleae-proto/src/capture_data.rs` -- Rust implementation
- `doc/protocol/digital-capture.md` -- overall capture protocol
