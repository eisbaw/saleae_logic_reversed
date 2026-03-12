---
id: TASK-0128
title: 'RE LogicProfessionalDevice::TestEeprom self-test protocol'
status: Done
assignee: []
created_date: '2026-03-11 11:21'
updated_date: '2026-03-12 04:47'
labels:
  - re
  - ghidra
  - eeprom
dependencies: []
references:
  - re/notes/eeprom-layout.md
  - re/notes/i2c-eeprom-protocol.md
  - crates/saleae-driver/src/eeprom.rs
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
LogicProfessionalDevice::TestEeprom() performs an EEPROM self-test. Understanding this reveals:
- The expected EEPROM contents for a healthy device
- Any checksum or integrity validation
- Error recovery behavior

Investigation:
1. Decompile TestEeprom
2. Document what addresses are read and what values are expected
3. Check if this runs during normal init or only on demand
4. Compare against our eeprom.rs implementation
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
## RE Results

Decompiled `LogicProfessionalDevice::TestEeprom()` at ELF VA 0x1c44010 (Ghidra 01d44010), source: `LogicProfessionalDevice.cpp` line 84.

### Algorithm
1. **Write phase**: Writes incrementing byte pattern (0x00, 0x01, ..., wrapping at 256) to EEPROM addresses 0x0008..0x0FFF in 256-byte chunks. First 8 bytes (CY header + VID/PID) preserved.
2. **Read phase**: Reads entire 4096-byte EEPROM via `ReadEeprom` (16 x 256-byte chunks at offsets 0x0000..0x0F00).
3. **Verify phase**: Compares bytes at offset 8 onward against expected incrementing pattern. Asserts "EEprom error detected!" on mismatch.

### Key findings
- EEPROM is 4096 bytes (0x1000) on FX3 Pro devices
- TestEeprom is **destructive** -- overwrites device serial, calibration, HW revision
- Called only via vtable (3 vtable entries) -- manufacturing/diagnostic test, NOT part of normal init
- FX3 ReadEeprom uses opcode 0x07 (ReadVersion) with params `[0x33, 0x81, addr_lo, addr_hi, count_lo, count_hi]`
- FX3 WriteEeprom uses opcode 0x07 with 5-byte header `[0x33, 0x81, addr_lo, count]` then sends data in second EP1 exchange
- Also decompiled `WipeEeprom`: writes 38 bytes of 0xFF at offset 0 (wipes CY header too)

### Implementation
- Added `read_eeprom(address, count)` to `Ep1CommandChannel` -- typed wrapper around ReadVersion I2C tunnel
- Added `write_eeprom(address, data)` to `Ep1CommandChannel` -- two-step EP1 write transaction
- Added `eeprom_read_all()` -- reads full 4096-byte EEPROM in 16 chunks
- Added `eeprom_self_test()` -- full write/read/verify cycle with `EepromTestResult` struct
- Added constants: `FX3_EEPROM_SIZE`, `EEPROM_TEST_START_OFFSET`, `EEPROM_CHUNK_SIZE`
- All unit tests pass (28 eeprom tests + full workspace green)

### Decompiled output saved
- `re/ghidra/decompiled-test-eeprom.txt`
<!-- SECTION:FINAL_SUMMARY:END -->
