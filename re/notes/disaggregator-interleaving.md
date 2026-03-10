# Disaggregator Interleaving: Complete Analysis

Analysis date: 2026-03-10
Binary: libgraph_server_shared.so (48MB, stripped, ELF 64-bit x86-64)
Source: Saleae Logic 2 v2.4.40
Task: TASK-0069
Supersedes hypotheses in: data-interleaving.md (TASK-0035, TASK-0038)


## Summary

The DisaggregatorNode receives raw interleaved USB data from EP2 IN and
splits it into per-channel streams. Five decompiled rounds revealed:

1. The settings struct at offset `this+0x120` controls everything
2. A device-type enum (0-7) dispatches to four device-specific
   deinterleaving functions
3. The Pro devices (types 4/5/6) use a mixed digital+analog interleave
   cycle with packed 12-bit ADC encoding
4. Digital data undergoes endian byte-swapping during extraction
5. Analog data is packed 12-bit (3 bytes per 2 samples or 6 bytes per
   4 samples), sign-extended via arithmetic right shift


## DisaggregatorSettingsData Struct Layout (0xa0 bytes)

Constructed by `FillDisaggregatorSettings` (ELF VA 0x1cdb730).
Source data comes from `LogicDeviceNode + 0x3b0`.

```
Offset  Size  Type         Field
0x00    8     void*        vtable_1 (PTR_FUN_02f452b0)
0x08    8     u64          ref_count_strong (init 0)
0x10    8     u64          ref_count_weak (init 0)
0x18    8     void*        vtable_2 (PTR_FUN_02f44c20, inner object)
0x20    --    (inner object fields start here)
```

The inner object at +0x18 is what the deinterleaving functions receive
as their `param_2`. The deinterleaving functions access fields relative
to `settings + 0x20`, i.e., relative to the inner object start.

### Settings Inner Object (accessed via settings + 0x20)

When ProcessChunk calls `FUN_01bf1190(param_2, lVar11 + 0x20, local_230)`,
the second argument is `settings + 0x20`. All offsets below are relative
to this base.

```
Offset  Size  Type      Field (deduced name)
0x00    8     u64       unknown_0x00
0x08    8     u64/ptr   channel_enable_array_ptr (type2 uses this)
0x10    4     int       digital_ratio (digital packets per cycle)
0x14    4     int       digital_packet_size (bytes per digital packet, 2 or 4)
0x18    4     int       analog_group_size (analog samples per ADC group)
0x1c    4     int       analog_bytes_per_sample (raw packed bytes, e.g. 3)
0x20-0x118  256  long[32]  digital_channel_enables (1=enabled, per channel)
0x120-0x138  --  long[32]  analog_channel_enables (continues from digital)
0x140   4     int       device_validation_code (0,1,2,3 maps to type 1,2,3,4-6)
```

### Device Type Enum (at settings + 0x18, before inner object)

```
Value  Device                 Deinterleave function   Validation code
0      Unknown                ERROR (throws)          --
1      Logic 8                FUN_01bf21b0            0
2      Logic 16               FUN_01bf37e0            1
3      Logic 4                FUN_01bf2a90            2
4      Logic Pro 8            FUN_01bf1190            3
5      Logic Pro 16           FUN_01bf1190            3
6      Logic Pro (variant)    FUN_01bf1190            3
7      MSO                    ERROR (throws)          --
```

Note: All Pro devices (types 4, 5, 6) use the same deinterleaving
function FUN_01bf1190. The function dynamically counts enabled channels
from the settings arrays.


## Per-Device Deinterleaving

### Logic 8 (Type 1, FUN_01bf21b0, ELF VA 0x1ae21b0)

**Error string**: `"Logic data length must be a multiple of 8"`

Logic 8 produces **digital-only interleaved data** (8 channels). There
is no analog interleaving for Logic 8 -- it is digital-only at this
deinterleaving stage.

