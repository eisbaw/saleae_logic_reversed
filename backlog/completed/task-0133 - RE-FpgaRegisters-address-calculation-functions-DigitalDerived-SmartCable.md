---
id: TASK-0133
title: 'RE FpgaRegisters address calculation functions (DigitalDerived, SmartCable)'
status: Done
assignee: []
created_date: '2026-03-11 11:22'
updated_date: '2026-03-12 02:52'
labels:
  - re
  - ghidra
  - fpga
dependencies: []
references:
  - crates/saleae-proto/src/fpga_regs.rs
  - re/notes/fpga-register-map.md
  - re/notes/smart-cable.md
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Two named functions calculate FPGA register addresses:
- Saleae::FpgaRegisters::CalculateDigitalConfigurationAddress_DigitalDerived(int)
- Saleae::FpgaRegisters::CalculateDigitalConfigurationAddress_SmartCable(int, int)

We have a SmartCable address formula in fpga_regs.rs (0x0120 + port*16 + sub_ch*4) but the DigitalDerived formula is not implemented. This controls how digital channel configuration registers are addressed.

Investigation:
1. Decompile both functions
2. Document the address formulas
3. Verify our SmartCable formula matches
4. Implement DigitalDerived formula in fpga_regs.rs
5. Check if there are other CalculateXxx address functions we're missing
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Decompiled both `CalculateDigitalConfigurationAddress_DigitalDerived(int)` and `CalculateDigitalConfigurationAddress_SmartCable(int, int)` from `libgraph_server_shared.so` using Ghidra headless. Each function exists as two inlined variants in the binary.

**DigitalDerived** (ELF VAs 0x2d61e10, 0x2d66870):
- Formula: `channel_index * 4 + 0x170`, with assertion `channel < 4`
- Our code: `0x0170 + channel_index * 4` with `assert!(channel < 4)` -- exact match

**SmartCable** (ELF VAs 0x2d61da0, 0x2d66380):
- Formula: `port * 0x10 + sub_channel * 4 + 0x120`, with assertion `offset < 0x50`
- Our code: `0x0120 + port * 16 + sub_channel * 4` with `assert!(offset < 0x50)` -- exact match

Searched for other `Calculate*Address` functions (CalculateAnalog, CalculateScope, etc.) -- none exist. These are the only two address calculation functions in the FpgaRegisters namespace.

Updated doc comments on `REG_SMART_CABLE_BASE`, `REG_DIGITAL_CH_CONFIG_BASE`, `digital_ch_config_addr()`, and `smart_cable_config_addr()` to note decompilation-verified status with ELF VAs and source file (mso_fpga.cpp).
<!-- SECTION:FINAL_SUMMARY:END -->
