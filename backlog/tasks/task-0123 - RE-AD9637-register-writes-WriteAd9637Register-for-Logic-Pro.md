---
id: TASK-0123
title: RE AD9637 register writes (WriteAd9637Register) for Logic Pro
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
  - re/notes/init-sequence-pcap.md
  - crates/saleae-driver/src/jesd.rs
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Logic2FpgaDevice::WriteAd9637Register(U16, U16) and SetAd9637Enabled(bool) configure the AD9637 ADC in Logic Pro 8/16. The pcap shows ADC register writes via indirect access (regs 0x03/0x04/0x05/0x16) but we haven't decompiled the function that generates those values.

Investigation:
1. Find strings referencing Ad9637 or WriteAd9637Register
2. Decompile WriteAd9637Register and SetAd9637Enabled
3. Map the indirect register write pattern to AD9637 SPI register addresses
4. Document: which AD9637 registers are written, what values, in what order
5. Compare against AD9637 datasheet (Analog Devices)
6. Cross-reference against pcap Phase 3 ADC init register writes

This is our primary target device (Logic Pro 8) so getting ADC init right is critical.
<!-- SECTION:DESCRIPTION:END -->
