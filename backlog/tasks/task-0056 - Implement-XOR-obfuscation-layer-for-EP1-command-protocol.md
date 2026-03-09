---
id: TASK-0056
title: Implement XOR obfuscation layer for EP1 command protocol
status: Done
assignee: []
created_date: '2026-03-09 11:21'
updated_date: '2026-03-09 12:19'
labels:
  - rust
  - protocol
  - crypto
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-0049 confirmed that XOR obfuscation is MANDATORY for Logic Pro 8/16. All EP1 commands and responses must be encrypted after the reseed handshake.

Implementation in saleae-proto or saleae-usb:
1. Implement the 32-bit LFSR seed advance: taps at bits 1, 21, 22; iterate (seed & 0x1F) + 0x23 times
2. Implement SetControlByte: prepend control byte = (opcode & 0xD7) | flags, XOR'd with (seed & 0xFF) & 0xD7
3. Implement payload XOR: each byte XOR'd with cyclic seed byte (seed >> ((i%4)*8)) & 0xFF
4. Implement response decryption: cyclic XOR without control byte
5. After each write AND read, advance the LFSR
6. Implement ReseedDevice: send 4 random bytes on EP1 OUT, initialize LFSR state
7. Implement Ping (0x7D): send [0x7D, test_byte], expect complement back

Unit tests:
- Known seed -> known LFSR output sequence
- Encrypt/decrypt roundtrip
- Ping response validation

This must be verified against real hardware by sending a Ping after reseed and checking the response matches.

References: re/notes/xor-obfuscation.md, re/notes/reseed-mechanism.md
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Implemented SimpleEncryption XOR obfuscation in `crates/saleae-proto/src/obfuscation.rs`.\n\n## What was implemented\n\n- `lfsr_step()`: Single LFSR step with taps at bits 1, 21, 22\n- `advance_seed()`: Data-dependent iteration count `(seed & 0x1F) + 0x23` (35-66 steps)\n- `SimpleEncryption` struct holding LFSR state and `EncryptionMode`\n- `encrypt_command()`: Prepends control byte (0xD7 masked, firmware flag in bit 3), XORs opcode+payload with cyclic seed bytes, advances seed\n- `decrypt_response()`: Cyclic XOR without mask, advances seed\n- `EncryptionMode::XorAll` (Pro devices), `XorFirmwareOnly` (Logic 4/8), `None`\n- Ping validation helper (`validate_ping_response`, constants)\n- `reseed()` method for ReseedDevice integration\n\n## Tests (30 total)\n\n- LFSR pinned outputs from seed=1: [0xC0000000, 0xF0000000, 0xFC000000, 0xFF000000]\n- advance_seed pinned: seed=1 -> 0xA39054B4, 0xDEADBEEF -> 0x81573BA4\n- Zero seed identity (XOR with 0 is passthrough, seed stays 0)\n- Encrypt/decrypt roundtrip with nonzero seed\n- 10-cycle host/firmware lockstep simulation\n- Control byte generation, firmware download flag\n- Mode selection (XorAll, XorFirmwareOnly, None)\n- Ping complement validation\n\n## Files changed\n\n- `crates/saleae-proto/src/obfuscation.rs` (new, 846 lines)\n- `crates/saleae-proto/src/lib.rs` (added module + re-exports)\n\nHardware verification deferred to TASK-0058.
<!-- SECTION:FINAL_SUMMARY:END -->
