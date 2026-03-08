---
id: TASK-0023
title: Extract FpgaConstants namespace -- capture configuration limits
status: To Do
assignee: []
created_date: '2026-03-08 22:53'
labels: []
dependencies:
  - TASK-0003
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Extract all constants from the FpgaConstants namespace found in string analysis.

Known constants (from assertion strings):
- FpgaConstants::MaxSamplesPerPacket_Altera
- FpgaConstants::MaxSamplesPerPacket_Xilinx
- FpgaConstants::MinSamplesPerPacket_Altera
- FpgaConstants::MinSamplesPerPacket_Xilinx
- FpgaConstants::SamplesPerPacketMultiple_WithStats

Also found analog-specific limits:
- analogLimit_MaxDownsampleRatio
- analogLimit_MaxHoldoffSamples
- analogLimit_MaxTimeoutSamples
- analogLimit_MaxTotalTriggerBufferLength
- analogLimit_MaxTriggerBufferIndex
- analogLimit_MinTotalTriggerBufferLength
- analogLimit_MinTriggerBufferIndex

These constants define valid capture configurations. Extract their actual values from Ghidra -- they are likely small integers loaded as immediates. High value, low effort.

Once extracted and verified, promote to doc/protocol/capture-constants.md.
<!-- SECTION:DESCRIPTION:END -->
