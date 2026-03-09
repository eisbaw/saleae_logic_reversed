---
id: TASK-0009
title: Extract embedded firmware and bitstream blobs from libgraph_server_shared.so
status: Done
assignee: []
created_date: '2026-03-08 22:43'
updated_date: '2026-03-09 01:37'
labels: []
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Use binwalk entropy analysis and signature scanning to locate embedded firmware images and FPGA bitstreams within the 48MB libgraph_server_shared.so.

Hypothesis: firmware and bitstream images are embedded as static data blobs. The 10 distinct bitstream CRCs suggest at least 10 different bitstream variants.

Also check:
- resources/linux-x64/ directory for standalone firmware files
- app.asar for any embedded binary data

For each extracted blob:
- Record extraction offset, size, SHA-256
- Identify format (Cypress FX3 firmware, Xilinx .bit, Altera .rbf, Lattice bitstream)
- Save to firmware/ directory (gitignored)
- Document extraction method in re/notes/firmware-extraction.md
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Extracted 28 firmware/bitstream blobs from libgraph_server_shared.so:

- 3 FX3 firmware images (B2 wrapper + Pro 8 + Pro 16, 239 KB total)
- 4 Intel HEX firmware images (FX2/FX2LP legacy devices, 81 KB total)
- 2 Xilinx 7-series bitstreams (IDCODE 0x04AC2093, 9.6 MB total)
- 9 Xilinx Spartan-6 bitstreams (XC6SLX9 + XC6SLX16, 3.4 MB total)
- 10 Lattice ECP5 bitstreams (4 FPGA variants, 3.0 MB total)

All blobs validated: CY magic on FX3, EOF records on IHex, sync words on Xilinx, Lattice headers with CRC fields. SHA-256 hashes recorded.

Files added:
- re/scripts/extract-blobs.py -- extraction and validation script
- re/notes/firmware-extraction.md -- documentation with full manifest
- justfile recipe: extract-blobs
<!-- SECTION:FINAL_SUMMARY:END -->
