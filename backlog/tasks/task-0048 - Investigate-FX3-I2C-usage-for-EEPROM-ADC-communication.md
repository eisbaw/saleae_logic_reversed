---
id: TASK-0048
title: Investigate FX3 I2C usage for EEPROM/ADC communication
status: Done
assignee: []
created_date: '2026-03-09 02:33'
updated_date: '2026-03-09 08:14'
labels:
  - firmware
  - i2c
  - calibration
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The FX3 firmware calls InitI2C() during initialization. I2C is likely used for one or more of:
- EEPROM access (calibration data, serial number, device identity)
- ADC configuration (the Lattice bitstream design names reference ADS528x ADC family)
- Analog front-end control

The host-side strings include ReadEeprom, WriteEeprom, and TestEeprom methods on Fx3DeviceFeatures and LogicProfessionalDevice classes.

Understanding the I2C bus topology would clarify:
- How calibration data is stored/retrieved
- How the ADC is configured for different sample rates
- Whether the serial number comes from EEPROM or firmware

See: re/notes/fx3-usb-events.md (I2C references)
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Covered by TASK-0024 (I2C tunnel protocol with opcodes 0x55/0x87/0x88, EEPROM access, crypto IC) and TASK-0026 (ADC init via FPGA SPI bridge register 0x0004). I2C bus topology: EEPROM + ATECC crypto IC on I2C bus (segmented via GPIO rails), ADC access via FPGA SPI bridge not I2C. See re/notes/i2c-eeprom-protocol.md and re/notes/adc-initialization.md.
<!-- SECTION:FINAL_SUMMARY:END -->
