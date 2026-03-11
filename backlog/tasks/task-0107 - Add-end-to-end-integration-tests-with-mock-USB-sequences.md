---
id: TASK-0107
title: Add end-to-end integration tests with mock USB sequences
status: Done
assignee: []
created_date: '2026-03-10 23:14'
updated_date: '2026-03-11 01:29'
labels:
  - rust
  - testing
  - integration
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The MockTransport in saleae-usb records operations but we don't have integration tests that exercise the full device lifecycle against scripted USB responses.

Requirements:
- Test: firmware upload sequence (control transfers, re-enumeration wait)
- Test: FPGA programming sequence (reset, bitstream chunks, readback verify, post-init)
- Test: EP1 reseed + ping handshake (encryption state validation)
- Test: capture start -> stream EP2 data -> capture stop
- Test: error recovery (USB disconnect during capture)
- Each test uses MockTransport with pre-programmed responses
- Validate exact USB operation sequences match expected protocol

This doesn't require hardware but exercises the full driver stack.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Created 21 integration tests in saleae-driver/tests/integration.rs (958 lines) covering: firmware upload (vendor control transfers, chunking), FPGA programming (reset/bitstream/verify), EP1 reseed+ping handshake, capture lifecycle (start/stream/stop), error recovery (EP2 disconnect, EP1 timeout, bitstream USB error), full lifecycle (firmware->init->FPGA->post-init->EEPROM), and config validation. Enhanced MockTransport with BulkInResponse error injection. Added Clone to ProtoError/UsbError.
<!-- SECTION:FINAL_SUMMARY:END -->
