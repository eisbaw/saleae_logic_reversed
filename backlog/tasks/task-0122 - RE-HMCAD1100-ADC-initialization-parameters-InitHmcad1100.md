---
id: TASK-0122
title: RE HMCAD1100 ADC initialization parameters (InitHmcad1100)
status: Done
assignee: []
created_date: '2026-03-11 11:20'
updated_date: '2026-03-12 02:01'
labels:
  - re
  - ghidra
  - adc
  - analog
dependencies: []
references:
  - re/notes/adc-initialization.md
  - crates/saleae-driver/src/fpga.rs
  - crates/saleae-driver/src/hmcad.rs
  - re/ghidra/decompiled-hmcad-init.txt
  - re/ghidra/decompiled-hmcad-helpers.txt
  - re/ghidra/scripts/DecompileHmcadInit.java
  - re/ghidra/scripts/DecompileHmcadHelpers.java
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

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
## RE and Implementation Complete

### RE findings

Decompiled 5 functions via Ghidra headless:

1. **InitHmcad1100** (ELF 0x1c20ab0, 565 bytes) - Main init: writes 11 commands (1 FPGA reset + 10 HMCAD SPI registers)
2. **Hmcad1100Settings::SetDefault** (ELF 0x1c1f900, 174 bytes) - Defaults per DeviceType: Logic4 (mode=5, ch=6, coarse_gain=2) vs Logic8 (mode=4, ch=7, coarse_gain=0)
3. **WriteHmcadRegister** (ELF 0x1c207c0, 263 bytes) - 3-register FPGA proxy (r0x03/r0x04/r0x05), unlike AD9637's 4-register proxy (adds r0x16)
4. **SetTestPattenHmcad1100** (ELF 0x1c20cf0, 150 bytes) - Test pattern via 2 lookup tables (6 entries each, extracted from binary at 0x472e94/0x472ea0)
5. **CommonDeviceInit** (ELF 0x1c21fb0, 2260 bytes) - Dispatcher: AdcType==0 calls SetDefault+InitHmcad1100+ResetMainDcm

### Key technical findings

- HMCAD1100 uses 16-bit SPI registers via 3-register FPGA proxy (no trigger register 0x16)
- Register sequence: r0x80(reset), r0x25+r0x45(test patterns), r0x11+r0x12(channel gain), r0x2a+r0x2b(LVDS), r0x46(coarse gain), r0x50(fine gain), r0x55(gain select), r0x56(sleep)
- Gain values use nibble replication: gain=1 becomes 0x111 (3-nibble) or 0x1111 (4-nibble)
- After HMCAD init, CommonDeviceInit runs the same JESD power sequence as AD9637

### Implementation

New file `crates/saleae-driver/src/hmcad.rs` with:
- `Hmcad1100Settings` struct with `defaults(DeviceType)` matching decompiled SetDefault
- `write_hmcad_register()` - 3-register FPGA proxy write
- `init_hmcad1100()` - full register init sequence
- `set_test_pattern()` - test pattern control
- `init_hmcad1100_full()` - top-level init (settings + init + JESD)
- 12 unit tests, all passing. Full workspace (700+ tests) green.

### Not yet hardware-validated

Logic 4/8 hardware access needed for validation. Implementation is purely from decompilation.
<!-- SECTION:FINAL_SUMMARY:END -->
