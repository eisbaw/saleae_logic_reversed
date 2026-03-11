---
id: TASK-0111
title: Fix JESD204B power sequence to match pcap register ordering
status: Done
assignee: []
created_date: '2026-03-11 05:59'
updated_date: '2026-03-11 06:26'
labels:
  - rust
  - saleae-driver
  - fpga
  - jesd
dependencies: []
references:
  - re/notes/init-sequence-pcap.md
  - crates/saleae-driver/src/fpga.rs
  - re/notes/jesd204b-link.md
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Pcap reveals the exact JESD204B power-up register sequence that we don't follow precisely:

Register 0x00 write sequence: 0x1e -> 0x1c -> 0x18 -> 0x08 -> 0x00

This is a staged power-up: each write enables one more subsystem. Our current code may not follow this exact ordering, which could cause JESD link instability on real hardware.

Also from pcap Phase 3:
- ADC bank register access pattern: indirect via regs 0x03/0x04/0x05/0x16
- The indirect access writes bank select to 0x03, address to 0x04, data to 0x05, then triggers via 0x16
- 16 sequential calibration coefficient writes to register 0x17

Implementation:
1. Update fpga_post_init() to use the exact 5-step JESD power sequence
2. Implement indirect ADC register access (bank/addr/data/trigger pattern)
3. Add calibration coefficient register writes (16x to reg 0x17)
4. Add tests validating register write ordering

Reference: re/notes/init-sequence-pcap.md (Phase 3)
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Created jesd.rs with 3 functions: jesd_power_sequence (5-step 0x1E->0x1C->0x18->0x08->0x00), write_adc_register (indirect bank/addr/data/trigger via regs 0x03/0x04/0x05/0x16), write_calibration_coefficients (16x to reg 0x17). Added 17 named constants to fpga_regs.rs. Replaced all magic numbers in init.rs with FEAT_* constants. 7 new tests, all 583 workspace tests pass.
<!-- SECTION:FINAL_SUMMARY:END -->
