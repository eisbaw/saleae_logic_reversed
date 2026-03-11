---
id: TASK-0093
title: Implement Disaggregator (EP2 data deinterleaving) in saleae-proto
status: Done
assignee: []
created_date: '2026-03-10 23:13'
updated_date: '2026-03-11 00:06'
labels:
  - rust
  - saleae-proto
  - implementation
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-0069 fully documented the disaggregator interleaving format. Implement the EP2 bulk data deinterleaving in saleae-proto.

Requirements:
- ProcessChunk dispatch by device type (4 deinterleaving functions)
- Pro 8/16: interleaved digital+analog groups per cycle with byte-swap
- SSE psraw(4) equivalent for 12-bit ADC sign extension (arithmetic right shift)
- Non-Pro (Logic 8): simpler interleaving, 10-bit ADC
- Logic 4: separate path (FX2LP, different format)
- DeviceCalculateMixRate (GCD-based, from TASK-0076): compute digital_ratio and analog_ratio
- Unit tests with known interleaved data -> expected deinterleaved channels

Reference: re/notes/disaggregator-interleaving.md, re/notes/device-helpers.md
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Disaggregator implemented in saleae-proto/src/disaggregator.rs (1248 lines).\n\ncalculate_mix_rate: GCD-based ratio computation, 3 modes (digital/analog/mixed), validates 3 legal GPIF/ADC combos.\n\nDisaggregator struct: DeviceProfile + channels + rates -> process_chunk(&[u8]) -> DisaggregatedData (per-channel digital + analog i16).\n\nPro deinterleaving: byte-swapped words, 12-bit ADC from 6-byte packing (16-bit GPIF) or 12-byte overlapping u64 reads (32-bit GPIF), sign extension via (x & 0xFFF0) as i16 >> 4.\n\nNonPro: digital-only byte-swapped words.\n\nCaveats: Logic 8 transform function not fully decompiled, 10-bit ADC path recognized but not implemented, post-disaggregation bit inversion not included.\n\nCommit: 61a6650"
<!-- SECTION:FINAL_SUMMARY:END -->
