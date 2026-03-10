---
id: TASK-0072
title: RE AFE auto-scale logic (FullScaleKernel / VerticalScaleKernel)
status: Done
assignee: []
created_date: '2026-03-10 12:23'
updated_date: '2026-03-10 16:57'
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
Reverse engineered the AFE auto-scale logic from libgraph_server_shared.so. Key findings:

**Two-Phase Algorithm:**
1. FullScaleKernel (autoscale_horizontal_kernel.cpp): Zero-crossing detection with hysteresis (+49/-50 ADC codes). Classifies signals as DC (<6 crossings), AC (6-2600 crossings), or too-fast (>2600). Uses full +/-30V range initially.
2. VerticalScaleKernel (autoscale_vertical_kernel.cpp): Precision min/max measurement with 20% margin (1/128 minimum). Iteratively narrows voltage range over multiple waveforms to fit signal amplitude.

**Key Functions Decompiled:**
- AutoScaleRunner::OnWaveform (ELF 0x2d5bb20): State machine orchestrator, starts at +/-5V
- FullScaleKernel::HandleWaveform (ELF 0x2d5c600, 2074 bytes): Zero-crossing classifier
- VerticalScaleKernel::HandleWaveform (ELF 0x2d5df90, 1723 bytes): Min/max fitter with saturation detection
- HandleStartAutoScale (ELF 0x1cbe3d0): IPC entry point
- Result serializers with /error, /minVoltage, /maxVoltage JSON fields

**Class Hierarchy:** AutoScaleKernel (base) -> FullScaleKernel, VerticalScaleKernel. MsoConstants::MaxScopeChannels = 4.

**Files Created:**
- re/notes/afe-autoscale.md (comprehensive analysis)
- re/ghidra/scripts/DecompileAutoScale.java, DecompileAutoScale2.java
- re/ghidra/decompiled-autoscale.txt, decompiled-autoscale2.txt
<!-- SECTION:FINAL_SUMMARY:END -->
