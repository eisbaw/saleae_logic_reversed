---
id: TASK-0120
title: RE AnalogStore and DigitalStore RocksDB persistence format
status: Done
assignee: []
created_date: '2026-03-11 07:55'
updated_date: '2026-03-12 04:33'
labels:
  - re
  - ghidra
  - data-format
dependencies: []
references:
  - crates/saleae-proto/src/graph_time.rs
  - re/notes/waveform-persistence.md
  - doc/rocksdb-schema.md
  - re/ghidra/scripts/DecompileRocksDBStores.java
  - re/ghidra/scripts/DecompileRocksDBKeys.java
  - re/ghidra/scripts/DecompileRocksDBKeys2.java
  - re/ghidra/decompiled-rocksdb-stores.txt
  - re/ghidra/decompiled-rocksdb-keys.txt
  - re/ghidra/decompiled-rocksdb-keys2.txt
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The binary uses RocksDB v8.0.0 for capture data persistence via AnalogStore and DigitalStore. Understanding the key/value schema would enable:
- Reading .sal files that use this format
- Understanding how GraphTime keys are ordered (we implemented sign-bit XOR but haven't validated)
- Potential interop with Saleae's capture database

Investigation:
1. Find strings referencing analog_store and digital_store source paths
2. Decompile the key construction functions (likely use GraphTime for keys)
3. Document key format, value format, column families
4. Validate our GraphTime::to_be_bytes() key ordering against the actual implementation

Low priority -- only needed for .sal interop, which is already partially handled by sal.rs.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
## Findings

### Key Discovery: FoundationDB Tuple Layer Encoding

The RocksDB key encoding uses the **FoundationDB Tuple Layer** format -- a well-documented ordered key encoding that preserves sort order across composite keys in byte-comparable form. This was identified by matching the type tag bytes (0x02=string, 0x05=nested tuple, 0x14=zero, 0x15-0x1c=positive ints, 0x21=double) against the FDB spec.

### Two Separate Persistence Mechanisms

1. **RocksDB** -- runtime storage during live capture (temporary, not in .sal files)
2. **Cereal binary archives** -- .sal file persistence (digital-N.bin, analog-N.bin in ZIP)

RocksDB is NOT embedded in .sal files. It serves as a high-performance buffer during capture sessions only.

### RocksDB Key Structure

Each key is a 4-component FDB tuple:
```
<tag_byte> | <channel_name_string> | <sequence_u64> | <timestamp_graphtime>
```

- Tag: record type (1=seek prefix, 2=data record, 3=another prefix, 14=another namespace)
- Channel name: FDB byte-string encoded (0x02 prefix, 0x00->0x00 0xFF escaping, 0x00 0x01 terminator)
- Sequence: u64 encoded as FDB double (0x21 prefix, 8 bytes sign-flipped big-endian)
- Timestamp: GraphTime i128 as nested FDB tuple (0x05 prefix, high word int, low word int, 0x00 terminator)

### GraphTime::to_be_bytes() Assessment

Our implementation uses `i128 XOR (1 << 127)` for a fixed 16-byte encoding. The binary uses FDB's variable-length integer encoding in a nested tuple. Both preserve sort order, but they produce different byte sequences. This difference is irrelevant for .sal interop since .sal files use cereal, not RocksDB keys. Our encoding is correct for our own use cases.

### Key Functions Decompiled

- DB init: ELF 0x1c89d50 (creates 6 key prefixes across 3 namespaces)
- Tag encoder: ELF 0x1c9a280
- String encoder: ELF 0x1c99cb0
- u64/double encoder: ELF 0x1c9b2d0
- GraphTime tuple encoder: ELF 0x1b9a920
- GraphTime sub-encoders: ELF 0x1b9aad0 (high word), 0x1c9a530 (low word)
- Data write functions: ELF 0x1c991b0, 0x1c994b0
- Source: /workspace/monorepo/graph-io/core/saleae_base/include/serialize_data.h

### Deliverables

- `doc/rocksdb-schema.md` -- full schema documentation
- 3 Ghidra scripts for reproducible decompilation
- 114 decompiled functions covering the entire RocksDB persistence layer
<!-- SECTION:FINAL_SUMMARY:END -->
