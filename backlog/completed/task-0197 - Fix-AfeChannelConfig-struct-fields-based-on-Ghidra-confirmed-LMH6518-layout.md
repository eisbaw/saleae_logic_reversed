---
id: TASK-0197
title: Fix AfeChannelConfig struct fields based on Ghidra-confirmed LMH6518 layout
status: Done
assignee: []
created_date: '2026-03-13 10:07'
updated_date: '2026-03-13 10:19'
labels:
  - rust
  - correctness
  - RE-confirmed
dependencies: []
references:
  - re/notes/afe-config-enumeration.md
  - re/ghidra/decompiled-afe-config.txt
  - crates/saleae-driver/src/capture.rs
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-0192 Ghidra analysis confirmed the LMH6518 VGA SPI register layout. The current `AfeChannelConfig` struct in `crates/saleae-driver/src/capture.rs` has incorrect/speculative fields:

1. `gain` field doesn't exist on LMH6518 — the chip has `preamp_gain` (bool, bit 4) and `ladder_gain` (3-bit attenuation, bits 6:4 in auxiliary mode)
2. `ladder` should be `i8` (signed, represents dB attenuation 0 to -20)
3. `dac_offset` should be `f64` (DAC trim voltage, not u16)
4. `to_register_value()` bit packing is wrong — doesn't match LMH6518 SPI format (bit 0 = power down, bits 3:1 = filter select, bit 4 = preamp HG/LG, bits 7:5 = ladder)
5. `from_register_value()` needs to be added for reading back AFE state

Reference: `re/notes/afe-config-enumeration.md` and `re/ghidra/decompiled-afe-config.txt` contain the confirmed layout from Ghidra decompilation of 66 AFE configurations.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 AfeChannelConfig fields match LMH6518 SPI register layout from Ghidra analysis
- [x] #2 to_register_value() produces correct SPI words matching pcap-observed values
- [x] #3 from_register_value() round-trips correctly
- [x] #4 All 66 AFE configs from Ghidra decompilation can be represented
- [x] #5 Existing tests updated to use correct field names
<!-- AC:END -->
