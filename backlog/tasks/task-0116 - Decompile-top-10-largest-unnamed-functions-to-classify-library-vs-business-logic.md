---
id: TASK-0116
title: >-
  Decompile top 10 largest unnamed functions to classify library vs business
  logic
status: To Do
assignee: []
created_date: '2026-03-11 07:54'
labels:
  - re
  - ghidra
  - triage
dependencies: []
references:
  - re/ghidra/scripts/LargeFunctions.java
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The binary has 7,032 unnamed functions over 500 bytes. The top 10 range from 15k to 45k bytes each. Decompiling these will determine whether they're RocksDB/library internals (ignore) or Saleae business logic (investigate).

Top 5 ELF VAs to check:
1. 0x02a3a820 (44,952 bytes)
2. 0x01f6d820 (41,864 bytes)
3. 0x01f379c0 (34,211 bytes)
4. 0x026daa90 (34,012 bytes)
5. 0x022c7120 (29,844 bytes)

Write a Ghidra script to decompile these and check for string references that identify the subsystem. If they reference RocksDB, Tcl, Python, or X11 -- label them in Ghidra and move on.
<!-- SECTION:DESCRIPTION:END -->
