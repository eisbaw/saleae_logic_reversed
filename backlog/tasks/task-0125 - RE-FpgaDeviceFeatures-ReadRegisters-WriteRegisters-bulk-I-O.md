---
id: TASK-0125
title: RE FpgaDeviceFeatures ReadRegisters/WriteRegisters bulk I/O
status: Done
assignee: []
created_date: '2026-03-11 11:20'
updated_date: '2026-03-12 02:59'
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
  - re/ghidra/decompiled-register-ops.txt
  - re/ghidra/scripts/DecompileRegisterOps.java
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

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
## Decompilation Results

Five functions decompiled from libgraph_server_shared.so via Ghidra headless:

### FpgaDeviceFeatures::WriteRegisters (non-Pro, ELF VA 0x1c535f0)
- Opcode 0x80, wire format: [0x80, count, addr0, val0, addr1, val1, ...]
- Assertion: `num_registers < 32` (max 31 pairs)
- Returns silently if count == 0
- Fire-and-forget: NO response read for non-Pro devices

### FpgaDeviceFeatures::WriteRegisters (Pro override, ELF VA 0x1c549d0)
- Same wire format as non-Pro
- Same assertion: max 31 pairs
- Reads 1-byte response; throws "Failed to write registers" if byte != 0x00

### FpgaDeviceFeatures::ReadRegisters (ELF VA 0x1c53af0)
- Opcode 0x81, wire format: [0x81, count, addr0, addr1, ...]
- Assertion: `(count - 1) <= 0x3D`, so valid range is 1..=62 (NOT 63)
- Allocates response buffer of exactly `count` bytes, reads via vtable
- Response is raw values, no count prefix byte

### FpgaDeviceFeatures::WriteRegisterBit (ELF VA 0x1c53e30)
- Signature: `WriteRegisterBit(U8 addr, U8 bit_value, U8 bit_position, const S8* caller)`
- Asserts bit_position < 8 and bit_value is 0 or 1
- Classic read-modify-write: reads register, sets/clears bit, writes back
- Uses vtable dispatch for both read and write

### FpgaDeviceFeatures::WriteConfigurationData (ELF VA 0x1c544b0)
- Opcode 0x7F
- Non-Pro: 2-byte header [0x7F, len_8bit], max 62 data bytes, no response
- Pro: 3-byte header [0x7F, len_lo, len_hi], max 509 (USB2) or 1021 (USB3) data bytes
- Pro reads 1-byte response; throws "Failed to write configuration data" if != 0x00
- Max sizes fill exactly one USB max packet (64/512/1024)

## Code Changes

1. **Fixed ReadRegisters max count**: 63 -> 62 (matches firmware assertion)
2. **Added WriteRegisters ACK validation**: Pro devices now fail on non-zero ACK byte
3. **Implemented write_register_bit()**: read-modify-write for single-bit ops
4. **Updated docs**: corrected batch limits, added ELF VA references, documented WriteConfigData max sizes
5. **Added 3 new tests**: write_register_bit set, clear, and panic on invalid position

## Key Finding: No Batching Surprise

Both WriteRegisters and ReadRegisters already support batching (multiple registers per EP1 command). Our implementation was correct on this point. The only bugs were the ReadRegisters max count being off by 1, and the missing ACK validation for WriteRegisters on Pro devices.
<!-- SECTION:FINAL_SUMMARY:END -->
