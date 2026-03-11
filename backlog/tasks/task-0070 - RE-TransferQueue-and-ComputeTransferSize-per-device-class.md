---
id: TASK-0070
title: RE TransferQueue and ComputeTransferSize per device class
status: Done
assignee: []
created_date: '2026-03-10 12:23'
updated_date: '2026-03-10 16:17'
labels:
  - re
  - ghidra
  - usb
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Reverse engineer the USB transfer queue sizing algorithm. TASK-0015 explored the class but didn't extract the sizing formulas. Need:

- Logic4::ComputeTransferSize
- Logic8::ComputeTransferSize
- ProLogic8::ComputeTransferSize
- ProLogic16::ComputeTransferSize
- How transfer size varies with sample rate, channel count, USB speed (2.0 vs 3.0)
- Queue depth (number of outstanding transfers)
- Relationship between transfer size and streaming throughput

Matters for USB throughput optimization -- wrong buffer sizes cause overflow or latency.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
ComputeTransferSize is a single shared implementation at ELF 0x1c50230, not per-device. Formula: round_up(bandwidth_bytes_per_sec / 240, block_size) where block_size=4096 (USB 2.0) or 16384 (USB 3.0). Capped at 1,638,400 bytes, overridable via SALEAE_USB_TRANSFER_LIMIT env var. Queue depth always 128. Per-device difference only in GetUsbSpeed: Logic 4/8 default USB 2.0, Pro 8/16 default USB 3.0. Created re/notes/transfer-queue-sizing.md and 2 Ghidra scripts.
<!-- SECTION:FINAL_SUMMARY:END -->