**Algorithm**:
1. Validate: data length must be divisible by 8
2. Allocate 8 output buffers (one per channel), each of size `data_len / 8`
3. For each 8-byte group in the input:
   - Call `FUN_01bf1ff0(input_word)` to transform/unpack the 8-byte word
   - Distribute one byte to each of the 8 channel output buffers
   - Byte 0 (LSB) -> channel 0, byte 1 -> channel 1, ..., byte 7 -> channel 7

This means each byte in the output stream contains 8 digital samples
(1 bit per sample, 8 samples per byte). The input has the channels
interleaved at the byte level: for every 8 bytes of input, each channel
gets 1 byte.

When data is empty (__n == 0), it still allocates all 8 output buffers
(empty vectors).


### Logic 16 (Type 2, FUN_01bf37e0, ELF VA 0x1ae37e0)

**Error string**: `"Logic16 data length must be multiple of 2*num_chanels [bytes]"`
(note: original typo "chanels" is in the binary)

**Algorithm**:
1. Count enabled channels from `param_2 + 0x08` through `param_2 + 0x80`
   (16 slots, 8 bytes each, checking if value == 1)
2. Validate: data length must be divisible by `2 * num_enabled_channels`
3. Allocate one output buffer per enabled channel, each of size
   `data_len / num_enabled_channels`
4. For each 2-byte group per channel:
   - Read 2 bytes: `byte0 = *src`, `byte1 = *(src+1)`
   - Write to output in **swapped order**: `*dst++ = byte1`, `*dst++ = byte0`
   - This is a 16-bit little-endian to big-endian byte swap (or vice versa)

Each output byte still contains packed digital samples (8 per byte).
The byte-swap corrects the FPGA's bit ordering within 16-bit bus words.


### Logic 4 (Type 3, FUN_01bf2a90, ELF VA 0x1ae2a90)

**Error string**: `"Logic4 data size must be a multiple of 2 [bytes]"`

Logic 4 has a more complex deinterleaving because it supports an
optional **shift mode** (field at `param_2 + 0x08`).

**Algorithm**:
1. If shift_mode byte is nonzero, set initial output size to 0
   (shift mode adjusts the interpretation)
2. Compute `shifted_size = data_len << shift_bits` where
   `shift_bits = param_2[0x08] & 0x1f`
3. Validate: shifted_size must be a multiple of 8
4. Allocate output buffers based on shifted_size
5. Process data with conditional byte-swapping similar to Logic 16

The shift mode likely handles the Logic 4's simpler bus width (fewer
channels mapped to a narrower GPIF bus).


### Pro Devices (Types 4/5/6, FUN_01bf1190, ELF VA 0x1ae1190)

**Error string**: `"Logic8 data length must be multiple of the 'full cycle' size"`
(Note: error string says "Logic8" but this function handles all Pro
devices. The string is misleading -- likely a copy-paste artifact.)

This is the most complex deinterleaver, handling mixed digital+analog
data. It is the primary function for Logic Pro 8 and Logic Pro 16.

**Key computations**:

```
digital_group_bytes = settings.digital_ratio * settings.digital_packet_size
    (param_2 + 0x10) * (param_2 + 0x14)

num_enabled_analog = count of slots where value == 1 in:
    param_2[0x20..0x118] (first 16 slots, 8 bytes each)
    param_2[0x80..0x118] (next 16 slots, continuation)
    Total: 32 slots checked

analog_group_bytes = settings.analog_bytes_per_sample
                   * settings.analog_group_size
                   * num_enabled_analog
    (param_2 + 0x1c) * (param_2 + 0x18) * num_analog

full_cycle_size = digital_group_bytes + analog_group_bytes
```

The data MUST be a multiple of `full_cycle_size`.

**Interleave cycle structure** (one full cycle):

```
+------ digital_group_bytes ------+---- analog_group_bytes ----+
| D0 | D1 | ... | DN (repeated    | A0  | A1  | ... | AM       |
|    digital_ratio times)         | (packed 12-bit samples)    |
+---------------------------------+----------------------------+
```

