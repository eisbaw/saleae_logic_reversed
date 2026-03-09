---
id: TASK-0025
title: RE crypto/authentication protocol -- device anti-clone
status: Done
assignee: []
created_date: '2026-03-08 23:49'
updated_date: '2026-03-09 04:49'
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

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Binary searched for all crypto/auth strings (CryptoInterface, ATECC, ECDSA, challenge, nonce, etc.)
- [x] #2 Cross-referenced with existing findings (i2c-eeprom, fpga-bitstream, device-class-hierarchy, device-state-machine)
- [x] #3 Authentication flow analyzed: happens during init after FPGA programming, uses ECDSA P-256 challenge-response with ATECC508A/608A
- [x] #4 Failure mode identified: causes DSError terminal state (host-software enforced, not firmware)
- [x] #5 Open driver implications assessed: can skip crypto auth entirely, FX3 firmware is transparent I2C tunnel
- [x] #6 Documented in re/notes/crypto-authentication.md
<!-- AC:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Completed analysis of the Saleae device crypto/authentication protocol.

Key findings:

1. The crypto IC is a Microchip ATECC508A or ATECC608A (32-byte blocks, 16 slots, I2C wake, ECDSA P-256).

2. Three CryptoInterface transport variants exist: CryptoInterfaceI2c (FX2 devices), CryptoInterfaceFpga (FX3/FPGA devices), CryptoInterfacePro (Logic Pro devices).

3. Authentication uses ECDSA P-256 challenge-response: host sends random nonce, device signs with private key, host verifies signature using known public key via OpenSSL.

4. Authentication happens as the LAST step of device initialization, AFTER FPGA programming is complete.

5. Authentication is HOST-SOFTWARE enforced only. The FX3 firmware is a transparent I2C tunnel with no crypto-related strings or gating logic. The FPGA bitstreams are unencrypted. The EP1 command protocol has no authentication gate.

6. For the open Rust driver: crypto authentication can be skipped entirely. No ATECC driver, ECDSA handling, or OpenSSL dependency is needed. After FPGA programming and CommonDeviceInit, the device is ready for capture.

Files created: re/notes/crypto-authentication.md
<!-- SECTION:FINAL_SUMMARY:END -->
