---
id: TASK-0047
title: Investigate FX3 firmware command protocol on EP1 OUT/IN
status: Done
assignee: []
created_date: '2026-03-09 02:33'
updated_date: '2026-03-09 04:20'
labels:
  - firmware
  - protocol
  - fx3
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The FX3 firmware contains a command processing loop (process_raw_command in CommandProcessing.c) that reads commands from EP1 OUT and sends responses via EP1 IN. The string "unrecognized command" confirms a dispatch table exists.

This is the primary communication channel between the host and the FX3 after firmware is running. Understanding the command opcodes and payload formats is essential for implementing the Rust driver.

Approach options:
1. USB capture analysis: capture traffic during device init, FPGA programming, and capture start/stop
2. Ghidra disassembly of the ARM9 firmware code around process_raw_command
3. Host-side analysis: trace the host code that constructs commands sent to EP1 OUT

The host-side strings like "Enabling GPIF", "Enabling hardware monitoring in firmware", "Enabling fan control in firmware" suggest specific command types.

See: re/notes/fx3-usb-events.md (Command protocol section)
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Completed comprehensive analysis of the FX3 EP1 command protocol through static binary analysis of libgraph_server_shared.so v2.4.40.\n\n## Key Findings\n\n### 12 Command Opcodes Identified\n\n| Opcode | Name | Purpose |\n|--------|------|---------|\n| 0x01 | COMMAND_START_READ | Start data streaming |\n| 0x02 | COMMAND_STOP_STREAM | Stop data streaming |\n| 0x22 | ReseedDevice | FX3 liveness test |\n| 0x55 | WriteEeprom | EEPROM write |\n| 0x7E | ResetFpga | FPGA reset before programming |\n| 0x7F | WriteConfigurationData | FPGA bitstream/config chunks |\n| 0x81 | ReadRegisters | FPGA register read (command-wrapped) |\n| 0x82 | ResetAndReEnumerate | Device reset |\n| 0x86 | StopDataPath | Stop FPGA data path |\n| 0x88 | ReadI2c | I2C tunnel read |\n| 0x89 | Device2_1 | Device info query |\n| 0x8A | CMD_ASSERT_TEST | Firmware assertion test |\n| 0x8B | GetDeviceInfo | Firmware build info |\n\n### Two Register Access Paths Discovered\n- Direct path (vtable[0x18]): no opcode, raw 2/6 byte packets (MsoFpga)\n- Command path (vtable[0x20]/[0x28]): 0x81 opcode prefix (FpgaDeviceFeatures)\n\n### WriteConfigurationData USB 2.0/3.0 Format Difference\n- USB 2.0: [0x7F] [len_8bit] [data...] (max 62 bytes)\n- USB 3.0: [0x7F] [len_lo] [len_hi] [data...] (max 509 bytes)\n\n## Deliverables\n- `doc/protocol/ep1-commands.md` -- confirmed protocol documentation\n- `re/notes/ep1-command-protocol.md` -- working analysis notes with full evidence\n- `re/ghidra/scripts/TraceEP1Commands.py` -- Ghidra script for automated call site analysis\n\n## Method\nRIP-relative LEA scanning of stripped x86-64 binary to find cross-references from code to string constants, followed by surrounding instruction analysis to extract MOV byte immediate values representing command opcodes and vtable call offsets.
<!-- SECTION:FINAL_SUMMARY:END -->
