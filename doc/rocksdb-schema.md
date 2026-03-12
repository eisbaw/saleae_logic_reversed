# RocksDB Schema for AnalogStore/DigitalStore

## Overview

Saleae Logic 2 uses **two separate persistence mechanisms**:

1. **RocksDB** -- runtime data storage during live capture. Data is stored in a
   local RocksDB database while samples stream in from the device. The database
   path is configured via a `/databasePath` JSON config key.

2. **Cereal binary archives** (.sal files) -- export/import format. When saving
   a capture, data is serialized via `SaleaeBinaryOutputArchive` (cereal) into
   `digital-N.bin` and `analog-N.bin` files inside a ZIP archive. This is what
   we implement in `crates/saleae-proto/src/sal.rs`.

RocksDB is **not** embedded inside `.sal` files. It serves as a temporary
high-performance store during capture sessions.

## Key Encoding: FoundationDB Tuple Layer

Keys are encoded using the **FoundationDB Tuple Layer** encoding (sometimes
called "ordered key encoding"). This is a well-documented format that preserves
sort order across composite keys in a byte-comparable way.

Reference: https://apple.github.io/foundationdb/data-modeling.html#tuples

### Type Tags

| Tag | Type | Size |
|-----|------|------|
| `0x00` | Null / nested tuple terminator | 0 bytes |
| `0x02` | Byte string | variable |
| `0x05` | Nested tuple start | variable |
| `0x0c`-`0x13` | Negative integer (8 to 1 bytes) | variable |
| `0x14` | Integer zero | 0 bytes |
| `0x15`-`0x1c` | Positive integer (1 to 8 bytes) | 1-8 bytes |
| `0x1d` | Positive integer (>8 bytes) | 1 + length + value |
| `0x21` | IEEE 754 double (8 bytes, sign-flipped) | 8 bytes |
| `0x26` | False | 0 bytes |
| `0x27` | True | 0 bytes |

### String Encoding (tag `0x02`)

```
0x02 | <bytes with 0x00 escaped as 0x00 0xFF> | 0x00 0x01
```

### Integer Encoding (tags `0x14`-`0x1d`)

- Zero: just `0x14`
- 1-byte positive: `0x15 <byte>`
- 8-byte positive: `0x1c <8 bytes big-endian>`
- >8 byte positive: `0x1d <length_byte> <big-endian bytes>`
- Negative integers use tags `0x0c`-`0x13` with complement encoding

### Double Encoding (tag `0x21`)

```
0x21 | XOR(value, mask) big-endian
  where mask = 0xFFFFFFFFFFFFFFFF if value >= 0
              (only sign bit flipped) otherwise
```

This makes IEEE 754 doubles sort correctly in byte order.

### GraphTime i128 Encoding

GraphTime (i128 nanoseconds) is encoded as a **nested tuple** containing
two integer elements:

```
0x05                    -- tuple start
  <high_word_encoded>   -- upper 64 bits as variable-length integer
  <low_word_encoded>    -- lower 64 bits as variable-length integer
0x00                    -- tuple end
```

Where each word uses the standard FDB integer encoding (`0x14` for zero,
`0x15`-`0x1d` for positives, `0x0c`-`0x13` for negatives).

## Key Structure

The RocksDB key for a data record is a tuple of 4 components:

```
<tag_byte> | <channel_name_string> | <sequence_u64> | <timestamp_graphtime>
```

Where:
- **tag_byte** (`0x14`/`0x15`): Record type discriminant
  - Tag value `1`: key prefix (seek marker)
  - Tag value `2`: data record
  - Tag value `3`: another key prefix type
  - Tag value `0x0e` (14): yet another namespace
- **channel_name_string**: Channel identifier, FDB byte-string encoded
- **sequence_u64**: Data chunk sequence number, FDB double-encoded (8 bytes)
- **timestamp_graphtime**: GraphTime as nested tuple of two integers

## Key Namespaces

The DB init function (`FUN_01d89d50`, ELF 0x1c89d50) creates 6 key prefixes:

| Offset in struct | Tag | Suffix | Purpose |
|-----------------|-----|--------|---------|
| `[1..4]` + `[4]` | 1 | - | Seek prefix |
| `[5..8]` + `[8]` | 1 | 0xFF | Upper bound |
| `[9..12]` + `[12]` | 3 | - | Seek prefix |
| `[13..16]` + `[16]` | 3 | 0xFF | Upper bound |
| `[17..20]` + `[20]` | 14 (0x0e) | - | Seek prefix |
| `[21..24]` + `[24]` | 14 (0x0e) | 0xFF | Upper bound |

Each namespace has a seek prefix (for iteration start) and an upper bound
(for iteration end), enabling efficient range scans.

## Value Format

Values are **raw byte blobs** containing the serialized sample data. The
exact value format depends on the store type:

- **DigitalStore**: Transition-encoded digital samples (similar to the
  cereal format in `digital-N.bin`)
- **AnalogStore**: Packed waveform data (`PackedWaveformData` chunks)
- **WaveformStore** (MSO oscilloscope): Raw ADC waveform data

The value serialization is done by helper functions before the RocksDB
Put() call. The serialized bytes include the sample data payload but
not the key components (those are in the key).

## Column Families

The database uses the **default column family only**. There is no evidence
of additional column families in the DB::Open or Put calls.

## Implications for GraphTime::to_be_bytes()

Our current `GraphTime::to_be_bytes()` implementation uses a simple
`i128 XOR (1 << 127)` encoding. This is **different** from what the
binary uses.

The binary encodes GraphTime as a **FoundationDB nested tuple** containing
two variable-length integers (high and low 64-bit words). This means:

1. Our `to_be_bytes()` produces a fixed 16-byte encoding that preserves
   sort order. This is correct for our own use cases.

2. The binary's encoding is variable-length (2-20+ bytes for the timestamp
   component, wrapped in a tuple). This is needed for the FDB tuple layer's
   composite key ordering.

3. **For .sal file interop, our encoding is irrelevant** -- .sal files use
   cereal archives, not RocksDB keys. Our `to_be_bytes()` would only matter
   if we were reading/writing Saleae's RocksDB database directly.

4. If we ever need to read Saleae's RocksDB database (e.g., recovering
   unsaved captures), we would need to implement the full FDB tuple layer
   decoder.

## Source References

- Key encoding functions: ELF 0x1c9a280 (tag), 0x1c99cb0 (string),
  0x1c9b2d0 (u64/double), 0x1b9a920 (GraphTime tuple)
- GraphTime sub-encoders: ELF 0x1b9aad0 (high word), 0x1c9a530 (low word)
- DB init: ELF 0x1c89d50
- Data write (analog): ELF 0x1c991b0
- Data write (digital): ELF 0x1c994b0
- DB flush: ELF 0x1c8b2c0
- Source path: `/workspace/monorepo/graph-io/core/saleae_base/include/serialize_data.h`
