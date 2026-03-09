---
id: TASK-0014
title: RE capture start/stop command sequence
status: Done
assignee: []
created_date: '2026-03-08 22:44'
updated_date: '2026-03-09 01:12'
labels: []
dependencies:
  - TASK-0006
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Reverse engineer the capture configuration and start/stop protocol.

Known from string/symbol analysis:
- "Cannot start capture while device is not in Ready state"
- "Capture parameter failed to download to device"
- "Failed to set capture settings"
- capturedInterval, captureLatestTime, captureStartTime
- Pre/post trigger buffer management
- (config.mPreTriggerLength + config.mPostTriggerLength) % SamplesPerPacketMultiple_WithStats == 0
- config.mSamplesPerChunk constraints per FPGA vendor

Investigate:
1. What USB transfers configure sample rate, channels, trigger?
2. What is the start-capture command sequence?
3. What is the stop-capture command sequence?
4. How are capture parameters encoded? (struct layout)
5. What constraints exist on valid configurations?
6. How does the host know capture has started/stopped successfully?
7. How does pre-trigger buffering work with software triggers?
<!-- SECTION:DESCRIPTION:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Completed 2026-03-09. Searched the real binary for capture-related strings, cross-referenced with all existing notes (ipc-analysis, digital-capture-format, analog-capture-pipeline, device-state-machine, libusb-boundaries). Mapped the full capture start/stop sequence from IPC through device layer to USB. Created Ghidra script TraceCaptureSequence.py. Working notes in re/notes/capture-start-stop.md. Promoted confirmed findings to doc/protocol/capture-sequence.md.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
# TASK-0014: RE capture start/stop command sequence\n\n## Findings\n\n### Capture Start Sequence (7 steps)\n1. HandleStartCapture: validates DSReady state, registers 3 lambda callbacks ($7 data, $8 error, $9 completion), emits SamplingConfigurationData and DisaggregatorSettingsData\n2. SetBankPower: enables IO banks, waits for PLL lock\n3. SetCaptureParameters: writes config blob to FPGA via WriteConfigurationData on EP1 OUT (62 byte chunks USB2.0, 509 byte chunks USB3.0)\n4. TestFirmwareResponsivnessAndReseed: verifies FX3 is alive\n5. ComputeTransferSize: calculates optimal USB buffer (virtual, per device)\n6. SendStartCommand: tells FX3 to begin data flow\n7. ReadStream_QueueBuffersThread: begins async bulk reads on EP2 IN\n\n### Capture Stop Sequence (5 steps)\n1. HandleStopCapture: validates capture active\n2. SendStopCommand: sends COMMAND_STOP_STREAM via EP1 OUT\n3. Cancel in-flight transfers: libusb_cancel_transfer on all pending EP2 IN\n4. StopBulkReadStream: thread exit\n5. CaptureCompleteData: propagates end signal through graph\n\n### Configuration Constraints Documented\n- mSamplesPerChunk bounded by FpgaConstants per FPGA vendor\n- Trigger buffer alignment to SamplesPerPacketMultiple_WithStats\n- 5 device-specific StartSampling implementations\n- SamplingConfigurationData fields: downsample, pre, post, stats, chunk_size\n\n### Files Created\n- re/notes/capture-start-stop.md (working notes)\n- re/ghidra/scripts/TraceCaptureSequence.py (Ghidra analysis script)\n- doc/protocol/capture-sequence.md (promoted documentation)\n\n### Open Questions\n- Exact start/stop command byte format\n- Configuration blob binary layout\n- FPGA register address map\n- Numeric values of FpgaConstants"
<!-- SECTION:FINAL_SUMMARY:END -->