Within one full cycle, the function processes:
1. **Digital data first** -- `digital_ratio` groups of digital samples
2. **Analog data second** -- `analog_group_size` groups of analog samples

**Digital extraction (Pro)**:

Two code paths based on `digital_packet_size`:

- **digital_packet_size == 2** (16-bit GPIF mode):
  - For each digital group iteration (digital_ratio times):
    - For each enabled digital channel:
      - Read 2 bytes from input
      - Extract byte at offset +1 -> write to channel output
      - Extract byte at offset +0 -> write to channel output
      - (Byte swap: big-endian to little-endian, or FPGA-to-host correction)

- **digital_packet_size == 4** (32-bit GPIF mode):
  - For each digital group iteration:
    - For each enabled digital channel:
      - Read 4 bytes (uint32_t) from input
      - Full 32-bit endian swap: `result = bswap32(input)`
        Specifically: `(x >> 24) | ((x & 0xff0000) >> 8) | ((x & 0xff00) << 8) | (x << 24)`
      - Write swapped 4 bytes to channel output

**Analog extraction (Pro)**:

Two code paths based on `digital_packet_size` (which correlates with
GPIF bus width and ADC packing):

- **digital_packet_size == 2** (4 samples in 6 bytes):
  - Read 6 bytes from input per enabled analog channel
  - Unpack into 4 int16_t samples:
    ```
    sample[0] = sign_extend_12(bytes[0:2] bits[0:11])
    sample[1] = sign_extend_12(bytes[1:2] bits[4:15])
    sample[2] = sign_extend_12(bytes[2:4] bits[20:31])
    sample[3] = sign_extend_12(bytes[4:6] bits[32:43])
    ```
  - Specifically from decompilation:
    ```c
    // Read 6 bytes as two values
    uVar20 = *(uint32_t*)src;       // bytes 0-3
    sVar5 = *(int16_t*)(src + 4);   // bytes 4-5

    // Unpack 4 x 12-bit samples, sign-extended to 16-bit
    out[0] = (int16_t)((uVar20 << 4) & 0xFFF0) >> 4;   // bits[0:11]
    out[1] = (int16_t)((uVar20 >> 8) & 0xFFF0) >> 4;    // bits[8:19]
    out[2] = (int16_t)((uVar19 >> 20) & 0xFFF0) >> 4;   // bits[20:31]
    out[3] = (int16_t)(sVar5 & 0xFFF0) >> 4;             // bits[32:43]
    ```
  - Advance source pointer by 6 bytes

- **digital_packet_size != 2** (8 samples in 12 bytes, SSE optimized):
  - Read 12 bytes from input per enabled analog channel
  - Unpack into 8 int16_t samples using SSE `psraw` (packed shift right
    arithmetic word) by 4:
    ```c
    // Read 12 bytes
    uVar20 = *(uint64_t*)src;       // bytes 0-7
    uVar19 = *(uint64_t*)(src+4);   // bytes 4-11 (overlapping read)

    // Build SSE vector of 8 x 16-bit values
    vec[0] = (int16_t)(uVar20 << 4);     // bits[0:11]
    vec[1] = (int16_t)(uVar20 >> 8);     // bits[8:19]
    vec[2] = (int16_t)(uVar20 >> 20);    // bits[20:31]
    vec[3] = (int16_t)(uVar20 >> 32);    // bits[32:43]
    vec[4] = (int16_t)(uVar19 >> 12);    // bits[44:55]
    vec[5] = (int16_t)(uVar19 >> 24);    // bits[56:67]
    vec[6] = (int16_t)(uVar19 >> 36);    // bits[68:79]
    vec[7] = (int16_t)(uVar19 >> 48);    // bits[80:91]

    // Arithmetic right shift by 4 to sign-extend from 12-bit to 16-bit
    result = psraw(vec, 4);
    ```
  - Advance source pointer by 12 bytes
  - This writes 16 bytes (8 x int16_t) to the output buffer per channel

