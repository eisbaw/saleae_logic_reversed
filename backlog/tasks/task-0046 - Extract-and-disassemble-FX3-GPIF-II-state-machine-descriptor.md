---
id: TASK-0046
title: Extract and disassemble FX3 GPIF II state machine descriptor
status: To Do
assignee: []
created_date: '2026-03-09 02:33'
labels:
  - firmware
  - gpif
  - fx3
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The FX3 firmware loads a GPIF II state machine via CyU3PGpifLoad(). This descriptor defines the parallel interface timing between the FX3 and the FPGA. It is embedded as binary data in the firmware blob (not as strings). Extracting and disassembling this descriptor would reveal the exact bus timing protocol used for FPGA communication and sample data transfer.

The GPIF II descriptor format is documented in the Cypress FX3 SDK (GPIF II Designer output). The descriptor is a C struct (CyU3PGpifConfig_t) that can be parsed from the firmware binary.

Starting point: fw-fx3-pro8.img, look for the CyU3PGpifLoad call site and its data argument. The GPIF config data will be in Section 2 or Section 3 (SYSMEM).

See: re/notes/fx3-usb-events.md (GPIF references)
<!-- SECTION:DESCRIPTION:END -->
