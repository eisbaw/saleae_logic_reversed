---
id: TASK-0117
title: RE LogicProfessional8 vs LogicProfessional16 class differences
status: Done
assignee: []
created_date: '2026-03-11 07:54'
updated_date: '2026-03-12 03:06'
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

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
## RE Finding: LogicProfessional8 vs LogicProfessional16 are identical except channel count and public key

### Vtable Analysis

Both `LogicProfessional8Device` and `LogicProfessional16Device` inherit from `LogicProfessionalDevice`. Compared all 17 vtable entries:

| Vtable Index | Method | Pro8 vs Pro16 |
|---|---|---|
| 0 | (unknown, shared) | SAME |
| 1 | Destructor | DIFFERS (trivial - vtable pointer fixup only) |
| 2-12 | Various (GetDeviceSettings, Stop, etc.) | ALL SAME |
| 13 | GetPublicKey() | DIFFERS (different 65-byte ECDSA keys) |
| 14-16 | Various | ALL SAME |

### Constructor Analysis

Both call the same base constructor `LogicProfessionalDevice::LogicProfessionalDevice()` with:
- Same max digital rate: 500 MHz (0x1dcd6500)
- Same USB2 fallback rate: 50 MHz (0x2faf080)
- **Different channel count**: Pro8 passes `ecx=8`, Pro16 passes `ecx=16`
- Pro8 passes `r8d=0` (false), Pro16 passes through a bool param from caller

### GetPublicKey Difference

Each device class returns a distinct 65-byte (0x41) ECDSA P-256 uncompressed public key, XOR-obfuscated with a shared key in the binary. The keys are stored at different global addresses:
- Pro16: globals at VA 0x2eedad0-0x2eedb18
- Pro8: globals at VA 0x2eeda80-0x2eedac8

This is for Saleae's cloud authentication / licensing -- irrelevant to our open-source driver.

### RTTI Addresses (ELF VAs)
- `LogicProfessionalDevice` typeinfo: VA 0x2e48b78, vtable vfunc0: VA 0x2e489c0
- `LogicProfessional16Device` typeinfo: VA 0x2e48b90, vtable vfunc0: VA 0x2e48a58
- `LogicProfessional8Device` typeinfo: VA 0x2e48ba8, vtable vfunc0: VA 0x2e48af0

### Conclusion

**No code changes needed.** The existing `device.rs` profiles (`LOGIC_PRO_8_PROFILE` and `LOGIC_PRO_16_PROFILE`) already correctly model these devices as differing only in channel count (8 vs 16). All FPGA register configs, ADC channel mappings, sample rate tables, and behavioral methods are shared through the common `LogicProfessionalDevice` base class.
<!-- SECTION:FINAL_SUMMARY:END -->
