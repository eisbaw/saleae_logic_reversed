---
id: TASK-0128
title: 'RE LogicProfessionalDevice::TestEeprom self-test protocol'
status: To Do
assignee: []
created_date: '2026-03-11 11:21'
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
