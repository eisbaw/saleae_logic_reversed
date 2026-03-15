---
id: TASK-0223
title: Capture USB pcap mixed 50MHz/12.5MHz 4d+2a via Saleae GUI
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
  - re/captures/usbmon4-ep1-decoded.txt
  - re/notes/init-sequence-pcap.md
  - crates/saleae-driver/src/capture.rs
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Capture a usbmon pcap of the Saleae Logic 2 GUI performing a mixed capture at 12.5 MHz with 4 digital + 2 analog channels on the Logic Pro 8.

**Why**: Our only pcap uses 8d+8a at 12.5 MHz. A different channel count will reveal how mix ratios (r0x0d, r0x0e) and channel masks change. This is critical for understanding the GPIF packet layout formula.

**Procedure**:
1. Start usbmon: `just usb-capture mixed-12.5mhz-4d2a 120`
2. Launch Saleae Logic 2 GUI
3. Configure: digital channels 0-3, analog channels 0-1, 12.5 MHz
4. Run a 5-second capture
5. Decode and document

**Expected differences from existing pcap**:
- r0x08 (digital mask): 0x0f instead of 0xff
- r0x06 (analog mask): 0x03 instead of 0xff
- r0x0d/r0x0e (mix ratios): different values
- r0x14 (GPIF bus width): may change
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 pcap captured and saved to re/captures/
- [ ] #2 EP1 registers decoded and documented
- [ ] #3 Mix ratio values r0x0d/r0x0e extracted and compared with compute_mix_ratios()
<!-- AC:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Captured mixed 50MHz digital + 12.5MHz analog, 4dig+2ana. Pcap: re/captures/gui_pcaps/usbmon4_mixed_50m_12_5m_4d2a_20260315_211221.pcap (19MB). Decoded registers: r0x00=0x00, r0x08=0x03, r0x06=0x0f, r0x0a=0x04, r0x0b=0x02, r0x0c=0x00, r0x1a=0x00, r0x0d=0x06, r0x0e=0x04, r0x12=0x04, r0x13=0x00, r0x14=0x07
<!-- SECTION:FINAL_SUMMARY:END -->
