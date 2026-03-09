---
id: TASK-0047
title: Investigate FX3 firmware command protocol on EP1 OUT/IN
status: To Do
assignee: []
created_date: '2026-03-09 02:33'
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
