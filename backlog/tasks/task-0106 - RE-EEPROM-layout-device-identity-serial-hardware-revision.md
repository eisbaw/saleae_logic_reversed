---
id: TASK-0106
title: 'RE EEPROM layout -- device identity, serial, hardware revision'
status: Done
assignee: []
created_date: '2026-03-10 23:14'
updated_date: '2026-03-11 00:59'
labels:
  - re
  - ghidra
  - eeprom
  - hardware
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The device EEPROM stores identity information read during CommonDeviceInit (TASK-0060). We have partial knowledge but the full EEPROM layout is not documented.

Known from prior work:
- Device ID string read via EP1 opcode 0x8B
- Hardware revision string
- Scratch register read/write (opcode 0x7D)
- ADC type may be per-revision (TASK-0073)

Investigation steps:
1. Search binary for "EEPROM", "eeprom", "ReadEeprom", "WriteEeprom" strings
2. Find EP1 opcodes that read EEPROM data
3. Document EEPROM address map (what's at each offset)
4. Determine if calibration data is stored in EEPROM or only server-side
5. Check if hardware revision determines FPGA bitstream selection

This feeds into the calibration download task and multi-device support.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
FX3 EEPROM layout documented:\n\nAddress map:\n- 0x00-0x07: Reserved (wiped by WipeEeprom)\n- 0x08-0x0F: Device ID (uint64 LE, 8 bytes)\n- 0x10-0x1F: Unknown (wiped)\n- 0x20-0x25: Hardware revision string 'REVx.y' (6 bytes)\n- 0x26-0xFFF: Unused/test area\n\nNo calibration data in EEPROM (confirmed server-side by TASK-0101). Hardware revision determines ADC type and FPGA bitstream selection.\n\nEP1 opcodes: 0x8B = read device ID, 0x07 = read version/revision, 0x7D = scratch register.\n\nWipeEeprom clears offsets 0x00-0x1F (32 bytes). EEPROM accessed via I2C through FPGA bridge.\n\nAnalysis: re/notes/eeprom-layout.md (472 lines)"
<!-- SECTION:FINAL_SUMMARY:END -->
