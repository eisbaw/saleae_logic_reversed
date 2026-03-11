---
id: TASK-0117
title: RE LogicProfessional8 vs LogicProfessional16 class differences
status: To Do
assignee: []
created_date: '2026-03-11 07:54'
labels:
  - re
  - ghidra
  - multi-device
dependencies: []
references:
  - crates/saleae-proto/src/device.rs
  - re/notes/device-class-hierarchy.md
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Strings confirm LogicProfessional8Device and LogicProfessional16Device are distinct C++ classes. Our driver treats them as the same code path with different channel counts, but there may be other behavioral differences.

Investigation:
1. Find vtable entries for both classes
2. Decompile any methods that differ between them
3. Check for different FPGA register configs, ADC channel mappings, or sample rate tables
4. Document any differences beyond channel count

If they're truly identical except channel count, close this task. If there are real differences, update DeviceProfile in device.rs.
<!-- SECTION:DESCRIPTION:END -->
