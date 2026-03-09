---
id: TASK-0038
title: RE USB data interleaving format (DisaggregatorSettingsData)
status: To Do
assignee: []
created_date: '2026-03-09 00:50'
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
