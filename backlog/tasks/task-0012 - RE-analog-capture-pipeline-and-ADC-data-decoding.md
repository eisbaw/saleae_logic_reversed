---
id: TASK-0012
title: RE analog capture pipeline and ADC data decoding
status: To Do
assignee: []
created_date: '2026-03-08 22:44'
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
