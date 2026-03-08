---
id: TASK-0006
title: Trace libusb call sites in Ghidra -- map protocol boundaries
status: To Do
assignee: []
created_date: '2026-03-08 22:42'
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
