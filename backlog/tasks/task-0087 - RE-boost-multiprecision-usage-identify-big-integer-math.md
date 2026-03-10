---
id: TASK-0087
title: 'RE boost::multiprecision usage -- identify big integer math'
status: Done
assignee: []
created_date: '2026-03-10 12:32'
updated_date: '2026-03-10 20:15'
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
## Findings

Boost.Multiprecision is used exclusively for **128-bit integer timestamp arithmetic** -- not for cryptography, sample counting, or arbitrary precision math.

### Key discoveries:
1. **Only one boost header string** survives: `divide.hpp` with `eval_modulus` at ELF 0x2db34e0
2. **35 functions** use 128-bit intrinsics (`__udivti3`, `__fixunsdfti`, etc.)
3. All fall into 4 categories:
   - Integer-to-string formatting (libc++ `to_string` for `__int128`)
   - `std::filesystem::__last_write_time` (libc++ internal)
   - **GraphTime/SaleaeTime timestamp conversion** (the actual Saleae code)
   - RocksDB sample storage with nanosecond timestamp keys

### GraphTime internal representation:
```
struct GraphTime { uint64_t seconds; double fractional_ns; };
```
Converted to 128-bit nanoseconds when precision matters: `total_ns = (i128)seconds * 1e9 + (i128)fractional_ns`

### The boost eval_modulus is used for exactly one thing:
Computing `timestamp % signal_period` in `SimulatorSignalCsvRepeating::FindTrigger` (MSO simulator) to find where in a repeating waveform a trigger event occurs.

### Rust implication: **No bignum crate needed**
Rust's native `i128`/`u128` compiles to the same `__udivti3`/`__umodti3` intrinsics. Direct replacement.

### Full analysis: `re/notes/boost-multiprecision.md`
<!-- SECTION:FINAL_SUMMARY:END -->
