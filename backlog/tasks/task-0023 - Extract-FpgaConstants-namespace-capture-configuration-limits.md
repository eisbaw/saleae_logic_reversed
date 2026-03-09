---
id: TASK-0023
title: Extract FpgaConstants namespace -- capture configuration limits
status: Done
assignee: []
created_date: '2026-03-08 22:53'
updated_date: '2026-03-09 03:01'
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

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Extracted all FpgaConstants, MsoConstants, and MsoDeviceConstants from libgraph_server_shared.so v2.4.40.\n\nCompile-time constants (from CMP instruction immediates in ValidateConfig at VA 0x02d64640):\n- MinSamplesPerPacket_Xilinx = 256, MaxSamplesPerPacket_Xilinx = 7680\n- MinSamplesPerPacket_Altera = 64, MaxSamplesPerPacket_Altera = 6016\n- SamplesPerPacketMultiple_WithStats = 256 (conditional on stats flag)\n- MinTriggerBufferSamples = 8\n- MaxTriggerBufferSamples_Xilinx = 0x3FFFC000, MaxTriggerBufferSamples_Altera = 0x2FFFC000\n- MaxScopeChannels = 4, TotalAfeConfigurations = 66\n\nRuntime per-device constants (MsoDeviceConstants struct, 0xA8 bytes, 21 JSON-serialized fields):\n- analogLimit_* values are NOT compile-time constants; they are sent as JSON from native code to Electron frontend per-device.\n\nJS bundle analysis: downsample ratio quantization, available rates [50..31250] Hz, trigger buffer clamping.\n\nDeliverables:\n- doc/protocol/capture-constants.md (verified documentation)\n- re/notes/capture-constants.md (working notes with disassembly details)\n- re/ghidra/scripts/ExtractConstants.py (automated extraction script)\n\nCommit: a68b6c6
<!-- SECTION:FINAL_SUMMARY:END -->
