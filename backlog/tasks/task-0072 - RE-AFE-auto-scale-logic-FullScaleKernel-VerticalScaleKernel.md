---
id: TASK-0072
title: RE AFE auto-scale logic (FullScaleKernel / VerticalScaleKernel)
status: Done
assignee: []
created_date: '2026-03-10 12:23'
updated_date: '2026-03-10 16:58'
labels:
  - re
  - ghidra
  - analog
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Reverse engineer the automatic voltage range selection for analog channels. Need:

- FullScaleKernel class and its scaling algorithm
- VerticalScaleKernel class
- How the driver selects attn1, attn2, VGA gain settings based on signal amplitude
- Auto-range detection logic (when to switch ranges)
- GetAnalogAdvancedPerformanceOptions for multi-rate configs
- Per-AFE-config calibration coefficient selection

Partially relates to TASK-0040 (AFE voltage ranges) but focuses on the automatic selection logic rather than the static range definitions.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Two-phase auto-scale algorithm: FullScaleKernel at full +/-30V uses zero-crossing detection (thresholds: <6=DC, 6-2600=AC, >2600=too-fast, hysteresis +49/-50 ADC codes) to classify signals. VerticalScaleKernel measures actual min/max ADC values, detects saturation at -2048/2047, computes optimal range with 20% margin (min 1/128 ADC range). AutoScaleRunner orchestrates state machine starting at +/-5V. MaxScopeChannels=4 confirmed. Created re/notes/afe-autoscale.md and 2 Ghidra scripts.
<!-- SECTION:FINAL_SUMMARY:END -->
