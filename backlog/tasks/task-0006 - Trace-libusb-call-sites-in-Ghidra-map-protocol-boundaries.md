---
id: TASK-0006
title: Trace libusb call sites in Ghidra -- map protocol boundaries
status: Done
assignee: []
created_date: '2026-03-08 22:42'
updated_date: '2026-03-08 23:49'
labels: []
dependencies:
  - TASK-0003
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Find all call sites of libusb_control_transfer, libusb_bulk_transfer, libusb_submit_transfer, libusb_cancel_transfer in libgraph_server_shared.so.

For each call site:
1. Identify the calling function and its class context
2. Trace backward: what constructs the USB packet/parameters?
3. Trace forward: what processes the response?
4. Document endpoint numbers, transfer directions, request types

Hypothesis: these call sites are the protocol boundaries. The functions that call libusb are the protocol encoders/decoders. Everything above them is business logic, everything below is USB transport.

Create follow-up backlog tasks for each distinct protocol path discovered (firmware upload path, bitstream path, capture data path, config path, calibration path).
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
## Completed: String-based analysis of libusb boundaries + Ghidra script creation

### Artifacts produced
- `re/scripts/find-usb-constants.sh` -- Automated string analysis script (15 sections)
- `re/ghidra/scripts/TraceLibusbCalls.py` -- Ghidra script to trace all libusb callers, extract parameters, classify protocol paths
- `re/ghidra/reports/usb-constants-strings.txt` -- Full string analysis output
- `re/notes/libusb-boundaries.md` -- Comprehensive findings document

### Key findings
1. All 97 libusb symbols are exported in the dynamic symbol table (address range ~0x2db9f20-0x2dc1fb0)
2. Five transfer types used: control, sync bulk, async bulk (submit_transfer), cancel, interrupt
3. Device has exactly 4 endpoints after firmware load (EP0 control, EP1 OUT/IN bulk, EP2 IN bulk)
4. C++ abstraction: LinuxUsbDevice wraps libusb, DeviceFeatures classes add protocol semantics
5. Seven distinct protocol paths identified: firmware upload, FPGA programming, FPGA registers, capture streaming, EEPROM/I2C, device config, crypto
6. Two ADC types: HMCAD1100 (Logic 4/8) and AD9637 (Logic Pro)
7. 10 distinct bitstream CRC values covering multiple FPGA variants
8. Full original source tree structure recovered from embedded path strings

### Follow-up tasks created
- TASK-0024: EEPROM/I2C tunnel protocol
- TASK-0025: Crypto/authentication protocol
- TASK-0026: ADC initialization register maps
<!-- SECTION:FINAL_SUMMARY:END -->
