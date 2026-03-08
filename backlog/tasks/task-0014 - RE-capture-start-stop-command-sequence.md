---
id: TASK-0014
title: RE capture start/stop command sequence
status: To Do
assignee: []
created_date: '2026-03-08 22:44'
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
