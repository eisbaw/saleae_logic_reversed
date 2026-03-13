# Analog Bit Inversion and Logic 8 Transform

Analysis date: 2026-03-13
Task: TASK-0201
Binary: libgraph_server_shared.so (Saleae Logic 2 v2.4.40)


## Summary

Two EP2 data format uncertainties resolved:

1. **Analog bit inversion (XOR 0xFF)** in `FUN_02eae7f0`: This is a streaming
   pipeline feature for polarity/edge tracking across chunks, NOT a data format
   difference. It is not relevant to raw deinterleaving.

2. **Logic 8 transform** in `FUN_01bf1ff0`: This is a lookup-table-based 8x8
   bit matrix transpose. The Logic 8 deinterleaver uses it to convert the
   FPGA's bit-interleaved format into per-channel byte streams.


## Analog Bit Inversion (FUN_02eae7f0, ELF VA 0x1eae7f0)

### What it does

`FUN_02eae7f0` is called by `ProcessChunk` for every analog-flagged channel.
It receives the deinterleaved data and the per-channel state struct.

1. Calls `FUN_02eadc90` (ELF VA 0x1eadc90, 2826 bytes) with the data.
2. If `FUN_02eadc90` returns `(result & 1) != 0`:
   - Iterates backwards through the chunk queue
   - Performs bitwise NOT (XOR 0xFFFFFFFF using SSE-vectorized loop) on all
     bytes of each chunk except the first
   - First chunk gets partial bit-level XOR (handles bit offset edge case)
3. Appends data to the output queue and calls `FUN_01bef940` (digital extract).

### What FUN_02eadc90 actually does

Despite being called for "analog" channels, `FUN_02eadc90` operates at the
individual bit level:

- Iterates byte-by-byte through the data
- Extracts 8 bits per byte, checking each bit's MSB against the previous bit
- Detects transitions (edges) between consecutive bits
- XOR-flips bits in the output at transition points
- Returns a boolean indicating whether any transitions occurred

This is an **edge/transition detector** operating on packed bit data, used for
polarity tracking across streaming chunks in the DisaggregatorNode graph pipeline.

### Why it does NOT affect our Rust deinterleaver

The XOR 0xFF inversion is part of the Saleae software's streaming graph node
infrastructure. It handles state that accumulates across multiple `ProcessChunk`
calls (chunk queues, bit position tracking, polarity state). Our Rust code
processes raw EP2 data in a single pass without chunk accumulation, so this
feature is not relevant.

The Pro deinterleaver (`FUN_01bf1190`) already outputs correctly sign-extended
int16 analog samples via the `psraw(4)` path. No additional inversion is needed.

### Confidence: **Confirmed** (not applicable to raw deinterleaving)


## Logic 8 Transform (FUN_01bf1ff0, ELF VA 0x1af1ff0)

### What it does

`FUN_01bf1ff0` performs a **lookup-table-based 8x8 bit matrix transpose** on
an 8-byte (64-bit) input word. It is called by the Logic 8 deinterleaver
(`FUN_01bf21b0`) for each 8-byte group.

### Lookup table construction

On first call (guarded by `DAT_03079b50`), builds a 256-entry table at
`DAT_03079350` (2048 bytes total, 8 bytes per entry). Each entry scatters
one input byte across 8 output bit positions:

```c
table[i] = (i << 49 & mask7) | (i << 42 & mask6) | (i << 35 & mask5) |
           (i << 28 & mask4) | (i << 21 & mask3) | (i << 14 & mask2) |
           (i << 7  & mask1) | (i       & mask0);
```

The shifts (0, 7, 14, 21, 28, 35, 42, 49) place each bit of the input byte
at 7-bit intervals in the 64-bit output, which is exactly the pattern needed
for an 8x8 bit matrix transpose.

### Lookup-based transpose

```c
result = table[byte7] * 2       |  // shift 1
         table[byte6] << 2      |  // shift 2
         table[byte5] << 3      |  // shift 3
         table[byte4] << 4      |  // shift 4
         table[byte3] << 5      |  // shift 5
         table[byte2] << 6      |  // shift 6
         table[byte0] << 7      |  // shift 7
         table[byte_top];          // shift 0
```

Each byte of the input is looked up in the table (which scatters its bits),
then the results are shifted by different amounts and OR'd together. The
combined shifts + table scatter implement the full transpose.

### Effect on Logic 8 deinterleaving

The FPGA packs Logic 8 data so that in each 8-byte group:
- **Input**: Bit N of byte M = sample M from channel N
- **After transpose**: Bit M of byte N = sample M from channel N

The deinterleaver then distributes: byte 0 -> channel 0, byte 1 -> channel 1,
etc. Each output byte contains 8 consecutive digital samples for that channel.

### Rust implementation

We implement the transpose with direct bit manipulation rather than a
precomputed LUT. The function `bit_transpose_8x8()` in `disaggregator.rs`
gathers bit N from all 8 input bytes to produce output byte N.

### Confidence: **Confirmed**


## Per-Channel State Layout (within DisaggregatorNode)

The per-channel state array starts at `this + 0x1b0` with stride 0x38 (56 bytes):

```
Offset  Size  Field
0x00    8     counter/position (passed to FUN_02eadc90 as param_2)
0x08    8     bit state (passed to FUN_02eadc90 as param_3 pointer)
0x10    8     (unknown)
0x18    8     chunk queue begin pointer
0x20    8     chunk queue end pointer
0x28    8     chunk queue capacity pointer
0x30    1     channel_type (0 = digital, nonzero = analog)
```

The channel_type flag at offset 0x30 within the per-channel struct determines
whether ProcessChunk routes the channel data through:
- `FUN_01bef940` (digital: simple pass-through)
- `FUN_02eae7f0` (analog: edge detection + conditional inversion + queue management)


## Key Virtual Addresses

| Function | ELF VA | Ghidra VA | Size | Role |
|----------|--------|-----------|------|------|
| FUN_02eae7f0 | 0x1eae7f0 | 0x2eae7f0 | 1004 | Analog channel extraction (streaming) |
| FUN_02eadc90 | 0x1eadc90 | 0x2eadc90 | 2826 | Bit-level edge/transition detector |
| FUN_01bf1ff0 | 0x1af1ff0 | 0x1bf1ff0 | 436 | 8x8 bit matrix transpose (LUT-based) |
| FUN_01bf21b0 | 0x1ae21b0 | 0x1bf21b0 | 2215 | Logic 8 deinterleaver |


## Impact on Codebase

- `disaggregator.rs`: Logic 8 NonPro deinterleaver rewritten to use bit transpose
  instead of incorrect byte-swap approach. `cycle_bytes()` now returns 8 for NonPro.
- `bit_transpose_8x8()`: New public function for 8x8 bit matrix transpose.
- No changes needed for Pro deinterleaving or analog extraction.
