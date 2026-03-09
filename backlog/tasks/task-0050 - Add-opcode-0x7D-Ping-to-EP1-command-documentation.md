---
id: TASK-0050
title: Add opcode 0x7D (Ping) to EP1 command documentation
status: Done
assignee: []
created_date: '2026-03-09 06:12'
labels:
  - protocol
  - documentation
dependencies: []
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
New opcode 0x7D discovered during TASK-0028 analysis. It is a firmware liveness check: send [0x7D, test_byte], firmware returns complement of test_byte. Used by TestFirmwareResponsivnessAndReseed before each capture. Already added to doc/protocol/ep1-commands.md and re/notes/ep1-command-protocol.md. Needs USB capture validation.
<!-- SECTION:DESCRIPTION:END -->
