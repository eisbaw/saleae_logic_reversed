---
id: TASK-0025
title: RE crypto/authentication protocol -- device anti-clone
status: To Do
assignee: []
created_date: '2026-03-08 23:49'
labels:
  - usb-protocol
  - crypto
  - authentication
dependencies:
  - TASK-0006
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
String analysis found CryptoInterfaceFpga and CryptoInterfaceI2c classes with ReadTransmission, SerializeI2cTransfer operations. Source files: BasicCryptoCommands.cpp, crypto_interface_devices.cpp, crypto_interface.cpp, crypto_interface_i2c.cpp, cryptography.cpp. Also found SimpleEncryption.cpp. This appears to be a device authentication/anti-clone mechanism. It uses both FPGA SPI and I2C paths. LogicProfessionalDevice has GetPublicKey method. May need to understand and handle this for the open driver to work with real devices.
<!-- SECTION:DESCRIPTION:END -->
