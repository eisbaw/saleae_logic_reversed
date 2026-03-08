---
id: TASK-0024
title: RE EEPROM / I2C tunnel protocol -- calibration data path
status: To Do
assignee: []
created_date: '2026-03-08 23:49'
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
