---
id: TASK-0114
title: RE digital data compression format for USB 2.0 bandwidth
status: Done
assignee: []
created_date: '2026-03-11 05:59'
updated_date: '2026-03-11 06:48'
labels:
  - re
  - ghidra
  - digital
  - usb
dependencies: []
references:
  - re/notes/digital-capture-format.md
  - re/notes/withstats-format.md
  - re/notes/transfer-queue-sizing.md
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The PRD mentions "on-device data compression (USB 2.0 bandwidth constraint)" and strings reference streaming bandwidth limits. On USB 2.0, the ~40 MB/s bandwidth is insufficient for high sample rates, so the device likely uses RLE or similar compression for digital data.

Our digital capture implementation may only handle the uncompressed USB 3.0 format. Need to investigate:
1. Search for compression-related strings near digital capture code paths
2. Check if WithStats packet format includes compression flags
3. Decompile the digital data receive path for USB 2.0 vs 3.0 differences
4. Check if the FPGA does compression (register flags?) or if it's host-side
5. Look at Altera vs Xilinx min/max packet sizes (documented in PRD as different)

Strings to search: "compress", "rle", "run length", "packed", "WithStats", "hasLimitedBandwidth"

Reference: re/notes/digital-capture-format.md, re/notes/withstats-format.md
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
## Finding: No On-Device Compression\n\nExhaustive investigation confirms there is NO on-device data compression in the USB data path. The PRD hypothesis was incorrect.\n\n### How USB 2.0 Bandwidth is Handled\n\n1. **Sample rate limiting** -- GetInterfaceMaximumBitRate returns 300 Mbps for USB 2.0, 3 Gbps for USB 3.0. Sample rate/channel combinations are validated against this budget at configuration time.\n\n2. **Transfer size adaptation** -- ComputeTransferSize scales USB bulk transfer buffers (4KB-512KB) proportional to data rate. AdjustTransferSize aligns to 4KB (USB 2.0) or 16KB (USB 3.0) boundaries.\n\n3. **hasLimitedBandwidth flag** -- Boolean on device descriptor that restricts available sample rates in the UI.\n\n4. **12-bit ADC packing** -- Fixed encoding (not compression): 4 samples in 6 bytes or 8 samples in 12 bytes.\n\n### Key Constants Extracted\n- USB speed lookup table: Full Speed = 6 Mbps, High Speed = 300 Mbps, Super Speed = 3 Gbps\n- Altera: MinSamplesPerPacket=64, MaxSamplesPerPacket=6016\n- Xilinx: MinSamplesPerPacket=256, MaxSamplesPerPacket=7680\n- SamplesPerPacketMultiple_WithStats=256\n\n### Implication for Rust Implementation\nNo decompression logic needed. The driver must enforce bandwidth validation at configuration time to prevent FPGA FIFO overflows.\n\nWritten to: re/notes/digital-compression.md\nGhidra script: re/ghidra/scripts/DecompileCompression.java\nDecompiled output: re/ghidra/decompiled-compression.txt"
<!-- SECTION:FINAL_SUMMARY:END -->
