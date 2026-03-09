---
id: TASK-0052
title: Capture full lsusb -v in SPI flash boot state (requires physical access)
status: To Do
assignee: []
created_date: '2026-03-09 06:20'
labels:
  - usb
  - hardware
  - descriptor
dependencies:
  - TASK-0033
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-0033 documented the two-stage boot process from dmesg but could not capture full USB descriptors in the SPI flash boot state because:
1. The device is on the xHCI root hub which does not support per-port power switching
2. USB bus reset does not cut VBUS power (FX3 RAM firmware survives)
3. No uhubctl-compatible hub is between the device and root hub

To capture full SPI boot descriptors:
1. Stop Saleae Logic 2 if running
2. Physically unplug/replug the USB cable
3. Immediately run: sudo lsusb -v -d 21a9:1005
4. The device will show bcdDevice=1.00, Manufacturer=Cypress, Product=WestBridge
5. Document full descriptor dump including all endpoints, BOS, etc.

Expected SPI boot state:
- VID:PID = 21a9:1005
- bcdDevice = 1.00
- Product = WestBridge
- Manufacturer = Cypress  
- SerialNumber = real hardware serial (0000000004BE observed in dmesg)
- USB 2.0 High Speed only

Alternative: Install a programmable USB hub (uhubctl-compatible) between the device and the root hub to enable remote power cycling.
<!-- SECTION:DESCRIPTION:END -->
