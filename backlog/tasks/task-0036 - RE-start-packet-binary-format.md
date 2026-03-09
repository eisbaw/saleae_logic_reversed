---
id: TASK-0036
title: RE start packet binary format
status: Done
assignee: []
created_date: '2026-03-09 00:38'
updated_date: '2026-03-09 06:38'
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

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Reversed the binary layout of the start packet that precedes digital capture data in the EP2 IN bulk transfer stream.

Key findings:
- Every data chunk begins with a 4-byte little-endian header word containing: channel (bits 7:0), software flag (bit 8), is_start discriminator (bit 9), and config_id (bits 31:16)
- When is_start is set (bit 9 = 1), two additional fields follow: a 32-bit timestamp and a 64-bit time value, for 16 bytes total
- The parsing function at VA 0x2d41c00 was fully traced via disassembly: reads header, extracts bitfields via shift/mask, conditionally reads timestamp and time fields
- Up to 4 channels supported; per-channel state tracks whether start packet has been received (stride 0x30 bytes per channel)
- 4-byte alignment padding applied after header/start packet fields
- A separate per-packet stats parser at VA 0x2d41510 handles different metadata appended after sample data

Documentation: re/notes/start-packet-format.md
<!-- SECTION:FINAL_SUMMARY:END -->
