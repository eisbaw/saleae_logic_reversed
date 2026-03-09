---
id: TASK-0038
title: RE USB data interleaving format (DisaggregatorSettingsData)
status: Done
assignee: []
created_date: '2026-03-09 00:50'
updated_date: '2026-03-09 03:38'
labels:
  - analog
  - digital
  - usb-protocol
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Reverse engineer how analog and digital data are interleaved in the USB bulk transfer stream. The DisaggregatorNode uses DisaggregatorSettingsData to split the interleaved byte stream. Need Ghidra decompilation of DisaggregatorNode to determine if it is sample-interleaved, block-interleaved, or header+payload format.

Key targets:
- LogicDeviceNode::GetDisaggregatorSettings -- creates the settings
- DisaggregatorNode::HandlePipeData -- the actual splitting logic
- DisaggregatorSettingsData struct layout

Discovered during TASK-0012 analog pipeline analysis.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Duplicate of TASK-0035. Both tasks investigated together. See TASK-0035 for full analysis. Key findings: DisaggregatorSettingsData is constructed by LogicDeviceNode::GetDisaggregatorSettings(GraphTime). The struct describes the per-channel byte layout within the interleaved sample row. The disaggregator receives OwnedSpan<uint8_t> (raw bytes), DisaggregatorSettingsData (layout), and CaptureCompleteData (end signal). It emits OwnedSpan<uint8_t> per digital channel and VectorData<int16_t> per analog channel.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Duplicate of TASK-0035, investigated together. The DisaggregatorNode uses DisaggregatorSettingsData to split a sample-interleaved byte stream. Each sample row contains digital bytes followed by analog int16_t values for all enabled channels. The GPIF bus width (16/32-bit) determines the raw row width, and ContractReadData compacts it before disaggregation. Detailed findings in re/notes/data-interleaving.md and Ghidra script in re/ghidra/scripts/TraceDisaggregator.py.
<!-- SECTION:FINAL_SUMMARY:END -->
