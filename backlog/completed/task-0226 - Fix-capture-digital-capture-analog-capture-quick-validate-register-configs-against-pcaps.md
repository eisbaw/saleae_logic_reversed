---
id: TASK-0226
title: >-
  Fix capture-digital/capture-analog/capture-quick: validate register configs
  against pcaps
status: Done
assignee: []
created_date: '2026-03-15 17:28'
updated_date: '2026-03-15 22:31'
labels:
  - bug
  - hardware
dependencies:
  - TASK-0221
  - TASK-0222
  - TASK-0223
references:
  - crates/saleae-driver/src/capture.rs
  - crates/saleae-cli/src/commands.rs
  - re/notes/fpga-register-map.md
  - re/notes/init-sequence-pcap.md
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The `capture-digital`, `capture-analog`, and `capture-quick` CLI subcommands all fail with 0 bytes from EP2 (10s timeout). Only `full-init` works because it uses the pcap-validated 12.5 MHz mixed-mode register set.

**Root cause**: `compute_capture_registers()` produces register values for arbitrary sample rates and channel configurations, but these have never been validated against hardware. The FPGA does not start the GPIF data path with incorrect register values.

**Approach**:
1. Capture pcaps at various configurations (see dependent pcap tasks)
2. Compare each pcap's register values with our `compute_capture_registers()` output
3. Identify which registers are wrong
4. Fix the register computation
5. Test each subcommand against hardware

**Known working config**: 12.5 MHz, 8 digital + 8 analog (from init-sequence-pcap.md)
**Known broken configs**: 1 MHz digital-only, 1 MHz analog-only, 12.5 MHz digital-only via capture-quick

**Specific areas of suspicion**:
- r0x0d/r0x0e mix ratios for non-mixed configs (digital-only or analog-only)
- r0x12 (capture_config_2) may need different values per mode
- r0x14 (gpif_bus_width) may need to change with channel count
- r0x1a (adc_config) may be wrong for digital-only
- Channel enable sequence (r0x0f/r0x10/r0x11 from Phase 4/5) may be required before capture
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 capture-digital produces EP2 data at 1 MHz 8ch
- [ ] #2 capture-analog produces EP2 data at 1 MHz 2ch
- [ ] #3 capture-quick produces EP2 data in digital-only mode
- [ ] #4 hw-smoke recipe passes (all 3 capture types work)
- [ ] #5 hw-capture-matrix passes majority of test configurations
<!-- AC:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Fixed all 7 bugs in compute_capture_registers() validated against 6 pcap configs and Ghidra decompilation. All hw recipes pass (14/14 matrix, 3/3 idempotent). Committed as 659bed3, ee82354. Tagged v0.4.0.
<!-- SECTION:FINAL_SUMMARY:END -->
