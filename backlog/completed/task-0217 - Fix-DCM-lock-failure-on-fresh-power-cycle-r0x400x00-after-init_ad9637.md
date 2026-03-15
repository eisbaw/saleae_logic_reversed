---
id: TASK-0217
title: Fix DCM lock failure on fresh power cycle (r0x40=0x00 after init_ad9637)
status: Done
assignee: []
created_date: '2026-03-13 21:28'
updated_date: '2026-03-15 09:05'
labels:
  - bug
  - hardware
  - adc
  - dcm
dependencies: []
references:
  - crates/saleae-driver/src/adc.rs
  - re/notes/init-sequence-pcap.md
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
After power cycle + fresh firmware upload + FPGA program, DCM doesn't lock (r0x40=0x00 instead of 0xE0). This worked at tag v0.2-capture-works (2026-03-13) but now fails consistently.

The FPGA programs and verifies fine (readback=0xAA), EEPROM device ID reads correctly. The failure is specifically in the DCM cascade during init_ad9637 — reset_main_dcm reads r0x40=0x00 when it expects 0xE0 (bits 5+6+7 set).

Possible causes:
- DCM cascade needs retry logic (Saleae binary may retry)
- Timing issue — need delay between FPGA program and ADC init
- Bitstream variant mismatch (we use -02, maybe -03 is needed now)
- Board revision sensitivity (Rev 1.02 Xilinx Spartan-6)

Previously worked on same hardware with same bitstream. No functional code changes between working tag and current HEAD (only formatting and opcode fix which is after the DCM step).

Test: `just hw-init` after power cycle. Expected: DCM locks (r0x40=0xE0). Actual: r0x40=0x00.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Same root cause as TASK-0220: wrong bitstream variant. slx9-02 was set as default but slx9-03 is the correct variant for Rev 1.2 hardware. With slx9-03, DCM locks reliably on fresh power cycle.
<!-- SECTION:FINAL_SUMMARY:END -->
