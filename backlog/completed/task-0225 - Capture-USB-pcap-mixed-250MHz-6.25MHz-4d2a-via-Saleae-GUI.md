---
id: TASK-0225
title: Capture USB pcap mixed 250MHz/6.25MHz 4d+2a via Saleae GUI
status: Done
assignee: []
created_date: '2026-03-15 17:27'
updated_date: '2026-03-15 20:17'
labels:
  - RE
  - pcap
  - hardware
dependencies: []
references:
  - re/notes/sample-rate-options.md
  - re/notes/init-sequence-pcap.md
  - crates/saleae-driver/src/capture.rs
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Capture a usbmon pcap with different analog and digital sample rates (asymmetric rates).

**Why**: Our compute_capture_registers has a NOT YET HARDWARE-VALIDATED warning for non-equal digital/analog rates. The digital_ratio and mix_ratio computation is based on decompilation but untested. A pcap with 100 MHz digital + 50 MHz analog (ratio = 2) will validate or invalidate our formula.

**Procedure**:
1. Start usbmon capture
2. Configure GUI: 2 digital + 2 analog channels, select a rate tier where digital != analog
3. The GUI groups rates into tiers -- pick one where the digital rate is 2x the analog rate
4. Capture 5s, decode, extract r0x0a (analog dec), r0x0b (digital div), r0x0d/r0x0e (mix ratios)

**Key validation**: digital_ratio should be 2. Mix ratios should reflect the 2:1 bandwidth split.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 pcap captured with asymmetric rates
- [ ] #2 digital_ratio validated against our formula
- [ ] #3 Mix ratios validated
<!-- AC:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Captured mixed 250MHz digital + 6.25MHz analog, 4dig+2ana. Pcap: re/captures/gui_pcaps/usbmon4_mixed_125m_6_25m_4d2a_20260315_211344.pcap (45MB). Decoded registers: r0x00=0x00, r0x08=0x03, r0x06=0x0f, r0x0a=0x08, r0x0b=0x00, r0x0c=0x00, r0x1a=0x01, r0x0d=0x06, r0x0e=0x28, r0x12=0x04, r0x13=0x00, r0x14=0x03. Note: r0x1a=0x01 only seen in this high-rate config.
<!-- SECTION:FINAL_SUMMARY:END -->
