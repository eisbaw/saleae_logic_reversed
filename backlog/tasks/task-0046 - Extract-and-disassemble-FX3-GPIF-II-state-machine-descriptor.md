---
id: TASK-0046
title: Extract and disassemble FX3 GPIF II state machine descriptor
status: Done
assignee: []
created_date: '2026-03-09 02:33'
updated_date: '2026-03-09 08:06'
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

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Extracted and fully decoded the GPIF II state machine descriptor from the FX3 firmware (fw-fx3-pro8.img).

Key findings:
- 3-state GPIF II state machine implementing a synchronous 32-bit parallel slave FIFO interface
- FX3 is clock master, 50-100 MHz, SDR mode
- Simple 2-phase handshake: CTRL[0] from FPGA toggles to clock data across the 32-bit bus
- State 0/2: Sample input data when CTRL[0] asserts
- State 1: Push sampled data to DMA write queue when CTRL[0] deasserts
- 4 DMA threads enabled with 64-byte burst, THREAD_IN_STATE mode for dynamic thread selection
- CTRL[2] outputs DMA watermark flag to FPGA for backpressure
- Configuration is IDENTICAL between Pro 8 and Pro 16 firmware
- 16-bit mode is likely a runtime reconfiguration via WriteConfigurationData (0x7F)

Deliverables:
- re/scripts/parse-gpif.py: Complete parser for CyU3PGpifConfig_t from firmware binary
- re/notes/gpif-state-machine.md: Full analysis with state diagram, register decode, cross-references
<!-- SECTION:FINAL_SUMMARY:END -->
