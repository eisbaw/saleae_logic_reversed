---
id: TASK-0194
title: >-
  Analyze unprocessed pcap files (saleae-logic2-full-init.pcap,
  saleae-all-usb.pcap)
status: Done
assignee: []
created_date: '2026-03-13 05:37'
updated_date: '2026-03-13 08:21'
labels:
  - RE
  - captures
  - validation
dependencies: []
references:
  - re/captures/saleae-logic2-full-init.pcap
  - re/captures/saleae-all-usb.pcap
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Two pcap files in re/captures/ have never been analyzed: (1) saleae-logic2-full-init.pcap (184KB) -- full init sequence, may show different device variant or protocol differences. (2) saleae-all-usb.pcap (462KB) -- broad bus capture, may include EP2 streaming data. Also the 46GB usbmon0-20260312 capture may contain EP2 sample data not present in the EP1-only decoded files. No EP2 streaming data captures have been analyzed -- this blocks validation of the disaggregator against real device output.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Analyzed both pcap files in re/captures/:

**saleae-logic2-full-init.pcap** (184KB, 802 packets, 67.4s):
- USB capture of full Saleae Logic 2 startup with Logic Pro 8 (21a9:1005)
- Covers bootloader enumeration, re-enumeration, and FX3 firmware RAM upload
- Firmware: 119,776 bytes in 31 chunks across 4 memory regions (I-TCM + 3 System RAM segments)
- Entry point: 0x4000E020
- Cross-referenced against fw-fx3-pro8.img (48-byte header delta confirms completeness)
- Does NOT contain EP1 command traffic or EP2 streaming data
- Notable 16.9s app startup gap and 15s pre-upload gap

**saleae-all-usb.pcap** (462KB, 299 packets, 9.6s):
- MISNOMER: Actually a network capture (TCP/TLS over SLL), not USB
- 441 KB TLS upload from host to 160.79.104.10:443 during device init
- Small Cloudflare sessions (104.18.x.x, 172.66.x.x) likely telemetry/update checks
- All payload encrypted, no DNS/SNI captured

Commit: e444e29
<!-- SECTION:FINAL_SUMMARY:END -->
