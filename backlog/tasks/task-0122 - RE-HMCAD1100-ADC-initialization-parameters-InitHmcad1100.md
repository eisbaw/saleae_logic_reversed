---
id: TASK-0122
title: RE HMCAD1100 ADC initialization parameters (InitHmcad1100)
status: To Do
assignee: []
created_date: '2026-03-11 11:20'
labels:
  - re
  - ghidra
  - adc
  - analog
dependencies: []
references:
  - re/notes/adc-initialization.md
  - crates/saleae-driver/src/fpga.rs
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Logic2FpgaDevice::InitHmcad1100(Hmcad1100Settings) configures the HMCAD1100 ADC during device init. The HMCAD1100 is a multi-channel high-speed ADC used in Logic 4/8 (non-Pro). Getting these parameters wrong means the ADC won't produce valid samples.

Investigation:
1. Find assertion strings referencing InitHmcad1100 or Hmcad1100
2. Decompile the function and the Hmcad1100Settings struct
3. Document all register writes: address, value, purpose
4. Compare against HMCAD1100 datasheet (Texas Instruments, freely available)
5. Determine if our common_device_init() covers these writes or misses any

Related: Logic2FpgaDevice::SetTestPattenHmcad1100 (test pattern for ADC validation)

Note: Logic Pro 8/16 use AD9637, not HMCAD1100. This is for non-Pro devices (Logic 4/8).
<!-- SECTION:DESCRIPTION:END -->
