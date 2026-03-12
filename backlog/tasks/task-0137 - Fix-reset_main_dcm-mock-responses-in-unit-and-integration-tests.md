---
id: TASK-0137
title: Fix reset_main_dcm mock responses in unit and integration tests
status: Done
assignee: []
created_date: '2026-03-12 06:14'
labels:
  - testing
  - mocks
dependencies: []
references:
  - crates/saleae-driver/src/adc.rs
  - crates/saleae-driver/src/fpga.rs
  - crates/saleae-driver/tests/integration.rs
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The `reset_main_dcm()` function in `adc.rs` was changed from a 2-step reset (r0x00=0x04/0x00) to a hardware-validated 5-step JESD cascade with retry. All mock response queues need to match the new implementation.

**Already fixed:**
- `crates/saleae-driver/src/fpga.rs` - `queue_reset_main_dcm` closure in `queue_xilinx_device_init_responses`
- `crates/saleae-driver/tests/integration.rs` - `queue_reset_main_dcm` closure

**Pattern for new mock:**
5 write ACKs (JESD cascade) + 1 read r0x40 (0xA0=locked) + 2 write ACKs (SERDES) + 1 read r0x19 + 1 read r0x4a + 1 read final r0x40 = 11 responses per reset_main_dcm call (was 8).

All tests currently pass.
<!-- SECTION:DESCRIPTION:END -->
