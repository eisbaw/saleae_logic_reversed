---
id: TASK-0026
title: RE ADC initialization -- HMCAD1100 and AD9637 register maps
status: Done
assignee: []
created_date: '2026-03-08 23:49'
updated_date: '2026-03-09 05:33'
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

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Searched libgraph_server_shared.so binary for all ADC-related strings and cross-referenced with HMCAD1100 and AD9637 public datasheets. Created Ghidra script TraceAdcInit.py and documented findings in re/notes/adc-initialization.md.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Completed investigation of ADC initialization for all three ADC families used in Saleae Logic devices:

1. HMCAD1100 (Logic 4/8, Xilinx revisions): 8-bit quad ADC with 7-bit SPI register access via FPGA bridge (reg 0x0004). Init function InitHmcad1100 writes configuration through WriteSpi. Register map cross-referenced with SBAS547C datasheet. Gain calibration stored in /hittiteAdcGainRegisterValue.

2. AD9637 (Logic Pro 8/16, Xilinx revisions): 12-bit dual ADC with 13-bit SPI register access via ReadSpi_15BitAddress. Init verifies vendor ID (expected 0x49). Board revision assertion ensures correct ADC type.

3. ADS528x (all devices, Lattice ECP5 revisions): 12-bit octal ADC with JESD204B serial interface. Init involves PHY transceiver reset, JESD lock with retry, and bitslip detection/correction.

All three paths converge through foreground calibration: "Waiting for ADC calibration to finish" / "ADC failed to finish foreground calibration".

Deliverables:
- re/ghidra/scripts/TraceAdcInit.py (Ghidra script with HMCAD1100 and AD9637 register maps)
- re/notes/adc-initialization.md (comprehensive analysis with 60+ binary string cross-references)
<!-- SECTION:FINAL_SUMMARY:END -->
