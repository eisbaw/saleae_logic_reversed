---
id: TASK-0010
title: RE device state machine and USB descriptor layout
status: To Do
assignee: []
created_date: '2026-03-08 22:43'
labels: []
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Map the full device state machine from Ghidra analysis and USB captures.

Known strings suggesting states:
- "Cannot start capture while device is not in Ready state"
- "Done with usb stream, {} successful transfers"
- CY_U3P_USB_EVENT_CONNECT, _DISCONNECT, _RESET, _SUSPEND, _RESUME, _SETCONF, _SETINTF

Investigate:
1. What are all device states? (disconnected, connected, bootloader, firmware-loaded, fpga-programmed, idle, configured, capturing, error)
2. What triggers each state transition?
3. What USB descriptors does the device expose at each state? (before FW, after FW, after FPGA)
4. How does the host detect current state?
5. Dump full lsusb -v output at each stage

Once confirmed, promote state machine diagram to doc/state-machine/
<!-- SECTION:DESCRIPTION:END -->
