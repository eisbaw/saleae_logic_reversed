---
id: TASK-0192
title: 'RE: Decompile AFE config enumeration and HMCAD1100 full init sequence'
status: Done
assignee: []
created_date: '2026-03-13 05:36'
updated_date: '2026-03-13 08:31'
labels:
  - RE
  - Ghidra
  - adc
  - afe
dependencies: []
references:
  - 'crates/saleae-driver/src/capture.rs:437'
  - re/notes/afe-voltage-ranges.md
  - re/notes/adc-initialization.md
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
AFE configuration in capture.rs (AfeChannelConfig struct, lines 437-479) has completely speculative field names and unknown bit layout. Need Ghidra decompilation of: (1) AfeConfig_RawHardware::ForConfigNumber -- to get total AFE config count and field meanings. (2) EncodeVgaConfig -- to extract filter and power bit fields for LMH6518. (3) InitHmcad1100 -- full register init sequence for Logic 4/8. (4) GetAdcRegisterConfiguration -- ADS528x register table. These are the biggest remaining RE unknowns blocking non-Pro device support.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Decompiled and documented the AFE configuration system and HMCAD1100 init sequence from Ghidra analysis of libgraph_server_shared.so.

Key findings:
- ForConfigNumber: 66 total configs (TotalAfeConfigurations = 0x42), indexed into AllConfigs table at ELF VA 0x0079a448
- GetConfigNumber: linear scan with stride 0x18, total size 0x630
- EncodeVgaConfig: LMH6518 SPI uint16_t encoding -- bits [3:0]=ladder/2, bit [4]=preamp HG/LG, bits [8:6]=filter, bit [10]=power
- Extracted full AllConfigs table: 3 attn combos x 2 preamp modes x 11 ladder values = 66 entries, all with filter=4, power=1, dac=0.0
- AfeConfig_RawHardware struct: 24 bytes confirmed layout (attn1, attn2, preamp, power, filter, ladder, dac)
- CommonDeviceInit ADC type switch: 0=HMCAD1100, 1=AD9637, 2=ADS528x
- Hmcad1100Settings per-device defaults for device types 5, 6, 7

New files: DecompileAfeConfig.java, ExtractAfeConfigTable.java, re/notes/afe-config-enumeration.md
Commit: bd0af4c
<!-- SECTION:FINAL_SUMMARY:END -->
