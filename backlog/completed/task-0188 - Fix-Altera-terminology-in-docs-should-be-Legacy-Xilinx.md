---
id: TASK-0188
title: Fix "Altera" terminology in docs -- should be "Legacy Xilinx"
status: Done
assignee: []
created_date: '2026-03-13 05:36'
updated_date: '2026-03-13 07:54'
labels:
  - docs
  - protocol
  - fpga
dependencies: []
references:
  - doc/protocol/capture-constants.md
  - doc/protocol/fpga-registers.md
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
capture-constants.md and fpga-registers.md use "Altera FPGAs" but there are NO Altera bitstreams in Saleae Logic 2. Only Xilinx (Spartan-6, 7-Series) and Lattice ECP5 exist. The "Altera" references should be "Legacy Xilinx" or "FX2LP-era Xilinx." Also add register 0x15 (bank power control) to fpga-registers.md -- it's used in capture-sequence.md and code but not documented in the register map.
<!-- SECTION:DESCRIPTION:END -->
