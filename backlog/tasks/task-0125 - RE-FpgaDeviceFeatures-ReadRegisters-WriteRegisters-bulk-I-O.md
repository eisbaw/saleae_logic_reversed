---
id: TASK-0125
title: RE FpgaDeviceFeatures ReadRegisters/WriteRegisters bulk I/O
status: To Do
assignee: []
created_date: '2026-03-11 11:20'
labels:
  - re
  - ghidra
  - fpga
  - protocol
dependencies: []
references:
  - re/notes/fpga-features-vtable.md
  - re/notes/fpga-register-map.md
  - crates/saleae-driver/src/ep1.rs
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
FpgaDeviceFeatures::ReadRegisters(...) and WriteRegisters(...) are the low-level bulk register I/O functions. Our EP1 implementation does single-register reads/writes, but the binary may batch multiple registers into one EP1 transfer for efficiency.

Investigation:
1. Decompile ReadRegisters and WriteRegisters
2. Check if they batch multiple register ops into a single EP1 command
3. Document the batched register command format (if any)
4. Check if WriteConfigurationData differs from WriteRegisters
5. Document WriteRegisterBit (read-modify-write pattern)

If batching exists, our driver may need to support it for correctness (the device might expect batched writes for atomic register updates).
<!-- SECTION:DESCRIPTION:END -->
