---
id: TASK-0053
title: Investigate GPIF bus width runtime reconfiguration (16-bit vs 32-bit)
status: Done
assignee: []
created_date: '2026-03-09 08:13'
updated_date: '2026-03-09 09:04'
labels:
  - firmware
  - gpif
  - capture
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The GPIF II firmware initializes with a 32-bit data bus, but the host-side code references both 16-bit and 32-bit GPIF widths (see data-interleaving.md: "No valid analog and gpif width combo given"). The host likely reconfigures the GPIF bus width at runtime via the WriteConfigurationData command (opcode 0x7F).

Investigate:
1. Capture USB traffic during a capture start with different channel configurations to see if GPIF_BUS_CONFIG is rewritten
2. Find the host-side code that selects 16 vs 32-bit GPIF width
3. Determine which channel configurations trigger 16-bit vs 32-bit mode

Starting points:
- re/notes/gpif-state-machine.md (GPIF analysis)
- re/notes/data-interleaving.md (bus width references)
- re/notes/ep1-command-protocol.md (WriteConfigurationData 0x7F)
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Investigated GPIF bus width (16-bit vs 32-bit) selection logic.\n\nKey findings:\n1. The FX3 GPIF II physical bus is always 32-bit (GPIF_BUS_CONFIG = 0x6C). Only 2 references to this register exist in the FX3 firmware, both in init code (CyU3PGpifLoad). No runtime reconfiguration occurs.\n2. The host-side \"GPIF width\" is a logical concept -- how many valid bytes per physical bus word. The FPGA outputs data in 16-bit or 32-bit logical mode depending on enabled analog channels.\n3. The error \"No valid analog and gpif width combo given\" appears only in LogicStudentDevice.cpp (Logic 4, which is FX2LP-based, not FX3). Logic 4's physical GPIF bus CAN switch between 8/16-bit.\n4. ContractReadData strips padding bytes when logical width < physical bus width. ExpandWriteData adds padding for writes.\n5. Pro 8 and Pro 16 have identical GPIF firmware config (zero register differences).\n\nDocumented in re/notes/gpif-bus-width.md"
<!-- SECTION:FINAL_SUMMARY:END -->
