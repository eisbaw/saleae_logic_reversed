---
id: TASK-0085
title: 'RE measurement engine (analog_measurer, digital_measurer, range_measurer)'
status: Done
assignee: []
created_date: '2026-03-10 12:32'
updated_date: '2026-03-10 19:50'
labels:
  - re
  - ghidra
  - measurements
dependencies: []
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Source paths leaked:
- `/workspace/monorepo/graph-io/nodes/analog_measurer/src/analog_measurer.cpp`
- `/workspace/monorepo/graph-io/nodes/digital_measurer/src/digital_measurer.cpp`
- `/workspace/monorepo/graph-io/nodes/range_measurer/src/range_measurer.cpp`
- `/workspace/monorepo/graph-io/nodes/measurement_node/src/measurement_node/measurement_node.cpp`
- `/workspace/monorepo/graph-io/core/measurements/src/measurements/measurer.cpp`

The measurement engine computes real-time measurements on captured data:
- Frequency, period, duty cycle, rise/fall time (digital)
- Min, max, peak-to-peak, RMS, mean (analog)
- Range-based measurements (between cursors)

Investigation steps:
1. Search for strings like "measurer", "measurement", "frequency", "duty_cycle"
2. Decompile the base measurer class and its specializations
3. Document available measurement types
4. Determine if measurements are computed on-the-fly or post-capture

Nice-to-have for a feature-complete driver but not blocking basic capture functionality.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Fully reverse engineered the Saleae Logic 2 measurement engine architecture.\n\nKey findings:\n- Dual-layer architecture: C++ orchestration (graph kernel nodes) + Python extension layer\n- Two API generations: V1 (streaming batches, apiVersion 1.0.0) and V2 (span-based with declarative measures/annotations, apiVersion 1.1.0)\n- Measurements are POST-CAPTURE, not on-the-fly -- they operate on stored waveform data via store APIs\n- All measurement algorithms run in embedded Python 3.8; C++ handles data access and graph integration\n- Point measurements (measure_point) are commented out/not implemented\n\nBundled extensions documented:\n- V2: VoltageMeasurer (9 metrics), PulseMeasurer (4 metrics), TimeMeasurer (7 metrics)\n- V1: ClockStatsMeasurer (6 digital metrics), VoltageStatisticsMeasurer (1 analog metric)\n\nArtifacts:\n- re/notes/measurement-engine.md -- full architecture documentation\n- re/ghidra/scripts/DecompileMeasurers.java -- Ghidra script (phase 1)\n- re/ghidra/scripts/DecompileMeasurers2.java -- Ghidra script (phase 2)\n- re/ghidra/decompiled-measurers.txt -- cleaned decompiled output (46 functions)\n\n30+ C++ function addresses mapped with ELF VAs and descriptions.
<!-- SECTION:FINAL_SUMMARY:END -->
