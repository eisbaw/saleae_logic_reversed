---
id: TASK-0104
title: Implement AFE auto-scale algorithm in saleae-driver
status: Done
assignee: []
created_date: '2026-03-10 23:14'
updated_date: '2026-03-11 01:35'
labels:
  - rust
  - saleae-driver
  - implementation
dependencies: []
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-0072 documented the two-phase AFE auto-scale algorithm. Implement for automatic voltage range selection.

Requirements:
- FullScaleKernel: zero-crossing detection for signal classification
  - Thresholds: <6 = DC, 6-2600 = AC, >2600 = too-fast
  - Hysteresis: +49/-50 ADC codes
- VerticalScaleKernel: min/max ADC measurement, saturation detection, optimal range with 20% margin
- MaxScopeChannels = 4 (confirmed)
- Integration with CaptureConfig: auto-select voltage range if not manually specified
- Available voltage ranges per device type

Reference: re/notes/afe-autoscale.md
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Implemented AFE auto-scale in saleae-driver/src/autoscale.rs (1494 lines). Three types: FullScaleKernel (zero-crossing detector with +49/-50 hysteresis, DC/AC/TooFast classification), VerticalScaleKernel (min/max tracker, saturation detection, 20% margin range selection), AutoScaler (orchestrator with per-channel state machine, 5 retry iterations on saturation). Supports 8/10/12-bit ADC, up to 4 channels. 53 tests.
<!-- SECTION:FINAL_SUMMARY:END -->
