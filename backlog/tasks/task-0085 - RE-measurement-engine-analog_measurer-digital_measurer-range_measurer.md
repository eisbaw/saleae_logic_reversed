---
id: TASK-0085
title: 'RE measurement engine (analog_measurer, digital_measurer, range_measurer)'
status: Done
assignee: []
created_date: '2026-03-10 12:32'
updated_date: '2026-03-10 19:52'
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
Measurement engine is post-capture, not real-time. Dual-layer: C++ graph kernels (MeasurementNode, AnalogMeasurer, DigitalMeasurer, RangeMeasurer) orchestrate, Python 3.8 runs actual algorithms.\n\nTwo API generations:\n- V1 (apiVersion 1.0): Streaming batches via process_data() then measure()\n- V2 (apiVersion 1.1): Full-span measure_range() with Measure/Annotation system\n\n27 metrics across 5 bundled extensions:\n- VoltageMeasurer: Min, Max, PeakToPeak, Mean, Mid, Amplitude, Top (histogram), Base, RMS\n- PulseMeasurer: Rise Time (10-90%), Fall Time, Positive/Negative Overshoot\n- TimeMeasurer: Period, Frequency, Pulse Widths, Duty Cycle, Cycle RMS\n- ClockStatsMeasurer: Edge counts, Freq stats, Period StdDev (Welford's)\n- VoltageStatisticsMeasurer: Voltage RMS (numpy)\n\nKey functions: MeasurementNode::MeasureRange (ELF 0x1bfd130, 5864 bytes), Measurer::RunMeasurements analog (0x210a4c0) and digital (0x21097e0).\n\nPoint measurements not implemented (commented out in V2 API).\n\nAnalysis: re/notes/measurement-engine.md (447 lines)"
<!-- SECTION:FINAL_SUMMARY:END -->
