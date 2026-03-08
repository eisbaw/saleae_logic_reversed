---
id: TASK-0009
title: Extract embedded firmware and bitstream blobs from libgraph_server_shared.so
status: To Do
assignee: []
created_date: '2026-03-08 22:43'
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
