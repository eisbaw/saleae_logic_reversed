---
id: TASK-0041
title: RE JESD204B link management on Lattice ECP5 devices
status: Done
assignee: []
created_date: '2026-03-09 00:50'
updated_date: '2026-03-09 08:25'
labels:
  - fpga
  - jesd
  - lattice
dependencies: []
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Detailed analysis of the JESD204B link initialization and management between ADS528x ADC and Lattice ECP5 FPGA.

Known:
- JesdStatus struct: link_up, sync_status, realigned, aligned, spll_locked, cpll_locked
- Bitslip detection for ADC1 and ADC2 with retry logic
- PLL lock verification (system PLL, PCLK PLL, lane PLLs, sample PLL)
- FPGA register interface for JESD status readback

Targets:
- CommonDeviceInit path for Lattice devices
- JESD register addresses
- Bitslip correction algorithm
- PLL configuration sequence

Discovered during TASK-0012.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Completed JESD204B link management analysis for Lattice ECP5 devices.

Searched the binary for all JESD, bitslip, PLL, SERDES, and DDR-related strings. Cross-referenced findings with existing docs (adc-initialization.md, fpga-bitstream-protocol.md, fpga-registers.md, fpga-register-map.md).

Key findings documented in re/notes/jesd204b-link.md:

- Full JESD init sequence: ADC power-on -> SERDES reset -> PLL lock verification (system, PCLK, lane 0-3, lane 4-7, sample PLLs) -> bitslip detection/correction -> DDR ready wait
- JesdStatus struct fields: link_up, sync_status, realigned, aligned, spll_locked, cpll_locked
- Bitslip error handling with retry loop, dedicated BitslipError exception type, and shutdown cleanup
- JESD BER test facility (Start/Get/Stop functions, register 0x00FC)
- Two mysterious FPGA signal names: JESD_ASSERT, JESD_ASSH (likely SYNC~ control)
- DDR controller dependency on JESD link (must lock JESD before DDR calibrates)
- Comparison table: Lattice JESD204B vs Xilinx parallel LVDS interface

Open items requiring Ghidra decompilation: register 0x0008 exact bit field layout, retry count constants, JESD_ASSERT/ASSH field positions, ADC chip count per device.
<!-- SECTION:FINAL_SUMMARY:END -->
