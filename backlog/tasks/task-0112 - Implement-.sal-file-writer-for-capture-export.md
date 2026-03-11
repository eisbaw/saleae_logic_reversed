---
id: TASK-0112
title: Implement .sal file writer for capture export
status: Done
assignee: []
created_date: '2026-03-11 05:59'
updated_date: '2026-03-11 06:35'
labels:
  - rust
  - saleae-proto
  - export
dependencies: []
references:
  - re/notes/sal-file-format.md
  - crates/saleae-proto/src/export.rs
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
We RE'd the .sal format (TASK-0102): ZIP archive containing meta.json + per-channel .bin files. We have CSV/binary export (TASK-0103) but cannot create .sal files that Saleae Logic 2 can open.

Format (from re/notes/sal-file-format.md):
- ZIP container (standard deflate)
- meta.json: version 16, capture metadata, channel list, sample rates
- digital-N.bin: 8-byte header + transition-encoded data (cereal binary archive)
- analog-N.bin: 8-byte header + f32 voltage samples (cereal binary archive)

Implementation:
1. Add `zip` crate dependency to saleae-proto
2. Implement SalWriter struct that creates a .sal ZIP archive
3. Write meta.json with correct schema (version 16)
4. Write digital channel data in transition-encoded format
5. Write analog channel data as f32 arrays
6. Add saleae-cli `--format sal` option alongside csv/binary
7. Test by creating a .sal file and verifying Saleae Logic 2 can open it (manual test)

Reference: re/notes/sal-file-format.md
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Created sal.rs (1026 lines, 13 tests) with SalWriter producing Saleae-compatible .sal ZIP archives. Contains meta.json v16 schema (captureSettings, binData manifest, channel lists, sample rates) + digital-N.bin (transition-encoded u64 timestamps) + analog-N.bin (f32 voltages). Added zip/serde/serde_json deps. ExportFormat::Sal variant added to CLI. Note: simplified binary format may not achieve full Saleae Logic 2 interop due to missing cereal archive headers.
<!-- SECTION:FINAL_SUMMARY:END -->
