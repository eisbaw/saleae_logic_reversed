---
id: TASK-0133
title: 'RE FpgaRegisters address calculation functions (DigitalDerived, SmartCable)'
status: To Do
assignee: []
created_date: '2026-03-11 11:22'
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
