---
id: TASK-0087
title: 'RE boost::multiprecision usage -- identify big integer math'
status: Done
assignee: []
created_date: '2026-03-10 12:32'
updated_date: '2026-03-10 20:16'
labels:
  - re
  - ghidra
  - math
dependencies: []
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Build path leaked: `/workspace/monorepo/build/graph-io/linux-x64/release/vcpkg_installed/.../boost/multiprecision/cpp_int/divide.hpp`

Boost.Multiprecision is linked for arbitrary-precision integer arithmetic. This is unusual for a logic analyzer. Possible uses:
- 64-bit+ timestamp arithmetic (sample counts at high rates overflow u64?)
- Crypto operations (ECDSA curve math for ATECC508A?)
- Precise frequency/period calculations
- Sample rate ratio computations for mixed-rate captures

Investigation steps:
1. Search for xrefs to multiprecision symbols in the binary
2. Identify which source files call into multiprecision
3. Determine if it's used for timestamps, crypto, or something else
4. Check if we need equivalent precision in our Rust implementation

Could reveal a subtle precision requirement we'd otherwise miss.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Boost.Multiprecision is used exclusively for 128-bit integer timestamp arithmetic. None of the initial hypotheses (crypto, u64 overflow, arbitrary-precision) were correct.\n\nUsage breakdown:\n- 20 GraphTime/SaleaeTime functions (ELF 0x1c89c40-0x1c994b0): Convert {uint64_t seconds, double fractional_ns} to 128-bit nanosecond counts for RocksDB keys, sample-index-to-time, measurement formatting. Pattern: total_ns = (i128)seconds * 1e9 + (i128)fractional_ns.\n- 1 signal period modulus (ELF 0x2db2bc0): SimulatorSignalCsvRepeating::FindTrigger -- MSO simulator code, out of scope.\n- 4 integer-to-string formatting: libc++ {fmt} routines for __int128 -> decimal.\n- 1 filesystem timestamp: std::filesystem::__last_write_time splitting 128-bit ns into timespec.\n\nRust implication: No bignum crate needed. Native i128/u128 compiles to same __udivti3/__umodti3 intrinsics. GraphTime wrapping i128 nanoseconds is the natural Rust equivalent.\n\nAnalysis: re/notes/boost-multiprecision.md"
<!-- SECTION:FINAL_SUMMARY:END -->
