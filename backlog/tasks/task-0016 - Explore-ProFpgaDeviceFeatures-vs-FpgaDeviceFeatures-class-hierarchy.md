---
id: TASK-0016
title: Explore ProFpgaDeviceFeatures vs FpgaDeviceFeatures class hierarchy
status: In Progress
assignee: []
created_date: '2026-03-08 22:45'
updated_date: '2026-03-09 01:48'
labels: []
dependencies:
  - TASK-0003
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Investigate the device feature class hierarchy to understand per-device differences.

Known classes:
- Fx2DeviceFeatures (legacy FX2 devices)
- FpgaDeviceFeatures (Logic 4, Logic 8)
- ProFpgaDeviceFeatures (Logic Pro 8, Logic Pro 16)
- Logic2FpgaDevice (common base?)

Hypothesis: these classes define per-device capabilities -- max sample rate, channel count, supported voltage levels, ADC type, available analog channels, etc. Understanding this hierarchy tells us what differs between device variants.

Investigate:
1. What vtable entries differ between Pro and non-Pro?
2. What constants/capabilities are defined per class?
3. How does the code select which class to instantiate? (based on USB PID?)
4. What is the relationship to bitstream selection?

Create follow-up tasks for each significant behavioral difference found.
<!-- SECTION:DESCRIPTION:END -->
