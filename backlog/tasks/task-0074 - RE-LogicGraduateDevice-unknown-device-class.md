---
id: TASK-0074
title: RE LogicGraduateDevice -- unknown device class
status: Done
assignee: []
created_date: '2026-03-10 12:31'
updated_date: '2026-03-10 17:19'
labels:
  - re
  - ghidra
  - device-class
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Source path leaked: `/workspace/monorepo/graph-io/devices/logic_interface/src/LogicGraduateDevice.cpp`

This is a device class we haven't seen before. Not Logic, not Pro, not Student, not MSO. Could be:
- An unreleased product tier
- A renamed/rebranded device
- An educational edition (between Student and Pro?)
- A regional variant

Investigation steps:
1. Search binary for strings containing "Graduate" to find assertion messages
2. Find xrefs to those strings to locate the class vtable
3. Decompile key virtual methods (GetDeviceSettings, channel counts, sample rates)
4. Determine VID:PID and how it maps to our known device list
5. Check if it shares firmware/bitstream with any known device

This may reveal a device we need to support or can safely ignore.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Fully reverse engineered LogicGraduateDevice (Logic 8, VID:PID 21a9:1004).

Key findings:
- 17 vtable entries decompiled -- destructors, Initialize, GetUsbSpeed, SetCaptureParametersWrapper, GetDeviceSettings, StartSampling, StopDevice, GetPublicKey, ReadEepromBlock, WriteRegisterArray, CreateCryptoInterface, plus 4 shared methods
- Constructor at ELF VA 0x1c3d1a0: creates FpgaDeviceFeatures (NOT ProFpgaDeviceFeatures), 3 EP endpoints (0x01, 0x81, 0x82), triple-reads device ID for consistency
- 8 identical channels, all toggleable between digital and analog, max 5.0V
- Max digital 200 MHz, max analog 20 MHz, 12 bits/sample
- Sample rates validated as integer divisors of 200 MHz (digital) and 20 MHz (analog)
- Stream start = EP1 0x01, stream stop = EP1 0x02
- GetPublicKey returns 65-byte uncompressed ECDSA P-256 public key (XOR-deobfuscated)
- CryptoInterfaceFpga for Xilinx revisions, CryptoInterfaceI2c for Lattice revisions
- Forced speed defaults to USB 2.0 (unlike Pro which defaults to USB 3.0)
- Register 0x17 used as serialized FPGA config port (up to 16 bytes per write)

Output: `re/notes/logic-graduate-device.md`, `re/ghidra/scripts/DecompileGraduate.java`, `re/ghidra/scripts/DecompileGraduate2.java`, `re/ghidra/decompiled-graduate.txt`, `re/ghidra/decompiled-graduate2.txt`
<!-- SECTION:FINAL_SUMMARY:END -->
