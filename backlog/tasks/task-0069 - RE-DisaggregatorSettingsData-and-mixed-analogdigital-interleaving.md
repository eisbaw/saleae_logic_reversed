---
id: TASK-0069
title: RE DisaggregatorSettingsData and mixed analog+digital interleaving
status: Done
assignee: []
created_date: '2026-03-10 12:23'
updated_date: '2026-03-10 15:58'
labels:
  - re
  - ghidra
  - data-format
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Decompile the data disaggregation layer that separates interleaved analog and digital samples from the USB stream. Partially covered by TASK-0035 and TASK-0038 but gaps remain. Need:

- Full DisaggregatorSettingsData struct layout
- DisaggregatorNode processing logic
- How analog and digital samples are interleaved at different sample rates
- Per-channel sample extraction from interleaved stream
- Handling of mixed-rate configurations (e.g. digital at 100MHz, analog at 50MHz)
- Chunk boundary alignment rules

This is needed to correctly parse captures that use both analog and digital channels simultaneously.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
## Summary

Reverse engineered the DisaggregatorNode's complete interleaving architecture through 5 rounds of Ghidra headless decompilation, covering 12+ functions.

### Key Findings

**Device-type dispatch**: ProcessChunk (ELF 0x1aea5d0, 2097 bytes) switches on a device type enum (0-7) to dispatch to 4 device-specific deinterleaving functions:
- Type 1 (Logic 8): FUN_01bf21b0 - 8 bytes per cycle, 1 byte per channel, bit reordering
- Type 2 (Logic 16): FUN_01bf37e0 - 2*N bytes per cycle, byte-swapping
- Type 3 (Logic 4): FUN_01bf2a90 - shift-mode aware, 2-byte aligned
- Types 4/5/6 (Pro 8/16): FUN_01bf1190 - full mixed digital+analog interleaving

**Pro device interleaving (primary target)**:
- Full cycle = digital_groups + analog_groups
- Digital: byte-swap (bswap16 or bswap32) based on GPIF bus width
- Analog: packed 12-bit ADC values, either 4 samples in 6 bytes (16-bit GPIF) or 8 samples in 12 bytes (32-bit GPIF, SSE-optimized)
- Sign extension via `psraw(vec, 4)` - arithmetic right shift by 4

**DeviceCalculateMixRate**: Computes digital/analog ratio via GCD. Three valid GPIF/ADC combos: 16-bit/10-bit, 16-bit/12-bit, 32-bit/12-bit.

**ContractReadData**: Confirmed to run before disaggregation, removes every Nth byte for bus-width padding (N = LowSpeedDivider at this+0x19c).

**Settings struct**: 0xa0 bytes, inner object at +0x18 contains digital_ratio, digital_packet_size, analog_group_size, analog_bytes_per_sample, and 32-slot channel enable arrays.

### Files Changed
- `re/notes/disaggregator-interleaving.md` - comprehensive analysis document
- `re/ghidra/scripts/DecompileDisaggregator[1-5].py` - Ghidra decompilation scripts
<!-- SECTION:FINAL_SUMMARY:END -->
