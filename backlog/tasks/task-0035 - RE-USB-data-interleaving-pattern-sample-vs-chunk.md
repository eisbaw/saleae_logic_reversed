---
id: TASK-0035
title: RE USB data interleaving pattern (sample vs chunk)
status: To Do
assignee: []
created_date: '2026-03-09 00:38'
labels:
  - re
  - usb
  - data-format
dependencies:
  - TASK-0011
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Determine how digital and analog channels are interleaved in the EP2 IN bulk transfer stream.

Options:
1. Sample-interleaved: ch0-s0, ch1-s0, ch2-s0, ch0-s1, ch1-s1, ...
2. Chunk-interleaved: N samples of ch0, N samples of ch1, ...
3. Block-interleaved: full packet of ch0, full packet of ch1, ...

The DisaggregatorNode splits the stream -- analyze its code to determine the pattern. Also examine DisaggregatorSettingsData for channel layout metadata.

Method: Ghidra decompilation of DisaggregatorNode, or USB capture with known signals on multiple channels.
<!-- SECTION:DESCRIPTION:END -->
