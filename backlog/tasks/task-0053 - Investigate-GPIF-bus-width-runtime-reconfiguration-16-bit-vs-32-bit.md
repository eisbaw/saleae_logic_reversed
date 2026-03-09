---
id: TASK-0053
title: Investigate GPIF bus width runtime reconfiguration (16-bit vs 32-bit)
status: To Do
assignee: []
created_date: '2026-03-09 08:13'
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