**Analog sign extension**: The `psraw(x, 4)` instruction performs
packed signed 16-bit arithmetic right shift by 4. This converts a
left-aligned 12-bit value in a 16-bit container to a properly
sign-extended 12-bit value. A 12-bit ADC value of -2048 (0x800) would
be stored as 0x8000 after the left-shift packing, then `>> 4` gives
0xF800 which is -2048 in int16_t.


## Channel Enable Arrays in Settings

The settings struct contains two enable arrays that determine which
channels participate in deinterleaving:

### Digital Channel Enable (Pro devices)

Located at settings inner +0x20, 32 slots of 8 bytes each.

For Pro devices, the function checks `param_2 + 0xa0 + offset` (where
offset goes from -0x80 to 0 in steps of 8) for digital enables, and
`param_2 + 0x120 + offset` for analog enables. This maps to:

```
Digital enables: settings+0x20 through settings+0x118 (32 x 8 bytes)
Analog enables:  settings+0x120 through settings+0x218 (32 x 8 bytes? -- overlaps next field)
```

Wait -- the Pro function uses a loop from `lVar26 = -0x80` to `0` with
step 8 (16 iterations). It checks:
- `param_2 + 0xa0 + lVar26` for digital (lVar26 goes -0x80...-0x08, so
  offsets 0x20..0x98)
- `param_2 + 0x120 + lVar26` for analog (offsets 0xa0..0x118)

So for **Pro devices** (up to 16 channels):
```
Digital enables: param_2[0x20..0x98]  = 16 slots x 8 bytes = offsets 0x20, 0x28, ..., 0x98
Analog enables:  param_2[0xa0..0x118] = 16 slots x 8 bytes = offsets 0xa0, 0xa8, ..., 0x118
```

Each slot is a `long` (8 bytes), value `1` means enabled.

### Digital Channel Enable (Logic 16)

For Logic 16, the function checks `param_2 + 0x08` through
`param_2 + 0x80` (16 slots, same pattern but offset differently
because the settings struct for type 2 has a different layout).


## DeviceCalculateMixRate (ELF VA 0x1c19fa0)

This function computes the interleaving ratios for mixed-mode captures.
Source: `DeviceHelpers.cpp`.

### Valid GPIF Width / ADC Bit Combinations

```
GPIF Width   ADC Bits   Samples/Group   Bytes/Analog   Notes
16 (0x10)    10         8               5              10-bit ADC, 5 bytes per 8 samples
16 (0x10)    12         4               3              12-bit ADC, 3 bytes per 4 samples
32 (0x20)    12         8               3              12-bit ADC, 3 bytes per 8 samples
other        other      --              --             ERROR: "No valid analog and gpif width combo"
```

### Ratio Computation

The function uses GCD (Euclidean algorithm at ELF VA 0x1c47140) to
compute the digital-to-analog sample ratio:

```
gcd = GCD(digital_rate, analog_rate)
digital_packets = digital_rate / gcd
analog_packets = analog_rate / gcd
```

These ratios determine how many digital sample groups and analog sample
groups appear in each interleave cycle.

### Output Parameters

The function outputs four values:
1. `digital_packets` -- number of digital groups per cycle
2. `analog_packets` -- number of analog groups per cycle
3. `digital_ratio` -- some derived digital count
4. `analog_ratio` -- some derived analog count

### Example: Logic Pro 8, 100 MHz digital + 50 MHz analog

```
GCD(100M, 50M) = 50M
digital_packets = 100M / 50M = 2
analog_packets = 50M / 50M = 1

For 12-bit ADC with 32-bit GPIF:
  samples_per_group = 8
  bytes_per_analog_group = 3 (per sample, packed)

With 4 digital + 4 analog channels:
  digital_group_bytes = 2 * 4 = 8 bytes (2 digital packets * 4 bytes/pkt)
  analog_group_bytes = 3 * 8 * 4 = 96 bytes (3 bytes * 8 samples * 4 channels)
  full_cycle_size = 8 + 96 = 104 bytes
```


