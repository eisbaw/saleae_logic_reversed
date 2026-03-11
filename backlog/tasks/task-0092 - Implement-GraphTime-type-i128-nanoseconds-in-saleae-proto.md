---
id: TASK-0092
title: Implement GraphTime type (i128 nanoseconds) in saleae-proto
status: Done
assignee: []
created_date: '2026-03-10 23:13'
updated_date: '2026-03-11 00:15'
labels:
  - rust
  - saleae-proto
  - implementation
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-0087 confirmed boost::multiprecision is used solely for 128-bit nanosecond timestamps. Implement a `GraphTime` wrapper type in saleae-proto using Rust's native i128.

Requirements:
- `GraphTime(i128)` wrapping nanoseconds
- From/Into conversions: seconds+fractional_ns -> i128 (the internal representation found in binary)
- Arithmetic ops (Add, Sub, comparison)
- Display formatting (seconds.nanoseconds)
- Serialize to bytes for RocksDB key format (big-endian i128)
- SampleIndex -> GraphTime conversion given a sample rate

Reference: re/notes/boost-multiprecision.md
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Implemented `GraphTime(pub i128)` newtype in `crates/saleae-proto/src/graph_time.rs`.\n\nAPI surface:\n- Construction: `ZERO`, `from_nanos`, `from_seconds`, `from_seconds_and_frac`\n- Accessors: `nanos`, `seconds`, `subsec_nanos`\n- Arithmetic: `Add`, `Sub`, `Neg`, `Mul<i128>`, `Div<i128>`\n- Comparison: derived `Eq`, `PartialEq`, `Ord`, `PartialOrd`\n- Display: \"X.XXXXXXXXX s\" (9 decimal places)\n- Serialization: `to_be_bytes`/`from_be_bytes` with sign-bit flip for correct lexicographic ordering in RocksDB\n- Sample conversion: `from_sample_index`/`to_sample_index`\n\n43 unit tests covering conversions, arithmetic, display, byte roundtrips, sample index conversion, and edge cases (zero, negative, i128::MIN/MAX).\n\nExported as `saleae_proto::GraphTime` from crate root.
<!-- SECTION:FINAL_SUMMARY:END -->
