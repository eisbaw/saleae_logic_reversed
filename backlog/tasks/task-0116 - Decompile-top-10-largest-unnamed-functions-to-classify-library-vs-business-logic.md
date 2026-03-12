---
id: TASK-0116
title: >-
  Decompile top 10 largest unnamed functions to classify library vs business
  logic
status: Done
assignee: []
created_date: '2026-03-11 07:54'
updated_date: '2026-03-12 04:14'
labels:
  - re
  - ghidra
  - triage
dependencies: []
references:
  - re/ghidra/scripts/LargeFunctions.java
  - re/ghidra/scripts/ClassifyLargeFunctions.java
  - re/ghidra/decompiled-top5-large-funcs.txt
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The binary has 7,032 unnamed functions over 500 bytes. The top 10 range from 15k to 45k bytes each. Decompiling these will determine whether they're RocksDB/library internals (ignore) or Saleae business logic (investigate).

Top 5 ELF VAs to check:
1. 0x02a3a820 (44,952 bytes)
2. 0x01f6d820 (41,864 bytes)
3. 0x01f379c0 (34,211 bytes)
4. 0x026daa90 (34,012 bytes)
5. 0x022c7120 (29,844 bytes)

Write a Ghidra script to decompile these and check for string references that identify the subsystem. If they reference RocksDB, Tcl, Python, or X11 -- label them in Ghidra and move on.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
## Classification Results: All 5 are LIBRARY code

All top 5 largest unnamed functions are statically-linked library code. None contain Saleae business logic. No further RE investigation needed for these functions.

### Function-by-function classification:

| # | ELF VA | Size | Classification | Library | Identity |
|---|--------|------|---------------|---------|----------|
| 1 | 0x02a3a820 | 44,952 B | LIBRARY | Tcl | `TclNRExecuteByteCode` - Tcl bytecode interpreter main loop. 57 strings (coroutine, yield, dict, array ops). 84 named Tcl_* callees. |
| 2 | 0x01f6d820 | 41,864 B | LIBRARY | RocksDB | `DBOptions` serializer/parser. 61 strings all RocksDB config keys (wal_recovery_mode, enable_pipelined_write, etc). |
| 3 | 0x01f379c0 | 34,211 B | LIBRARY | RocksDB | `ColumnFamilyOptions` serializer/parser. 53 strings all RocksDB CF config (compaction, bloom filter, blob storage, etc). |
| 4 | 0x026daa90 | 34,012 B | LIBRARY | LZMA | LZMA range decoder. 0 strings, 1 named callee (memcpy). Characteristic range coding pattern: 0x800 probability init, 0x1000000 normalization threshold, >>5 probability adaptation, >>0xb range scaling. |
| 5 | 0x022c7120 | 29,844 B | LIBRARY | Python (CPython) | Python `struct` module pack/unpack or pickle serializer. 2 strings (surrogatepass, too many objects). 23 named Py* callees (PyDict_Next, PyLong_AsLong, _PyFloat_Pack8, etc). |

### Conclusion
The top 5 largest unnamed functions are all well-known library code (Tcl, RocksDB x2, LZMA, CPython). This confirms the expectation that the largest functions in a statically-linked binary tend to be library interpreters/parsers, not application logic. Saleae business logic functions are likely smaller and more numerous.

### Artifacts
- Ghidra script: `re/ghidra/scripts/ClassifyLargeFunctions.java`
- Full decompiled output: `re/ghidra/decompiled-top5-large-funcs.txt`
<!-- SECTION:FINAL_SUMMARY:END -->
