---
id: TASK-0139
title: EP2 returns 0 bytes during capture - no data flows after StartSampling
status: Done
assignee: []
created_date: '2026-03-12 06:15'
updated_date: '2026-03-12 09:30'
labels:
  - hardware
  - capture
  - blocker
dependencies:
  - TASK-0138
references:
  - crates/saleae-cli/src/commands.rs
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Even with FPGA programmed, post-init complete, and all capture registers verified via readback, EP2 IN returns 0 bytes and times out after 10s.

**Observed behavior:**
- `capture-quick --digital-mask 0xff` (digital-only): EP2 timeout, 0 bytes
- Mixed mode: DCM fails to lock first, so EP2 isn't tested properly
- StartSampling (0x01) returns ACK
- r0x00=0x01 (trigger) writes successfully
- All 12 capture registers verified via readback

**Possible causes:**
1. DCM must lock before any EP2 data flows (even digital-only may need it)
2. Missing GPIF streaming setup on the FX3 side
3. EP2 DMA not configured correctly by the firmware
4. Need the framed protocol handshake (0xA7/0xFE packets) that happens after firmware upload
5. Digital-only capture may still need some ADC/clock init

**Depends on:** TASK-0138 (DCM lock investigation) - if DCM locks, EP2 may start flowing.

**Test plan:** After power cycle + fresh firmware upload, test with full init sequence including framed handshake.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 EP2 returns non-zero bytes during a capture
- [x] #2 At least one capture mode (digital or analog) produces valid data
<!-- AC:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
**EP2 data flows after fixing bitstream variant and DCM cascade.**

Root cause was TASK-0138 (wrong bitstream + wrong DCM cascade). Once the correct bitstream (slx9-03) is used and DCM locks (r0x40=0xE0), EP2 returns ~49.8 MB in 2 seconds (24.3 MB/s sustained) during a digital capture with all 8 channels enabled.

The capture sequence works exactly as the pcap shows:
- Write 12 capture registers (verified via readback)
- StartSampling (0x01) returns [0x07, 0x00]
- Write r0x00=0x01 (trigger)
- EP2 bulk IN returns data continuously
- Write r0x00=0x00, StopStream to end capture

No framed protocol handshake needed. No GPIF setup beyond the standard capture registers.
<!-- SECTION:FINAL_SUMMARY:END -->
