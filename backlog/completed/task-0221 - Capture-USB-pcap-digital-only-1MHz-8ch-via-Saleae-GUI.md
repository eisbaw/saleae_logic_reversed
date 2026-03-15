---
id: TASK-0221
title: 'Capture USB pcap: digital-only 1MHz 8ch via Saleae GUI'
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
  - re/notes/fpga-register-map.md
  - crates/saleae-driver/src/capture.rs
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Capture a usbmon pcap of the Saleae Logic 2 GUI performing a digital-only capture at 1 MHz with all 8 channels enabled on the Logic Pro 8.

**Why**: We only have ONE pcap reference (12.5 MHz mixed 8d+8a). The `capture-digital` CLI subcommand fails with 0 bytes from EP2 because we don't know the correct register values for digital-only mode. A pcap of the GUI doing the same operation will reveal the exact register configuration.

**Procedure**:
1. Start usbmon capture: `just usb-capture digital-1mhz-8ch 120`
2. Launch Saleae Logic 2 GUI: `saleae-logic-2`
3. Configure: 8 digital channels, 0 analog channels, 1 MHz sample rate
4. Run a 5-second capture
5. Stop the capture, stop usbmon
6. Decode EP1 commands from the pcap using `re/scripts/decode-pcap-init.py`
7. Extract the WriteRegisters for SetCaptureParameters (the 12-register batch)
8. Document the register values in `re/captures/` companion .md file

**Key registers to extract**: r0x00, r0x06, r0x08, r0x0a, r0x0b, r0x0c, r0x0d, r0x0e, r0x12, r0x13, r0x14, r0x1a

Compare with our computed values from `compute_capture_registers()` to find the discrepancy.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 pcap captured and saved to re/captures/
- [ ] #2 EP1 SetCaptureParameters registers decoded and documented
- [ ] #3 Register values compared with compute_capture_registers() output for same config
<!-- AC:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Captured digital-only 1MHz 8ch via Saleae GUI automation + usbmon. Pcap: re/captures/gui_pcaps/usbmon4_digital_1mhz_8ch_20260315_210312.pcap (2.1MB). Decoded registers: r0x00=0x00, r0x08=0x00, r0x06=0xff, r0x0a=0x00, r0x0b=0x64, r0x0c=0x00, r0x1a=0x00, r0x0d=0x00, r0x0e=0x08, r0x12=0x04, r0x13=0x00, r0x14=0xff
<!-- SECTION:FINAL_SUMMARY:END -->
