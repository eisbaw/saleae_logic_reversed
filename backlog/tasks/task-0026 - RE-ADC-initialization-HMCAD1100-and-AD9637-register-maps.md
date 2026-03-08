---
id: TASK-0026
title: RE ADC initialization -- HMCAD1100 and AD9637 register maps
status: To Do
assignee: []
created_date: '2026-03-08 23:49'
labels:
  - analog
  - adc
  - hardware
dependencies:
  - TASK-0006
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Two ADC types found: HMCAD1100 (Logic 4/8, TI quad 8-bit) and AD9637 (Logic Pro 8/16, ADI quad 12-bit). ADC register access goes through FPGA: "ADC Read 0x{:x} = 0x{:x}", "ADC Write 0x{:x} = 0x{:x}". Key methods: Logic2FpgaDevice::InitHmcad1100, Logic2FpgaDevice::SetAd9637Enabled, Logic2FpgaDevice::WriteAd9637Register(U16, U16), Logic2FpgaDevice::SetTestPattenHmcad1100. ADC foreground calibration is referenced. Board revision determines ADC type. The ADC init sequence is critical for getting valid analog samples.
<!-- SECTION:DESCRIPTION:END -->
