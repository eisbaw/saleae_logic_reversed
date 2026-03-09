---
id: TASK-0032
title: Identify Xilinx 7-series IDCODE 0x04AC2093
status: Done
assignee: []
created_date: '2026-03-09 00:21'
updated_date: '2026-03-09 08:20'
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

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
IDCODE 0x04AC2093 identified as **Xilinx XCAU15P (Artix UltraScale+)**, not a 7-series part.\n\nKey findings:\n- Part code 0x4AC2 falls outside the 7-series range (0x3620-0x37C8)\n- Identified via openFPGALoader device database (src/part.hpp)\n- Two ~4.5 MB XCAU15P bitstreams are embedded in the binary (at offsets 0x007D9010 and 0x00C48A20)\n- The remaining 9 bitstreams are Spartan-6: 6x XC6SLX4 (~333 KB each) and 3x XC6SLX9 (~453 KB each)\n- JESD204B strings in the binary confirm GTH transceiver usage, which requires the XCAU15P\n- XCAU15P likely used in newer Logic Pro 8/16 hardware revisions (vs Spartan-6 in older revisions)\n- PRD should be updated: not all FX3-based devices use Spartan-6\n\nDocumented in re/notes/xilinx-7series-idcode.md with complete embedded bitstream map.
<!-- SECTION:FINAL_SUMMARY:END -->
