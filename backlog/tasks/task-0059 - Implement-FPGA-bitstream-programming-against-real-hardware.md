---
id: TASK-0059
title: Implement FPGA bitstream programming against real hardware
status: To Do
assignee: []
created_date: '2026-03-09 11:21'
labels:
  - rust
  - hardware-testing
  - fpga
dependencies:
  - TASK-0058
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Program the Lattice ECP5 FPGA bitstream on the Logic Pro 8.

Implementation:
1. Extract the correct bitstream for Logic Pro 8 from the .so (one of the 10 Lattice ECP5 bitstreams)
   - Need to determine which of the 10 maps to Logic Pro 8 (depends on hardware revision)
   - Try reading FPGA register after reset to identify expected bitstream
2. Send ResetFpga command (opcode 0x7E) via EP1
3. Send bitstream via WriteConfigurationData (opcode 0x7F) in chunks
   - USB 3.0 format: 509-byte chunks via EP1 OUT
   - USB 2.0 format: 62-byte chunks via EP1 OUT
4. Verify programming success: read FPGA status register, expect 0xAA
5. Run post-programming init sequence:
   - Write COMMIT_CONTROL register
   - Write CAPTURE_CONTROL register
   - Write INIT_CONFIG register
   - Configure GPIF
   - For Lattice: reset JESD PHY, poll DDR ready ((reg_0x0008 & 0x18) == 0x18)

Test: After programming, register reads should return valid data. Device should not enter error state.

References: doc/protocol/fpga-bitstream.md, doc/protocol/fpga-registers.md, re/notes/fpga-bitstream-protocol.md
<!-- SECTION:DESCRIPTION:END -->
