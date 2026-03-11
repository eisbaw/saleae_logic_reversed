---
id: TASK-0105
title: RE analog downsampler node -- decimation algorithms
status: Done
assignee: []
created_date: '2026-03-10 23:14'
updated_date: '2026-03-11 01:44'
labels:
  - re
  - ghidra
  - analog
dependencies: []
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Source path leaked: `/workspace/monorepo/graph-io/nodes/analog_downsampler_node/src/analog_downsampler_node.cpp`

The analog downsampler handles sample rate reduction for display and analysis. Understanding this reveals:
- Decimation algorithm (simple averaging? anti-alias filter + decimate?)
- LOD (level-of-detail) generation for zoom levels
- Relationship to the 6 power-of-2 reduction levels found in TASK-0081

Investigation steps:
1. Search for "downsampler", "downsample", "decimate" strings
2. Find assertion messages referencing analog_downsampler_node.cpp
3. Decompile the downsampling kernel
4. Document the algorithm and its relationship to waveform summaries

Relevant for correct analog data at reduced sample rates.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
RE'd the analog downsampler node. Key findings: (1) Simple integer-stride decimation -- selects every Nth sample, no averaging or filtering. (2) Data type is VectorData&lt;int16_t&gt;. (3) DownsampleMode enum: Average=1, MinMax=2, written to FPGA register 0x40 bits[6:5]. (4) Valid ratios: 1,2,4,6,8,16,24,32...up to 162528. (5) Anti-aliasing handled upstream by FPGA or calibration FIR. (6) WaveformSummaryNode (LOD for display) is completely separate. Documented in re/notes/analog-downsampler.md, 23 functions decompiled.
<!-- SECTION:FINAL_SUMMARY:END -->
