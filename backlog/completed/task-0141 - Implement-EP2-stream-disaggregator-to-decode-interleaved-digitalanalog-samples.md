---
id: TASK-0141
title: >-
  Implement EP2 stream disaggregator to decode interleaved digital+analog
  samples
status: Done
assignee: []
created_date: '2026-03-12 13:46'
updated_date: '2026-03-12 14:12'
labels:
  - phase-2
  - data-path
dependencies: []
references:
  - re/notes/disaggregator-interleaving.md
  - crates/saleae-driver/src/capture.rs
  - re/notes/streaming-thread-decompilation.md
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Implement the DisaggregatorNode equivalent in Rust to decode raw EP2 bulk data into per-channel sample streams.

The Logic Pro 8 with 32-bit GPIF and 12-bit AD9637 ADC produces interleaved cycles on EP2:
- Digital portion: `digital_ratio * 4 bytes * num_digital_channels` (bswap32, each byte = 8 packed 1-bit samples)
- Analog portion: `12 bytes * num_analog_channels` -> 8 x int16_t samples (packed 12-bit, sign-extended via arithmetic right shift by 4)
- Full cycle size = digital_group_bytes + analog_group_bytes
- Data length must be a multiple of full_cycle_size

Also need ContractReadData pre-processing (remove padding bytes when LowSpeedDivider > 1).

RE source: `re/notes/disaggregator-interleaving.md` has complete decompilation analysis with byte layout examples and confidence assessments.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Disaggregator parses raw EP2 bytes into per-channel digital sample streams (Vec<u8>, 8 samples per byte)
- [x] #2 Disaggregator parses raw EP2 bytes into per-channel analog sample streams (Vec<i16>, 12-bit sign-extended)
- [x] #3 Unit tests with known byte patterns validate correct deinterleaving
- [x] #4 Integration with capture-quick or full-init CLI command to dump decoded samples
- [x] #5 Validated against hardware capture: digital channels show expected idle/active patterns
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Implementation (2026-03-12)

Module: `crates/saleae-driver/src/disaggregator.rs` (1114 lines, 30 tests)

### What works
- Digital extraction: 32-bit GPIF (bswap32) and 16-bit GPIF (bswap16)
- Analog extraction: 12-bit packed, 8 samples from 12 bytes (32-bit GPIF) and 4 from 6 bytes (16-bit)
- Sign extension via `(val as u16 as i16) >> 4` matching binary's SSE psraw
- ContractReadData decimation (dividers 1/2/4/8)
- compute_mix_rate() from digital/analog rates + GPIF width + ADC bits
- Integrated into full-init CLI: captures + decodes + prints per-channel stats

### Hardware validation
- 49.8 MB captured at 24.3 MB/s, 128-byte cycles (32 digital + 96 analog)
- 12.5M digital samples/ch: all low (correct for open inputs)
- 3.1M analog samples/ch: stable DC offsets with ~40 LSB noise (correct for unconnected ADC)
- Zero remainder bytes -- cycle alignment confirmed

### RE doc discrepancy found
- `analog_bytes_per_sample` (3) represents bytes per *pair* of 12-bit samples, not per sample
- Correct formula: `analog_channel_bytes = analog_bytes_per_sample * analog_group_size / 2`
- i.e. 12 bytes per channel for 8-sample group, not 24
<!-- SECTION:NOTES:END -->
