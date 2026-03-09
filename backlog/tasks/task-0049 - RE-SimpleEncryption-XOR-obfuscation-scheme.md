---
id: TASK-0049
title: RE SimpleEncryption XOR obfuscation scheme
status: Done
assignee: []
created_date: '2026-03-09 06:11'
updated_date: '2026-03-09 08:41'
labels:
  - firmware
  - protocol
  - investigation
dependencies:
  - TASK-0028
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
ReseedDevice sends 4 random bytes as a shared XOR seed. The seed is used by SetControlByte() in SimpleEncryption.cpp to obfuscate EP1 command traffic. Need to reverse the exact XOR scheme: is it rolling XOR, PRNG-seeded, or cyclic over the 4 bytes? This affects whether our Rust driver needs to implement the obfuscation layer or if it can be bypassed (e.g., by not sending a reseed). Also need to determine which commands are obfuscated -- all EP1 traffic after reseed, or only specific commands?
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Fully reverse engineered the SimpleEncryption XOR obfuscation scheme from static analysis of libgraph_server_shared.so v2.4.40.

## Key Findings

1. **Three encryption modes** based on DeviceType enum:
   - DeviceType 0-1 (FX2 devices): substitution cipher with fixed constants, no seed
   - DeviceType 4-5 (Logic 4, Logic 8): seed-based XOR only during firmware download
   - DeviceType 6-7 (Logic Pro 8, Logic Pro 16): seed-based XOR on ALL EP1 commands

2. **XOR algorithm**: Cyclic XOR with 4 bytes of the 32-bit seed. Write path prepends a control byte (opcode AND 0xD7, with firmware download flag in bit 3). Read path XORs without control byte.

3. **LFSR seed advancement**: After each write AND each read, both host and firmware advance the seed through a 32-bit LFSR with taps at bits 1, 21, 22. Iteration count is data-dependent: (seed AND 0x1F) + 0x23 (35-66 steps).

4. **Cannot be bypassed for Pro devices**: The firmware decodes using its seed copy. Unencrypted data would be garbled. Zero-seed bypass is theoretically possible but untested and unnecessary.

5. **DeviceType enum**: Mapped all 8 values to VID:PID pairs via GetVidPidForDevice jump table.

## Rust Driver Impact

The Rust driver MUST implement:
- ReseedDevice (4 random bytes to EP1 OUT)
- Control byte construction and prepend
- Cyclic XOR encrypt/decrypt
- LFSR advancement after each EP1 write and read
- Lockstep maintenance between host and device

Estimated ~50 lines of Rust code for the full scheme.

## Deliverables
- re/notes/xor-obfuscation.md -- complete analysis with algorithms, addresses, confidence levels
<!-- SECTION:FINAL_SUMMARY:END -->
