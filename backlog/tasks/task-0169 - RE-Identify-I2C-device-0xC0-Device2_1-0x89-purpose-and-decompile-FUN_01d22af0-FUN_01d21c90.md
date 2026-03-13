---
id: TASK-0169
title: >-
  RE: Identify I2C device 0xC0, Device2_1 (0x89) purpose, and decompile
  FUN_01d22af0/FUN_01d21c90
status: Done
assignee: []
created_date: '2026-03-12 22:28'
updated_date: '2026-03-13 00:46'
labels:
  - RE
  - Ghidra
  - I2C
dependencies: []
references:
  - re/captures/usbmon4-ep1-decoded.txt
  - re/ghidra/decompiled-xilinx-init.txt
  - re/notes/i2c-0xC0-identification.md
  - re/notes/device2-1-opcode-0x89.md
  - re/notes/decompiled-functions-01d22af0-01d21c90.md
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Three RE unknowns from captures and completed tasks: (1) I2C device at address 0xC0 (0x60 after shift) receives calibration data, has registers 0x01/0x08/0x23/0x43/0x1C -- identity unknown, likely crypto IC or DSP. (2) Device2_1 (opcode 0x89) always called before I2C calibration reads, purpose marked "UNKNOWN". (3) FUN_01d22af0 and FUN_01d21c90 mentioned in TASK-0136 as needed for complete pre-capture init but never decompiled.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Investigation Results (2026-03-13)

### Finding 1: I2C Device 0xC0 = ATECC508A/608A Crypto Authentication IC

The I2C device at address 0xC0 (7-bit: 0x60) is a Microchip ATECC508A or ATECC608A crypto authentication IC, NOT an EEPROM. Evidence:

1. I2C address 0xC0 is the ATECC default address
2. Command/response pattern matches ATECC protocol (write command, poll with status 0x01/0x02, read result)
3. Response sizes match ATECC commands: 35 bytes = Nonce response, 67 bytes = Sign response (ECDSA signature)
4. Data written includes 32-byte host nonce for challenge-response
5. RTTI confirms CryptoInterfacePro class for Logic Pro devices
6. Source file: BasicCryptoCommands.cpp

The previously labeled "registers" (0x01, 0x04, 0x08, 0x1C, 0x23, 0x43) are actually ATECC protocol word addresses and response length bytes.

Corrected label in init-sequence-pcap.md from "I2C EEPROM" to "I2C Crypto IC (ATECC508A/608A)".

### Finding 2: Opcode 0x89 (Device2_1) = I2C Wake Token for ATECC

EP1 opcode 0x89 sends an I2C wake pulse to the ATECC crypto IC. The ATECC enters sleep mode between commands and requires SDA held low for ~60us to wake. In the capture, all 14 instances of Device2_1 appear immediately before WriteI2C/ReadI2C operations to address 0xC0. Format: 1-byte opcode, 1-byte ACK response.

### Finding 3: FUN_01d22af0 = ResetMainDcm

This is the `ResetMainDcm` method from Logic2FpgaDevice.cpp. Two paths:
- Case 0 (Xilinx Spartan-6): Staged power-down r0x00 = 0x1e->0x1c->0x18->0x08->0x00. No PLL lock check.
- Case 1 (Lattice ECP5): PLL reset, poll r0x40 for lock (0xA0 bits), SERDES bitslip recovery with up to 8 retries.

Our hardware (Logic Pro 8 Rev 1.2) uses the Spartan-6 path, which is already implemented correctly in our Rust driver.

### Finding 4: FUN_01d21c90 = CommonDeviceInit (Crypto Auth Orchestrator)

This is the crypto authentication step in the init sequence. Branches on param_1[0x44]:
- 0 = New crypto interface (CryptoInterfacePro, calls GetPublicKey vtable)
- 1 = Old crypto interface (shared_ptr<CryptoInterface>, calls vtable[0x80])
- >=2 = Assertion failure (UsesNewCryptoInterface)

Our Rust driver correctly skips this function entirely since crypto auth is host-software-only.

### Impact on Rust Driver

None. All three items are either already implemented (ResetMainDcm Spartan-6 path, ADC init) or intentionally skipped (crypto authentication). No code changes needed.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Investigated three RE unknowns. I2C device 0xC0 identified as ATECC508A/608A crypto IC (not EEPROM). Opcode 0x89 identified as I2C wake token for ATECC. FUN_01d22af0 is ResetMainDcm (Xilinx/Lattice PLL reset). FUN_01d21c90 is CommonDeviceInit (crypto auth orchestrator). No Rust driver changes needed -- all paths already implemented or intentionally skipped.
<!-- SECTION:FINAL_SUMMARY:END -->
