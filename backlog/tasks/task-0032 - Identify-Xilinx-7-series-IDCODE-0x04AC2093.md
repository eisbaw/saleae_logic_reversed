---
id: TASK-0032
title: Identify Xilinx 7-series IDCODE 0x04AC2093
status: To Do
assignee: []
created_date: '2026-03-09 00:21'
labels:
  - xilinx
  - fpga
  - identification
dependencies: []
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Two large (~4.5MB) bitstreams embedded in the binary use IDCODE 0x04AC2093. This is a valid Xilinx manufacturer code (0x049) with 7-series config protocol, but the part code 0x4AC2 does not appear in publicly documented Xilinx IDCODE lists. May be a custom part, NDA-restricted, or from Xilinx BSDL files not in public docs. Check if this could be a Zynq or Artix variant. These bitstreams may correspond to an early Logic Pro hardware revision.
<!-- SECTION:DESCRIPTION:END -->
