---
id: TASK-0019
title: Explore Cypress FX3 USB event handler strings
status: Done
assignee: []
created_date: '2026-03-08 22:45'
updated_date: '2026-03-09 02:34'
labels: []
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The binary contains CY_U3P_USB_EVENT_* strings that map directly to the Cypress FX3 SDK event types. These reveal the firmware's USB event handling.\n\nKnown strings:\n- CY_U3P_USB_EVENT_CONNECT\n- CY_U3P_USB_EVENT_EP0_STAT_CPLT\n- CY_U3P_USB_EVENT_RESET / CY_U3P_USB_EVENT_DISCONNECT\n- CY_U3P_USB_EVENT_RESUME\n- CY_U3P_USB_EVENT_SETCONF. data:%d\n- CY_U3P_USB_EVENT_SETINTF. data:%d\n- CY_U3P_USB_EVENT_SET_SEL\n- CY_U3P_USB_EVENT_SPEED\n- CY_U3P_USB_EVENT_SUSPEND\n- "CyU3PUsbSetDesc failed, %d"\n- "CyU3PUsbStart failed, %d"\n- "Error\! Invalid USB speed."\n- "Endpoint count is not 4. The device is likely programmed with firmware from a different version"\n\nHypothesis: these strings are in the firmware itself, compiled into the .so as embedded firmware blobs. Cross-referencing their locations with binwalk may help locate firmware images.\n\nAlternatively they may be debug logging for simulating/testing the firmware protocol on the host side.\n\nKey finding: "Endpoint count is not 4" tells us the device exposes exactly 4 endpoints after firmware load.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Confirmed that ALL CY_U3P_USB_EVENT_* strings are inside the embedded FX3 firmware blobs, not in host-side code. Cross-referenced string file offsets against firmware blob regions (Pro 8: 0x0079E7A0-0x007BBBB0, Pro 16: 0x007BBBB0-0x007D8FC0). Both firmware images contain identical strings at identical relative offsets.\n\nKey findings:\n- Firmware has 3 source files: CommandProcessing.c, ProUsb.c, LogicPro.c\n- Runs on Express Logic ThreadX RTOS (ARM9/RVDS G5.1.5.1) with 8 system threads\n- USB event callback (LogicProEventLoop) handles 10 event types via switch dispatch\n- SETCONF event triggers endpoint config + DMA channel creation + GPIF socket setup\n- Separate USB SETUP handler logs vendor control transfers (bType, bTarget, Request, Value, Index, Length)\n- Command processing loop (process_raw_command) on EP1 OUT/IN handles host commands\n- DMA architecture: BulkSrcSink for capture data, separate command TX/RX buffers\n- 'Error! Invalid USB speed.' exists in BOTH firmware (0x7D40) and host code (0x36309A) as separate strings\n- 'Endpoint count is not 4' is host-side only (0x4208C6)\n\nDocumented in re/notes/fx3-usb-events.md. Created follow-up tasks: TASK-0046 (GPIF descriptor extraction), TASK-0047 (EP1 command protocol), TASK-0048 (I2C/EEPROM investigation).
<!-- SECTION:FINAL_SUMMARY:END -->
