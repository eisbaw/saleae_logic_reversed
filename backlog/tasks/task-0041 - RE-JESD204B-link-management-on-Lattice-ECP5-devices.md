---
id: TASK-0041
title: RE JESD204B link management on Lattice ECP5 devices
status: To Do
assignee: []
created_date: '2026-03-09 00:50'
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
