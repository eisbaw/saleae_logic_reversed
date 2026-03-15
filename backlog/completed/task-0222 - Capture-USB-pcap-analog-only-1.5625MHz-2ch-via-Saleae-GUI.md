---
id: TASK-0222
title: Capture USB pcap analog-only 1.5625MHz 2ch via Saleae GUI
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
Capture a usbmon pcap of the Saleae Logic 2 GUI performing an analog-only capture at 1 MHz with 2 analog channels on the Logic Pro 8.

**Why**: The `capture-analog` CLI subcommand fails with 0 bytes from EP2. We need to see how the official software configures registers for analog-only mode, especially:
- What digital channel mask is used (0x00? or does it always enable some?)
- What r0x0d/r0x0e mix ratios look like with no digital channels
- Whether any additional registers are written beyond the standard 12

**Procedure**:
1. Start usbmon capture: `just usb-capture analog-1mhz-2ch 120`
2. Launch Saleae Logic 2 GUI
3. Configure: 0 digital channels, 2 analog channels (ch0, ch1), 1 MHz sample rate
4. Run a 5-second capture
5. Stop, decode EP1, extract register config
6. Document in companion .md

**Note**: The GUI may not allow 0 digital channels -- document what the minimum is.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 pcap captured and saved to re/captures/
- [ ] #2 EP1 SetCaptureParameters registers decoded
- [ ] #3 Documented whether GUI allows 0 digital channels
- [ ] #4 Register values compared with compute_capture_registers() for analog-only config
<!-- AC:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Captured analog-only 1.5625MHz 2ch (1MHz not a valid analog rate). Pcap: re/captures/gui_pcaps/usbmon4_analog_1_5mhz_2ch_20260315_210644.pcap (9.5MB). Decoded registers: r0x00=0x00, r0x08=0x03, r0x06=0x00, r0x0a=0x20, r0x0b=0x00, r0x0c=0x00, r0x1a=0x00, r0x0d=0x06, r0x0e=0x00, r0x12=0x01, r0x13=0x00, r0x14=0x56
<!-- SECTION:FINAL_SUMMARY:END -->
