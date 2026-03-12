---
id: TASK-0135
title: Fix is_lattice detection -- currently assumes all FX3 devices are Lattice
status: Done
assignee: []
created_date: '2026-03-11 23:41'
updated_date: '2026-03-12 01:40'
labels:
  - bugfix
  - init
  - fpga
dependencies: []
references:
  - crates/saleae-cli/src/commands.rs
  - re/notes/eeprom-layout.md
  - re/ghidra/decompiled-xilinx-init.txt
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
In commands.rs (lines 332, 435, 447), `is_lattice` is set to `device.device_type.is_fx3()` which is WRONG. All FX3 devices are treated as Lattice ECP5 but:

- Logic Pro 8 Rev 0/1 = Xilinx Spartan-6 SLX9 with AD9637 ADC
- Logic Pro 8 Rev 4+ = Lattice ECP5 with ADS528x ADC (JESD204B)
- Logic 4/8 Rev 0/1 = Xilinx Spartan-6 with HMCAD1100 ADC

The FPGA type is determined by hardware revision from EEPROM, not device type. The RevisionDetail struct contains FpgaType (0=Xilinx, 1=Xilinx-alt, 2=Lattice ECP5) and AdcType (0=AD9637, 1=ADS528x, 2=unknown).

Fix: read EEPROM hardware revision (addr 0x20, 6 bytes: "REV" + major.minor) to determine FPGA type, then select correct init path. For our test device (Rev 1.2), FpgaType=Xilinx, AdcType=AD9637.

The current code runs the Lattice init path (JESD power sequence, DDR polling) which fails on Xilinx devices because JESD registers don't exist.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Fixed `is_lattice` detection to use EEPROM hardware revision instead of `device.device_type.is_fx3()`.

Changes:
- Added `HwRevision` struct and `read_hw_revision()` to `crates/saleae-driver/src/fpga.rs` that parses EEPROM offset 0x20 ("REV" + major.minor) and determines Xilinx vs Lattice from the major revision byte
- Fixed all 3 occurrences in `crates/saleae-cli/src/commands.rs` where `is_lattice` was incorrectly derived from `is_fx3()` -- now reads actual EEPROM revision via EP1
- Fixed 5 unit tests and 4 integration tests that were broken by a pre-existing change to `program_fpga` verification (write-then-readback of r0x7F instead of just reading r0x00)
- Added 10 unit tests for `HwRevision` parsing and Lattice detection

Mapping from `ParseBoardRevision` switch statement:
- Major 0, 1, 7 = Xilinx Spartan-6 (not Lattice)
- Major 3, 4, 5, 6, 8, 9, 10, 11 = Lattice ECP5
- Major 2 = unsupported (throws in binary)
<!-- SECTION:FINAL_SUMMARY:END -->
