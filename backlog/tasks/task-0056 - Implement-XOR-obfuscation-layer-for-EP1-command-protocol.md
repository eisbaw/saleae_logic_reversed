---
id: TASK-0056
title: Implement XOR obfuscation layer for EP1 command protocol
status: To Do
assignee: []
created_date: '2026-03-09 11:21'
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
