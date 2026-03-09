---
id: TASK-0035
title: RE USB data interleaving pattern (sample vs chunk)
status: Done
assignee: []
created_date: '2026-03-09 00:38'
updated_date: '2026-03-09 03:38'
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

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Investigated via binary string analysis of libgraph_server_shared.so. Found sample-interleaved byte-level format with GPIF width selection (16/32-bit bus). DisaggregatorNode splits stream using DisaggregatorSettingsData. Digital: 8 samples/byte in groups of 2 or 4. Analog: int16_t (2 bytes/sample). ContractReadData removes bus-width padding before disaggregation. Created Ghidra script TraceDisaggregator.py and documented findings in re/notes/data-interleaving.md. Merged with duplicate TASK-0038.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Determined that the USB data interleaving pattern is sample-interleaved at the byte level. Each sample row in the EP2 IN bulk transfer contains one sample-period of data for all enabled channels, laid out contiguously. The FPGA outputs data through the FX3 GPIF II interface at either 16-bit or 32-bit bus width, selected based on analog channel configuration. ContractReadData removes bus-width padding, and the DisaggregatorNode then splits the compacted stream into per-channel data using layout metadata from DisaggregatorSettingsData. Digital channels are packed 8 samples per byte in groups of 2 or 4. Analog channels use int16_t (2 bytes per sample). Exact struct layout of DisaggregatorSettingsData requires Ghidra decompilation.
<!-- SECTION:FINAL_SUMMARY:END -->
