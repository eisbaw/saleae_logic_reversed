---
id: TASK-0024
title: RE EEPROM / I2C tunnel protocol -- calibration data path
status: Done
assignee: []
created_date: '2026-03-08 23:49'
updated_date: '2026-03-09 04:41'
labels:
  - usb-protocol
  - calibration
  - i2c
dependencies:
  - TASK-0006
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
String analysis reveals EEPROM read/write goes through FX3 I2C tunnel. Key strings: "ReadI2c request/response", "WriteI2C request/response", "I2c tunnel error code", "TransferI2c returned unexpected number of bytes". FX2 has 256-byte EEPROM, FX3 uses larger. MSO uses AT24C64D (64Kbit). The I2C tunnel likely uses vendor control transfers with specific bRequest values. EEPROM probably stores calibration data, serial number, and device metadata. Trace Fx2DeviceFeatures::ReadEeprom, Fx3DeviceFeatures::ReadEeprom, and LogicProfessionalDevice::TestEeprom in Ghidra to extract the control transfer parameters used for I2C tunneling.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Completed comprehensive analysis of the I2C/EEPROM tunnel protocol through static binary analysis of libgraph_server_shared.so v2.4.40.\n\n## Key Findings\n\n### Three I2C Access Paths\n\n1. **FX2 vendor control transfers** (Logic, Logic 16) -- EP0 control transfers with i2c_err_t returns, 256-byte EEPROM (24C02), 64-byte pages\n2. **FX3 EP1 command tunnel** (Logic 4/8/Pro 8/16) -- opcodes 0x55 (WriteEeprom), 0x87 (WriteI2C), 0x88 (ReadI2c)\n3. **MSO I2C tunnel** (MSO devices) -- AT24C64D EEPROM (8192 bytes, 32-byte pages), MsoDeviceInterface\n\n### New Opcode Discovered: 0x87 (WriteI2C)\nRaw I2C write tunnel, distinct from WriteEeprom (0x55). Used for crypto IC communication. MOV byte [rbp+0x00], 0x87 at VA 0x1c5157d. Logs \"WriteI2C request/response\".\n\n### I2C Bus Devices\n- EEPROM: stores device ID, hardware revision, metadata\n- Crypto IC (ATECC508A/608A): device authentication via ECDSA, challenge-response, 32-byte block operations\n- Three crypto interface implementations: CryptoInterfaceI2c (FX2), CryptoInterfaceFpga (FX3), CryptoInterfacePro (Pro)\n- I2C bus segmented via GPIO: I2C_RAIL_EN1, I2C_RAIL_EN2\n\n### Calibration Data Path\n- Logic 4/8/Pro: EEPROM stores device ID only; calibration downloaded from Saleae servers by JS frontend\n- MSO: calibration stored on-device in AT24C64D EEPROM (AfeMetadata, CalibrationConstants)\n\n## Deliverables\n- `re/notes/i2c-eeprom-protocol.md` -- complete analysis with code regions, confidence levels\n- `re/ghidra/scripts/TraceI2CTunnel.py` -- Ghidra script for automated I2C tunnel analysis\n- Updated `doc/protocol/ep1-commands.md` -- added WriteEeprom, WriteI2C, ReadI2c detailed formats\n- Updated `re/notes/ep1-command-protocol.md` -- 13 opcodes (was 12)\n\n## Method\nRIP-relative LEA scanning with correct .text/.rodata VA delta correction (+0x1000), immediate value extraction for command opcodes, RTTI type name analysis for class hierarchy, cross-reference with TASK-0047 EP1 command protocol findings."
<!-- SECTION:FINAL_SUMMARY:END -->