## ProcessChunk: The Dispatch Function (ELF VA 0x1aea5d0)

`FUN_01bea5d0` (2097 bytes) is the main processing function. It:

1. Initializes a 0x200-byte (512 bytes) array of 64 longs (32 channel
   slots, 16 bytes each = pointer + refcount)
2. Reads settings from `this->0x120`
3. Switches on device type `settings->0x18`
4. Calls the device-specific deinterleaver with `(data, settings+0x20, output_array)`
5. Iterates 16 channels (0 to 0x10):
   - Per-channel state at `this + 0x1e0 + channel * 0x38`
   - Flag at `this->0x1e0 + channel*0x38` discriminates digital vs analog:
     - `flag == '\0'` (0): digital channel -> calls FUN_01bef940
     - `flag != '\0'`: analog channel -> calls FUN_02eae7f0
   - Emits output via FUN_02d910f0 (pipe emit)
   - Applies glitch filters if configured (via tree at `this->0x140`)
6. Calls stats_setup (FUN_01beda50) and finalize (FUN_02d91580) at the end


## Data Handler: Settings Gate (ELF VA 0x1ae9a70)

`FUN_01be9a70` is the entry point for incoming data:

1. Sets `this->0x134 = 1` (has_received_data flag)
2. Checks `this->0x130 == 1` (has_settings flag)
3. If settings received: queues data at `this+0x150`, processes each chunk
4. If no settings: throws
   `"DisaggregatorNode requires DisaggregatorSettingsData before any other data"`


## ContractReadData (ELF VA 0x1c3bad0)

Operates on the raw USB buffer BEFORE disaggregation. Switches on
`this->0x19c` (the LowSpeedDivider / GPIF logical width ratio):

```
Value  Action
1      No-op (data width matches bus width)
2      Copy every 2nd byte (remove 1 padding byte per data byte)
4      Copy every 4th byte (remove 3 padding bytes per data byte)
8      Copy every 8th byte (remove 7 padding bytes per data byte)
other  ERROR: "_LowSpeedDivider is not a valid size"
```

Buffer size stored at `this->0x1a0`. When divider > 1, the function
iterates through the buffer, keeping every Nth byte and discarding
the rest. The output overwrites the input buffer in-place.

This confirms that ContractReadData runs BEFORE the disaggregator and
handles bus-width padding. The disaggregator then operates on the
compacted data.


## Analog Channel Extraction (ELF VA 0x1eae7f0)

`FUN_02eae7f0` handles analog channel data AFTER deinterleaving. It:

1. Calls `FUN_02eadc90` to check some condition on the data
2. May perform bit inversion (XOR with 0xFF...FF) on the analog data
   - This is a bitwise NOT operation on the raw bytes
   - Uses SSE-vectorized code for large blocks (32+ bytes at a time)
   - This likely handles ADC encoding where some devices use offset
     binary (unsigned) that needs conversion to two's complement (signed)
3. Handles a partial-sample edge case for the first chunk (bit-level
   operations with `XOR 0x80 >> bit_offset`)
4. Appends the data chunk to the output queue
5. Calls FUN_01bef940 (digital extract) internally to manage the queue
   data structure (reuses the same shared_ptr vector management)


## DisaggregatorNode State Layout

```
Offset    Size    Field
0x120     8       ptr to DisaggregatorSettingsData
0x130     4       has_settings (int, 1 = settings received)
0x134     4       has_received_data (int, 1 = data received)
0x140     8       ptr to glitch filter map (std::map)
0x150     --      data queue
0x198     8       ptr to stats tree (std::map or similar)
0x1A0     8       stats tree sentinel
0x1E0+N*0x38  --  per-channel state array (N = 0..15)
  +0x00   1       channel_type (0 = digital, nonzero = analog)
  +0x08   --      (other per-channel fields)
  ...
```

Per-channel stride is 0x38 (56 bytes).

### Per-Channel State in Settings (from outer loop in FUN_01be9570)

