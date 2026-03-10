---
id: TASK-0075
title: RE ComputationUtilities -- unknown helper functions
status: Done
assignee: []
created_date: '2026-03-10 12:31'
updated_date: '2026-03-10 17:34'
labels:
  - re
  - ghidra
  - device-helpers
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Source path leaked: `/workspace/monorepo/graph-io/devices/logic_interface/src/ComputationUtilities.cpp`

Unexplored helper module in the device layer. Could contain:
- Sample rate calculation formulas
- Channel mapping logic
- Buffer size computations
- Timing/clock arithmetic
- Interleaving math

Investigation steps:
1. Search binary for strings containing "ComputationUtilities" or nearby assertion messages
2. Identify functions compiled from this translation unit
3. Decompile and categorize what computations are performed
4. Cross-reference with gaps in our understanding (transfer sizing, sample rate limits, etc.)

May fill in missing pieces for TASK-0070 (TransferQueue sizing) or TASK-0069 (interleaving).
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Fully cataloged ComputationUtilities.cpp: 19 functions spanning ELF VA 0x1c46900-0x1c47340.\n\nConfirmed functions (via assertion strings):\n- SelectClosestIndex(vector<U64>&, U64) at 0x1c46ab0\n- SelectClosestIndex(vector<U32>&, U32) at 0x1c46d40\n- Lcm(U32, U32) at 0x1c47170 -- with overflow detection\n- LcmWithQuotients(U32, U32, U32*, U32*) at 0x1c47240\n\nAttributed by proximity (no assertions, small utility functions):\n- Gcd(U32, U32) at 0x1c47140 -- Euclidean algorithm, called by DeviceCalculateMixRate\n- RoundTowardZero, Ceil, IsInteger (inlined in rate validation)\n- ClampU8, Distance2D\n- AbsDiff (U32/U64), Abs (I32/I64/double)\n- SeedRandom, RandomInRange, RandomSelect (int/long overloads)\n\nMost functions have zero direct callers (compiler inlines them). Only Gcd (1 caller: DeviceCalculateMixRate) and Lcm (1 caller: DeviceCalculateSamplesCollected) retain direct call references.\n\nFor Rust implementation: most map to standard library functions (abs_diff, clamp, gcd from num crate). SelectClosestIndex is trivially replaced by iterator min_by_key.
<!-- SECTION:FINAL_SUMMARY:END -->
