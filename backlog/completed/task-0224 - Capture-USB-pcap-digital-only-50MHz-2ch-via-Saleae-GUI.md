---
id: TASK-0224
title: Capture USB pcap digital-only 50MHz 2ch via Saleae GUI
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
  - re/notes/init-sequence-pcap.md
  - crates/saleae-driver/src/capture.rs
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Capture a usbmon pcap at a higher sample rate: digital-only 25 MHz with 2 channels.

**Why**: Tests digital divider computation at a different rate. digital_divider = 200MHz / 25MHz = 8 = 0x08. Also tests reduced channel count (2 vs 8).

**Procedure**:
1. Start usbmon: `just usb-capture digital-25mhz-2ch 120`
2. Configure GUI: digital ch0+ch1, 0 analog, 25 MHz
3. Capture 5s, decode, document
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 pcap captured
- [ ] #2 Register values extracted
- [ ] #3 Digital divider value confirmed
<!-- AC:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Captured digital-only 50MHz 2ch. Pcap: re/captures/gui_pcaps/usbmon4_digital_50mhz_2ch_20260315_211304.pcap (15MB). Decoded registers: r0x00=0x00, r0x08=0x00, r0x06=0x03, r0x0a=0x00, r0x0b=0x02, r0x0c=0x00, r0x1a=0x00, r0x0d=0x00, r0x0e=0x02, r0x12=0x04, r0x13=0x00, r0x14=0x20
<!-- SECTION:FINAL_SUMMARY:END -->
