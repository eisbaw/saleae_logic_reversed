---
id: TASK-0227
title: 'RE: Decode SetCaptureParameters from Ghidra for all capture modes'
status: Done
assignee: []
created_date: '2026-03-15 17:28'
updated_date: '2026-03-15 20:57'
labels:
  - RE
  - ghidra
dependencies: []
references:
  - re/notes/fpga-register-map.md
  - crates/saleae-driver/src/capture.rs
  - re/notes/init-sequence-pcap.md
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Decompile `Logic2FpgaDevice::SetCaptureParameters` more thoroughly to understand how the official software computes register values for different capture modes (digital-only, analog-only, mixed).

**Current knowledge**: We have the register list from fpga-register-map.md and a partial decompilation. The `compute_capture_registers()` function matches the pcap for 12.5 MHz mixed mode but fails for other configurations.

**Specific questions to answer**:
1. How does SetCaptureParameters compute r0x0d/r0x0e (mix_ratio_a/b) for digital-only mode?
2. How does it compute them for analog-only mode?
3. Does r0x12 (capture_config_2) change between modes?
4. Is r0x1a (adc_config) always written, or only in analog mode?
5. What is the exact formula for r0x14 (gpif_bus_width / decimation_factor)?
6. Are there any mode-dependent registers not in the standard 12-register batch?

**Approach**:
1. Use Ghidra headless to decompile SetCaptureParameters (known ELF VA from fpga-register-map.md)
2. Trace the full computation path for each register
3. Pay special attention to conditional branches based on channel counts
4. Cross-reference with pcap data once captured
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 SetCaptureParameters decompiled with all register computations documented
- [ ] #2 Conditional logic for digital-only vs analog-only vs mixed identified
- [ ] #3 Formula for each register documented with source line references
- [ ] #4 compute_capture_registers() updated to match decompilation
<!-- AC:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Decompiled SetCaptureParameters (ELF 0x1c24ca0) from Ghidra. All 12 registers match all 6 pcap configs exactly. Found 7 bugs in current Rust implementation (2 critical, 3 moderate, 1 minor, 1 doc). Full analysis in re/notes/capture-register-formulas.md.
<!-- SECTION:FINAL_SUMMARY:END -->
