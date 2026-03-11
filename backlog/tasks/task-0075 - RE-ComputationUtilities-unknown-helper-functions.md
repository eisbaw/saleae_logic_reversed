---
id: TASK-0075
title: RE ComputationUtilities -- unknown helper functions
status: Done
assignee: []
created_date: '2026-03-10 12:31'
updated_date: '2026-03-10 17:35'
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
ComputationUtilities.cpp contains 19 math/utility functions. Key functions with assertion attribution:\n- SelectClosestIndex (two overloads, U64 and U32) at ELF 0x1c46ab0, 0x1c46d40\n- Lcm(U32,U32) at 0x1c47170 with overflow assertion\n- LcmWithQuotients(U32,U32,U32*,U32*) at 0x1c47240\n- Gcd(U32,U32) at 0x1c47140 (Euclidean, 48 bytes)\n\nAlso by proximity: RoundTowardZero, Ceil, IsInteger, ClampU8, Distance2D, AbsDiff (U32/U64), Abs (I32/I64/double), SeedRandom, RandomInRange, RandomSelect.\n\nMost are inlined at call sites -- only Gcd (called by DeviceCalculateMixRate) and Lcm (called by DeviceCalculateSamplesCollected) have direct call references.\n\nFull analysis in re/notes/computation-utilities.md."
<!-- SECTION:FINAL_SUMMARY:END -->
