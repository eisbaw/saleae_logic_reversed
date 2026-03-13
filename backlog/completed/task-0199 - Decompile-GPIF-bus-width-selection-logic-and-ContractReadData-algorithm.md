---
id: TASK-0199
title: Decompile GPIF bus width selection logic and ContractReadData algorithm
status: Done
assignee: []
created_date: '2026-03-13 10:09'
updated_date: '2026-03-13 10:41'
labels:
  - RE
  - EP2
  - Ghidra
dependencies: []
references:
  - re/notes/disaggregator-interleaving.md
  - crates/saleae-proto/src/disaggregator.rs
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Two critical EP2 data path functions are not fully understood:

1. **GPIF bus width selection**: Determines 16-bit vs 32-bit GPIF mode based on (device_type, analog_channel_count). Error string "No valid analog and gpif width combo given" is known but selection table not decompiled. This affects analog sample unpacking.

2. **ContractReadData** (ELF VA 0x1d3bad0): Removes padding bytes from bus-width-aligned interleaved stream. Uses divisor-based keep-every-Nth-byte logic (divisors 1/2/4/8). When each divisor is used and how it relates to GPIF width + channel count is not documented.

Both are needed to correctly handle all device/channel combinations.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 GPIF width selection table documented for all device/channel combos
- [x] #2 ContractReadData divisor selection logic documented
- [x] #3 Both documented in re/notes/ or doc/protocol/
- [x] #4 Disaggregator updated if corrections needed
<!-- AC:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Decompiled three key functions via Ghidra headless scripts:

1. **GPIF width selection** (DeviceSettings caller at ELF 0x1c4a6f9): Formula `(device_type_id & ~1) == 6` gives 32-bit for Pro devices (type 6/7), 16-bit for all others. Static per-device, not dependent on analog channel count. ADC bits hardcoded to 12 in this path.

2. **LowSpeedDivider** (LogicDevice::StartSampling at ELF 0x1c3ad40): Determined purely by sample rate: 25k->8, 50k->4, 100k->2, >=200k->1. At low rates the FPGA pads the EP2 stream; ContractReadData strips the padding.

3. **ContractReadData** (ELF 0x1c3bad0): Simple stride-N byte copy with 4x loop unrolling. Called from OnWrite, which then calls ExpandWriteData to restore the buffer for DMA reuse.

Code changes:
- `capture.rs`: Replace hardcoded `low_speed_divider=1` with `compute_low_speed_divider(sample_rate)` based on decompiled thresholds. Added 5 tests.
- `disaggregator.rs`: Updated `contract_read_data` doc comment with divider selection context.
- New RE notes: `re/notes/gpif-width-selection.md` with full analysis.
- Two new Ghidra scripts and decompilation outputs.
<!-- SECTION:FINAL_SUMMARY:END -->
