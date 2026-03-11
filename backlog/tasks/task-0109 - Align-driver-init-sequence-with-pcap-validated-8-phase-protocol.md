---
id: TASK-0109
title: Align driver init sequence with pcap-validated 8-phase protocol
status: Done
assignee: []
created_date: '2026-03-11 05:58'
updated_date: '2026-03-11 06:11'
labels:
  - rust
  - saleae-driver
  - protocol
dependencies: []
references:
  - re/notes/init-sequence-pcap.md
  - crates/saleae-driver/src/ep1.rs
  - crates/saleae-driver/src/fpga.rs
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Our driver's initialization doesn't match the exact 261-operation, 8-phase sequence validated from the pcap (re/notes/init-sequence-pcap.md). The pcap is ground truth -- 928 EP1 frames decoded with 0 unknown opcodes.

Current driver init: firmware upload -> FPGA program -> common_device_init (ADC + JESD) -> done.

Pcap-validated init:
1. Reseed + Ping + GetDeviceInfo + ReadVersion + ResetFpga
2. FPGA bitstream upload (334 chunks)
3. Post-FPGA: ADC init, JESD link setup, calibration coefficient writes
4. Reseed #2 + digital channel config
5. Reseed #3 + updated channel config
6. Reseed #4 + EEPROM calibration read (I2C 0xC0)
7. Capture params + StartSampling
8. StopStream + power down

Key differences to reconcile:
- Multiple reseeds throughout init (not just one at start)
- EEPROM calibration read happens mid-init, not at capture time
- Channel config is sent before capture start
- Exact register write ordering matters for JESD link stability

Reference: re/notes/init-sequence-pcap.md
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Implemented the pcap-validated 8-phase init sequence as `init.rs` orchestrator module in saleae-driver.

**New files:**
- `crates/saleae-driver/src/init.rs` -- DeviceSession orchestrator with 8 phases

**Modified files:**
- `crates/saleae-driver/src/ep1.rs` -- Added GetDeviceInfo (0x8B), ReadVersion (0x07), Device2_1 (0x89), WriteI2C (0x87), ReadI2C (0x88)
- `crates/saleae-driver/src/lib.rs` -- Registered init module

**Architecture:**
- `DeviceSession` struct tracks phase state (NotStarted through Teardown)
- Each phase method validates prerequisites before executing
- Reseeds happen at correct phase boundaries (Phase 1, 4, 5, 6)
- JESD power sequence (0x1e->0x1c->0x18->0x08->0x00) extracted as reusable helper
- ADC bank register indirect access pattern extracted as helper
- No code duplication -- orchestrator calls existing ep1.rs/fpga.rs functions

**Tests:** 11 tests covering phase ordering, prerequisite validation, command sequence verification, and reseed timing at phase boundaries.

**Limitations:**
- Phase 3 calibration coefficients are hardcoded from pcap (device-specific values should come from calibration data)
- Phase 6 EEPROM read is simplified (full polling protocol deferred to TASK-0111)
- Phase 3 register values from pcap may vary by device/revision
<!-- SECTION:FINAL_SUMMARY:END -->
