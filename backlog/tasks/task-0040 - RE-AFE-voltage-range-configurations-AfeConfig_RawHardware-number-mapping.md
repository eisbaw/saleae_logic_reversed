---
id: TASK-0040
title: RE AFE voltage range configurations (AfeConfig_RawHardware number mapping)
status: Done
assignee: []
created_date: '2026-03-09 00:50'
updated_date: '2026-03-09 07:17'
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

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Completed RE analysis of AFE voltage range configurations. Created `re/notes/afe-voltage-ranges.md` documenting:

**Key Findings:**

1. **Two AFE config types**: `AfeConfig_VoltageScale` (frontend sends desired min/max voltage) and `AfeConfig_RawHardware` (actual hardware settings: attn1, attn2, dac, preamp, ladder, filter, power).

2. **Signal chain**: Input -> Attenuator 1 (100x) -> Attenuator 2 (10x) -> LMH6518 VGA (preamp HG/LG + ladder 0 to -20dB in 2dB steps) -> DAC offset (-2.5V to +2.5V) -> ADC.

3. **Not a discrete set**: Unlike traditional oscilloscopes, the voltage range is NOT a fixed set of discrete values. The frontend sends the desired min/max voltage, and the native code (`AnalogChannelConfigToHardwareConfig`) computes optimal AFE settings.

4. **Confirmed constants**: DAC range -2.5V to +2.5V (assertion), ladder attenuation 0 to -20dB in 2dB steps (3 assertions), attenuator factors 100x and 10x (transform names), coupling AC/DC, probe 1x/10x.

5. **Config numbering**: `ForConfigNumber(int)` / `GetConfigNumber()` system exists for indexed access. Total count (`TotalAfeConfigurations`) not yet extracted from binary.

6. **Per-device differences**: Pro devices have full AFE with LMH6518. Logic 4/8 have simpler fixed-gain paths.

**Still unknown (needs Ghidra)**: Exact TotalAfeConfigurations count, EncodeVgaConfig bit layout, VoltageLimitAfterAttenuators value, LMH6518 filter options.
<!-- SECTION:FINAL_SUMMARY:END -->
