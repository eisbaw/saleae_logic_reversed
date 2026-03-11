---
id: TASK-0114
title: RE digital data compression format for USB 2.0 bandwidth
status: Done
assignee: []
created_date: '2026-03-11 05:59'
updated_date: '2026-03-11 06:49'
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
No on-device compression exists. PRD hypothesis was wrong. Bandwidth handled by 5 mechanisms: (1) sample rate limiting via GetInterfaceMaximumBitRate (USB2=300Mbps, USB3=3Gbps), (2) transfer size adaptation (4KB-512KB proportional to rate), (3) hasLimitedBandwidth flag restricting UI options, (4) 12-bit ADC packing (1.5 bytes/sample, fixed not compressed), (5) LowSpeedDivider padding removal. Extracted FPGA packet size constants: Altera min=64/max=6016, Xilinx min=256/max=7680. No decompression needed in Rust driver.
<!-- SECTION:FINAL_SUMMARY:END -->
