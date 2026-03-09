---
id: TASK-0012
title: RE analog capture pipeline and ADC data decoding
status: Done
assignee: []
created_date: '2026-03-08 22:44'
updated_date: '2026-03-09 00:49'
labels: []
dependencies:
  - TASK-0006
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Reverse engineer the analog sample data pipeline.

Known from string/symbol analysis:
- AnalogDeserializer
- AnalogMeasurerV1, AnalogMeasurerV2 -- two versions of the measurer
- AnalogChannelConfigToHardwareConfig
- analogChannelCount, analogChannels, afeChannels
- analogDownsample, analogLimit_MaxDownsampleRatio
- "Analog Fifo overflow" -- streaming overflow detection
- "analog fsm error" -- analog-specific state machine
- analog_decimation_upper constraints

Investigate:
1. What ADC is used per device? (HMCAD1100, CDK8307B, ADC1173)
2. What is the raw ADC data format? (bit width, encoding, byte order)
3. How is analog data interleaved with digital in the USB stream?
4. What is the analog decimation/downsampling pipeline?
5. What is the difference between MeasurerV1 and MeasurerV2?
6. How does sample rate vs channel count tradeoff work for analog?
7. What does AnalogChannelConfigToHardwareConfig do -- translate user config to register writes?

This is the harder path. Analog data is NOT raw ADC values -- it goes through calibration and FIR filtering. This task covers the raw ADC side; calibration is TASK-0013.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Binary analysis completed. Searched libgraph_server_shared.so for all analog-related strings, symbols, and RTTI type info.

Key findings:
- Three ADC families identified: HMCAD1100 (8-bit, Logic 4/8 old rev), AD9637 (12-bit, Pro old rev), ADS528x (12-bit, all new Lattice ECP5 rev)
- ADC selection by RevisionDetail::AdcType enum
- Raw ADC data stored as int16_t (VectorData<int16_t>) -- confirmed by template instantiations
- Full analog pipeline mapped: DisaggregatorNode -> AnalogCalibrationFilter -> AnalogCreatorNode -> AnalogDownsamplerNode -> AnalogStore
- AFE signal chain: Input -> attn1 -> attn2 -> LMH6518 VGA -> ADC, with DC offset DAC
- Two downsample modes: Average (1), MinMax (2)
- Per-packet AnalogStats metadata
- JESD204B interface for ADS528x on Lattice ECP5 with bitslip detection
- Two Python APIs: analog_span_adc (Logic) and analog_span_wdc (MSO)
- ADC-to-voltage linear transform: mScale, mOffset fields
- AFE configs numbered with predefined attn/gain/dac settings

Artifacts produced:
- re/ghidra/scripts/TraceAnalogCapture.py
- re/notes/analog-capture-pipeline.md
- doc/protocol/analog-capture.md
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Reverse engineered the analog capture pipeline from static binary analysis. Identified three ADC families (HMCAD1100, AD9637, ADS528x) with their initialization sequences. Mapped the full data pipeline from USB to storage. Documented AFE signal chain, decimation modes, JESD204B interface, and ADC data format (int16_t). Created Ghidra trace script and comprehensive documentation.
<!-- SECTION:FINAL_SUMMARY:END -->
