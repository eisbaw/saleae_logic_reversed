---
id: TASK-0138
title: >-
  DCM lock fails on hardware - investigate r0x40=0x00 with both 2-step and
  5-step cascade
status: Done
assignee: []
created_date: '2026-03-12 06:14'
updated_date: '2026-03-12 09:30'
labels:
  - hardware
  - dcm
  - blocker
dependencies: []
references:
  - crates/saleae-driver/src/adc.rs
  - crates/saleae-driver/src/fpga.rs
  - re/captures/
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Hardware testing on Logic Pro 8 Rev 1.2 (Spartan-6, AD9637) shows r0x40=0x00 after both DCM reset approaches:

1. **2-step reset** (decompiled binary, AdcType==1): r0x00=0x04, sleep 1ms, r0x00=0x00 → r0x40=0x00
2. **5-step JESD cascade** (pcap-validated): 0x1E→0x1C→0x18→0x08→0x00 with ADC reset between retries → r0x40=0x00 after 3 attempts

Previous session reported `device-init` getting r0x40=0x20 with the 2-step code + fpga_post_init, but current testing can't reproduce this because the FX3 command thread died and we need a power cycle.

**Key question**: Is there something else in the pcap sequence between FPGA programming and the first DCM cascade that we're missing? The pcap may do additional register writes that prime the FPGA for DCM operation.

**Investigation needed:**
- Re-analyze the pcap capture to find ALL register writes between FPGA verify and first ResetMainDcm call
- Check if `fpga_post_init` register values match pcap exactly (COMMIT_CONTROL, CAPTURE_CONTROL, INIT_CONFIG, GPIF values)
- Test after power cycle + fresh firmware upload
- Consider whether GPIF must be fully configured before DCM works
- Check if any EP2 setup (USB streaming configuration) is required before DCM

**Hardware state**: EP1 command thread is dead (bulk OUT timeout), needs physical power cycle before further testing.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 r0x40 shows non-zero DCM lock status after reset_main_dcm on hardware
- [x] #2 Root cause of r0x40=0x00 identified and documented
<!-- AC:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
**Root cause identified and fixed.**

The DCM lock failure had TWO causes:

1. **Wrong bitstream variant**: This device needs `bitstream-spartan6-slx9-03.bin`, not `slx9-02`. With slx9-02, r0x40 stays 0x00 permanently. With slx9-03, DCM locks to r0x40=0xE0 on the second cascade exactly as the pcap shows. The FPGA verify (r0x7F=0xAA readback) passes for multiple variants (02, 03, 07) but only slx9-03 produces a working ADC clock for the DCM.

2. **Wrong DCM cascade sequence**: The code was using a 2-step reset (r0x00=0x04, sleep, r0x00=0x00) instead of the pcap-validated 5-step JESD cascade (0x1E→0x1C→0x18→0x08→0x00). Fixed `dcm_cascade()` to use `jesd::jesd_power_sequence()`. Removed SERDES retry (not present in pcap for AdcType==0 devices).

**Hardware-validated result**: r0x40=0xE0 (all DCMs locked), r0x4a=0x00 (no bitslip), 49.8 MB captured at 24.3 MB/s over EP2.

**Code changes**: adc.rs (dcm_cascade, reset_main_dcm simplified), fpga.rs/integration.rs (mock helpers updated), commands.rs (default bitstream, auto-detection), main.rs (--auto-bitstream flag).
<!-- SECTION:FINAL_SUMMARY:END -->
