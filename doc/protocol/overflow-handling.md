# Overflow Handling in EP2 Stats Stream

Status: Partially confirmed (Ghidra decompilation of v2.4.40)
Source functions:
- `ParseMsoWaveformData` (ELF VA 0x2d41510, mso_data.cpp)
- `HandleWaveformData` (ELF VA 0x2d44b80, oscilloscope_instrument.cpp)
- `PrintErrors` (ELF VA 0x1c1fcb0, Logic2FpgaDevice.cpp)


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
0x00    8     Previous overflow flags (u64, LE)
0x08    8     Current overflow flags (u64, LE)
```

Overflow is detected when `cur_overflow != prev_overflow`.

### Confidence: Hypothesis

The 16-byte total and 2x8 entry structure are **High confidence** (direct
decompilation). The interpretation as prev/cur overflow counters is
**Hypothesis** -- the parser skips these bytes and saves the stream position
for a downstream consumer. The field names are inferred from the format
string `cur_overflow={}, prev_overflow={}` used in HandleWaveformData.


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

### Per-Block Stats (124 bytes)

The 124-byte per-block stats are not individually parsed by the stats
parser. Their internal layout is unknown. As a working hypothesis, the
first 20 bytes may share the same format as the mandatory StatsHeader, with
the remaining 104 bytes containing additional per-block metadata.

### Confidence: Hypothesis

The 124-byte size is **High confidence** (decompiled as `n_blocks * 0x7c`).
The internal structure is **Hypothesis** -- the parser skips the bytes
without field-level parsing.


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

The boolean array is derived from the overflow flags, but the exact
unpacking logic (which maps flag bits to array indices) is in a separate
function not yet decompiled.


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
- `crates/saleae-proto/src/capture_data.rs` -- Rust implementation
- `doc/protocol/digital-capture.md` -- overall capture protocol
