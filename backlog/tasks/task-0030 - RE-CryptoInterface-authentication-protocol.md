---
id: TASK-0030
title: RE CryptoInterface authentication protocol
status: To Do
assignee: []
created_date: '2026-03-09 00:21'
labels:
  - crypto
  - authentication
  - fpga
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Reverse engineer the ATECC-style crypto authentication used during FPGA programming and device initialization. Three implementations exist: CryptoInterfaceFpga, CryptoInterfaceI2c, CryptoInterfacePro. Uses ECDSA signing (secp521r1 referenced). Source: devices/crypto_device/src/crypto_device/crypto_interface.cpp. Key questions: Is auth required before FPGA programming? What happens if auth fails? Is this anti-cloning or license enforcement?
<!-- SECTION:DESCRIPTION:END -->
