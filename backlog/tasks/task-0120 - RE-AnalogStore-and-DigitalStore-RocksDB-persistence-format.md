---
id: TASK-0120
title: RE AnalogStore and DigitalStore RocksDB persistence format
status: To Do
assignee: []
created_date: '2026-03-11 07:55'
labels:
  - re
  - ghidra
  - data-format
dependencies: []
references:
  - crates/saleae-proto/src/graph_time.rs
  - re/notes/waveform-persistence.md
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
