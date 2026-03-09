---
id: TASK-0040
title: RE AFE voltage range configurations (AfeConfig_RawHardware number mapping)
status: To Do
assignee: []
created_date: '2026-03-09 00:50'
labels:
  - analog
  - afe
  - calibration
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Reverse engineer the mapping from AFE config number to hardware settings (attenuator values, VGA gain, DAC voltage, resulting voltage range).

Targets:
- AfeConfig_RawHardware::ForConfigNumber(int) -- maps number to settings
- AfeConfig_RawHardware::GetConfigNumber() -- reverse mapping
- MsoAfeMetadata::AFE_CONFIG_COUNT -- total number of configs
- MsoAfeMetadata::MAX_AFE_CHANNEL_COUNT -- max channels

This determines the voltage ranges available to users (e.g., +/-10V, +/-5V, +/-2.5V, +/-1V).

Discovered during TASK-0012.
<!-- SECTION:DESCRIPTION:END -->