The outer loop accesses `settings + 0x348 + channel * 0x18` and
`settings + 0x358 + channel * 0x18`, giving a stride of 0x18 (24 bytes)
per channel entry in the settings.


## Byte Layout Examples

### Example 1: Logic Pro 8, 8 Digital Only (100 MHz)

GPIF width: 32-bit, digital_packet_size = 4

```
Input stream (per cycle):
  [B0 B1 B2 B3] [B4 B5 B6 B7] ... (8 channels * 4 bytes = 32 bytes)

After deinterleaving + byte swap:
  Channel 0: [B3 B2 B1 B0] (bswap32)
  Channel 1: [B7 B6 B5 B4] (bswap32)
  ...
  Channel 7: [B31 B30 B29 B28]

Each output byte = 8 digital samples (1 bit per sample, packed)
```

### Example 2: Logic Pro 8, 4 Digital + 4 Analog (Digital 100 MHz, Analog 50 MHz)

With 12-bit ADC, 32-bit GPIF:
- samples_per_group = 8
- bytes_per_analog = 3 (per sample pair, packed 12-bit)
- digital_ratio = 2, analog_ratio = 1

```
One interleave cycle:
  === Digital group 1 ===
  [D0: 4 bytes] [D1: 4 bytes] [D2: 4 bytes] [D3: 4 bytes]  (16 bytes)
  === Digital group 2 ===
  [D0: 4 bytes] [D1: 4 bytes] [D2: 4 bytes] [D3: 4 bytes]  (16 bytes)
  === Analog group (8 samples per channel, 12 bytes each) ===
  [A0: 12 bytes] [A1: 12 bytes] [A2: 12 bytes] [A3: 12 bytes] (48 bytes)

Total cycle: 16 + 16 + 48 = 80 bytes
```

Each analog 12-byte block unpacks to 8 x int16_t samples via SSE:
```
Bytes:  [b0 b1 b2 b3 b4 b5 b6 b7 b8 b9 b10 b11]
Sample 0: sign_extend_12( (b1 b0) << 4 >> 4 )   bits [0:11]
Sample 1: sign_extend_12( (b2 b1) >> 4  >> 4 )   bits [8:19]
...
Sample 7: sign_extend_12( (b11 b10) >> 0 >> 4 )  bits [80:91]
```

### Example 3: Logic Pro 8, 4 Digital + 4 Analog, 16-bit GPIF

With 12-bit ADC, 16-bit GPIF:
- samples_per_group = 4
- bytes_per_analog = 3 (per sample pair, packed 12-bit)
- digital_packet_size = 2

```
One interleave cycle:
  === Digital groups ===
  [D0: 2 bytes] [D1: 2 bytes] [D2: 2 bytes] [D3: 2 bytes]
  (repeated digital_ratio times)
  === Analog group (4 samples per channel, 6 bytes each) ===
  [A0: 6 bytes] [A1: 6 bytes] [A2: 6 bytes] [A3: 6 bytes]
```

Each analog 6-byte block unpacks to 4 x int16_t samples:
```
Bytes: [b0 b1 b2 b3 b4 b5]
Sample 0: sign_extend_12( (u32 << 4) & 0xFFF0 >> 4 )
Sample 1: sign_extend_12( (u32 >> 8) & 0xFFF0 >> 4 )
Sample 2: sign_extend_12( (u32 >> 20) & 0xFFF0 >> 4 )
Sample 3: sign_extend_12( (i16) & 0xFFF0 >> 4 )
```


## Key Virtual Addresses

