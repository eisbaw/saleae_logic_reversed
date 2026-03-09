---
id: TASK-0036
title: RE start packet binary format
status: To Do
assignee: []
created_date: '2026-03-09 00:38'
labels:
  - re
  - data-format
dependencies:
  - TASK-0011
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Reverse engineer the exact binary layout of the "start packet" that precedes digital data.

Known fields from log string:
- ch (channel number)
- timestamp (device timestamp)
- time (host time)  
- config_id (configuration ID)

Need to determine:
- Packet header magic bytes (if any)
- Field sizes and positions
- Byte ordering (little-endian expected on USB)
- How the receiver distinguishes start packets from data packets

Method: Ghidra analysis of the code around "Got start packet" and "Expected start packet" string references. Also USB capture analysis.
<!-- SECTION:DESCRIPTION:END -->
