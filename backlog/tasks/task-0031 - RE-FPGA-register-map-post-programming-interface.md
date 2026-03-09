---
id: TASK-0031
title: RE FPGA register map (post-programming interface)
status: Done
assignee: []
created_date: '2026-03-09 00:21'
updated_date: '2026-03-09 03:27'
labels:
  - fpga
  - registers
  - protocol
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Map the FPGA register addresses and their functions. The host uses 16-bit addressed, 32-bit valued registers via FpgaDeviceFeatures::ReadRegisters/WriteRegisters. Log format: "FPGA WriteRegister 0x{:04x} = 0x{:08x} ({})". Also includes CalculateDigitalConfigurationAddress functions for smart cable support. This register map is needed for capture configuration, ADC control, and trigger setup.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Mapped 39 FPGA register addresses from libgraph_server_shared.so v2.4.40 via binary analysis.

Method: Found "FPGA WriteRegister" format string at offset 0x0041fd7b, scanned .text for RIP-relative LEA instructions referencing it (57 xrefs, all in VA range 0x2d62000-0x2d68000 = MsoFpga code region), extracted register addresses from nearby movw $imm16 instructions (49 total).

Wire protocol: 16-bit addresses, 32-bit values, big-endian on wire (ROL byte-swap). Transport: USB bulk EP1 OUT (write) and EP1 IN (read) via FX3 microcontroller.

Register groups:
- 0x0004-0x0008: SPI bridge to ADC, DDR/JESD status
- 0x0020-0x0024: GPIF data interface control
- 0x0030-0x0034: Test data generation
- 0x0040-0x0068: Capture control, scope config IDs, analog channel config (14 registers)
- 0x0070-0x009C: Trigger configuration and software trigger (12 registers)
- 0x00E0, 0x00FC: Init config, JESD BER test
- 0x0104: Digital voltage threshold
- 0x0170-0x017C: Per-channel digital config (address = 0x170 + ch*4)
- 0x01C0-0x01C4: Commit/bank control, digital test pattern

Key bit fields decoded:
- Reg 0x0008 bits [4:3]: DDR ready (locked + calibrated)
- Reg 0x0004 bit 23: SPI R/W flag, bits [31:24] must be zero for writes
- Digital glitch filter: pulse width must be 5 or multiple of 10

Post-programming init sequence: COMMIT_CONTROL -> CAPTURE_CONTROL -> INIT_CONFIG -> GPIF_FLUSH_TIMEOUT -> GPIF_ENABLE -> [Lattice: JESD PHY reset + DDR ready poll]

Smart cable registers (0x0180+) confirmed MSO-only, out of scope.

Deliverables:
- doc/protocol/fpga-registers.md (confirmed findings)
- re/notes/fpga-register-map.md (detailed working notes with disassembly)
- re/ghidra/scripts/ExtractFpgaRegisters.py (automated extraction script)
<!-- SECTION:FINAL_SUMMARY:END -->