| Function | ELF VA | Ghidra VA | Size |
|----------|--------|-----------|------|
| DisaggregatorNode_data_handler | 0x1ae9a70 | 0x1be9a70 | ~100 |
| DisaggregatorNode_ProcessChunk | 0x1aea5d0 | 0x1bea5d0 | 2097 |
| Deinterleave_Pro (types 4,5,6) | 0x1ae1190 | 0x1bf1190 | 2601 |
| Deinterleave_Logic8 (type 1) | 0x1ae21b0 | 0x1bf21b0 | 2215 |
| Deinterleave_Logic4 (type 3) | 0x1ae2a90 | 0x1bf2a90 | 2754 |
| Deinterleave_Logic16 (type 2) | 0x1ae37e0 | 0x1bf37e0 | 1069 |
| Extract_digital | 0x1aef940 | 0x1bef940 | 379 |
| Extract_analog | 0x1eae7f0 | 0x2eae7f0 | 1004 |
| ContractReadData | 0x1c3bad0 | 0x1d3bad0 | 383 |
| ExpandWriteData | 0x1c3bef0 | 0x1d3bef0 | 338 |
| GetDisaggregatorSettings | 0x1cd6e80 | 0x1ce6e80 | 81 |
| FillDisaggregatorSettings | 0x1cdb730 | 0x1ceb730 | 147 |
| DeviceCalculateMixRate | 0x1c19fa0 | 0x1d19fa0 | 397 |
| GCD helper | 0x1c47140 | 0x1d47140 | 46 |
| Disaggregator_stats_setup | 0x1aeda50 | 0x1beda50 | -- |
| Disaggregator_finalize | 0x1c91580 | 0x2d91580 | -- |


## Confidence Assessment

| Finding | Confidence | Source |
|---------|-----------|--------|
| Device type enum (0-7) dispatches deinterleaving | **Confirmed** | Decompilation of ProcessChunk |
| Pro devices use same deinterleave function | **Confirmed** | Switch cases 4,5,6 -> FUN_01bf1190 |
| 12-bit ADC packed: 4 samples in 6 bytes (16-bit GPIF) | **Confirmed** | Decompilation of FUN_01bf1190 |
| 12-bit ADC packed: 8 samples in 12 bytes (32-bit GPIF) | **Confirmed** | Decompilation of FUN_01bf1190 + SSE psraw |
| Sign extension via arithmetic right shift by 4 | **Confirmed** | psraw(vec, 4) in decompilation |
| Digital byte-swap during extraction | **Confirmed** | bswap32 pattern in both 16/32-bit paths |
| ContractReadData removes every Nth byte for divider N | **Confirmed** | Decompilation of FUN_01d3bad0 |
| Settings must arrive before data | **Confirmed** | Error string + flag check |
| Logic 8: 8 bytes per cycle, 1 byte per channel | **Confirmed** | Decompilation of FUN_01bf21b0 |
| Logic 16: 2*num_channels bytes per cycle | **Confirmed** | Decompilation of FUN_01bf37e0 |
| Analog data may undergo bit inversion | **High** | XOR 0xFF pattern in FUN_02eae7f0 |
| Settings inner object field layout | **High** | Consistent across deinterleave functions |
| Channel enable arrays are 32x8 byte slots | **High** | Pro function iterates 32 slots |
| Full cycle = digital_groups + analog_groups | **High** | Division + modulo check in Pro function |


## Open Questions

1. **FUN_01bf1ff0**: Called by Logic 8 deinterleaver to transform 8-byte
   words. Is this a simple bit reordering, byte swap, or something more?
   Needs decompilation.

2. **Analog bit inversion**: Under what conditions does FUN_02eae7f0
   perform the bitwise NOT? Is this offset-binary to two's-complement
   conversion for certain ADC types?

3. **Logic 4 shift mode**: What does the shift field at param_2+0x08
   do? It shifts the data length left by some bits. Is this related to
   the Logic 4's lower channel count and narrower bus?

4. **Settings handler**: Which function sets `this->0x130 = 1` when
   DisaggregatorSettingsData arrives? This is the pipe handler for the
   settings data type.

5. **Exact mapping of GPIF width to digital_packet_size**: Is
   digital_packet_size always equal to GPIF_width/8? (16-bit -> 2,
   32-bit -> 4)

6. **Validation of interleave examples against pcap**: The byte layout
   examples above are derived from decompilation but should be validated
   against actual USB captures.
