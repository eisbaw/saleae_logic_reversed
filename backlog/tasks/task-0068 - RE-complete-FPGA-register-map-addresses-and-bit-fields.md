---
id: TASK-0068
title: RE complete FPGA register map (addresses and bit fields)
status: Done
assignee: []
created_date: '2026-03-10 12:23'
updated_date: '2026-03-10 15:41'
labels:
  - re
  - ghidra
  - fpga
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Expand FPGA register map beyond what TASK-0031 documented. Current coverage ~60%. Need to decompile:

- All FpgaDeviceFeatures virtual method implementations (per device variant)
- CalculateDigitalConfigurationAddress_DigitalDerived(channel_index) -- full formula
- CalculateDigitalConfigurationAddress_SmartCable(port, sub_channel)
- Sample rate configuration registers
- Capture start/stop control registers
- Overflow detection registers
- Channel enable/disable registers
- Analog MUX selection registers
- SetDigitalDerivedVoltageThresholds register writes

Cross-reference with existing doc/protocol/fpga-registers.md and validate against pcap register write sequences.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Decompiled 59 FPGA register functions. Documented both interfaces: MsoFpga (16-bit addr, 32-bit value, registers 0x00-0x9c) and FpgaDeviceFeatures (8-bit addr via EP1 opcode 0x80/0x81). Extracted CalculateDigitalConfigurationAddress formulas, SetField bit-packing instantiations, voltage threshold config, ADC register tunnel, and init sequences. Rewrote doc/protocol/fpga-registers.md and re/notes/fpga-register-map.md. Created DecompileFpgaRegisters.java script.
<!-- SECTION:FINAL_SUMMARY:END -->
