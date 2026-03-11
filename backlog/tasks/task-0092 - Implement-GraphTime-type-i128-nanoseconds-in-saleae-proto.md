---
id: TASK-0092
title: Implement GraphTime type (i128 nanoseconds) in saleae-proto
status: Done
assignee: []
created_date: '2026-03-10 23:13'
updated_date: '2026-03-11 00:16'
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
GraphTime(i128) newtype in saleae-proto/src/graph_time.rs.\n\nConstruction: ZERO, from_nanos, from_seconds, from_seconds_and_frac (matches binary's {u64 seconds, f64 fractional_ns}).\nAccessors: nanos(), seconds(), subsec_nanos().\nArithmetic: Add, Sub, Neg, Mul<i128>, Div<i128>.\nDisplay: 'X.XXXXXXXXX s' with 9 zero-padded decimal places.\nSerialization: to_be_bytes/from_be_bytes with sign-bit XOR for correct RocksDB lexicographic key ordering.\nSample conversion: from_sample_index/to_sample_index using i128 to prevent overflow.\n\n43 tests passing."
<!-- SECTION:FINAL_SUMMARY:END -->
