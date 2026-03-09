---
id: TASK-0060
title: 'Implement CommonDeviceInit (ADC init, PLL lock, JESD link)'
status: Done
assignee: []
created_date: '2026-03-09 11:21'
updated_date: '2026-03-09 13:36'
labels:
  - rust
  - hardware-testing
  - adc
  - fpga
dependencies:
  - TASK-0059
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
After FPGA programming, the device needs ADC initialization before it can capture.

Implementation:
1. Detect ADC type from hardware revision (ADS528x for current-gen Lattice ECP5 devices)
2. Configure FPGA SPI bridge (register 0x0004) for ADC access
3. Initialize ADC via SPI writes through FPGA
4. For ADS528x: establish JESD204B link
   - Reset JESD PHY transceivers
   - Wait for PLL lock (spll, cpll, lane PLLs)
   - Run bitslip detection/correction
   - Verify link_up and sync_status
5. Wait for DDR ready
6. Read EEPROM device ID via I2C tunnel (opcode 0x88)

This is the most complex init step. Failure modes include:
- ADC calibration timeout
- JESD bitslip failure (retry up to N times)
- PLL lock failure

Test: After CommonDeviceInit, the device should be in Ready state. EEPROM device ID should be readable.

References: re/notes/adc-initialization.md, re/notes/jesd204b-link.md, doc/protocol/fpga-registers.md, re/notes/i2c-eeprom-protocol.md
<!-- SECTION:DESCRIPTION:END -->
