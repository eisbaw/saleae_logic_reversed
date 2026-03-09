---
id: TASK-0059
title: Implement FPGA bitstream programming against real hardware
status: In Progress
assignee: []
created_date: '2026-03-09 11:21'
updated_date: '2026-03-09 13:27'
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

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Implementation Progress (2026-03-09)

Implemented FPGA programming and post-init in code with mock tests. Hardware testing BLOCKED (device red LED blinking, needs power cycle).

### Files Created
- `crates/saleae-proto/src/fpga_regs.rs` -- 39 register address constants, helper functions
- `crates/saleae-driver/src/fpga.rs` -- `program_fpga()` and `fpga_post_init()` functions

### Files Modified
- `crates/saleae-driver/src/ep1.rs` -- Added `write_register_16`, `read_register_16`, `write_register_8`, `read_register_8`, `write_fpga_register`, `read_fpga_register` methods
- `crates/saleae-driver/src/error.rs` -- Added `FpgaProgramming` error variant
- `crates/saleae-driver/src/lib.rs` -- Added `fpga` module
- `crates/saleae-proto/src/lib.rs` -- Added `fpga_regs` module
- `crates/saleae-cli/src/main.rs` -- Added `ProgramFpga` subcommand
- `crates/saleae-cli/src/commands.rs` -- Added `program_fpga()` command implementation

### Open Questions
- 16-bit register path encryption: We route it through `send_command` treating addr_hi as opcode. Needs hardware validation.
- Post-init register VALUES are placeholders (all zeros except GPIF_FLUSH_TIMEOUT=100000 ticks). Need USB capture to get real values.
- Readback register address (0x00) is a guess. Need USB capture confirmation.
- JESD PHY reset bit pattern is a guess (bit 0). Need USB capture confirmation.
<!-- SECTION:NOTES:END -->
