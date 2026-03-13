---
id: TASK-0167
title: Consolidate MockTransportWrapper into shared test utility
status: Done
assignee: []
created_date: '2026-03-12 22:28'
updated_date: '2026-03-12 23:18'
labels:
  - testing
  - tech-debt
  - driver
dependencies: []
references:
  - 'crates/saleae-driver/src/eeprom.rs:644'
  - 'crates/saleae-driver/src/adc.rs:479'
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Identical 25-line MockTransportWrapper struct with manual UsbTransport impl delegation is copy-pasted across eeprom.rs, adc.rs, hmcad.rs, jesd.rs, and integration.rs. When UsbTransport gains a new method, every copy must be updated. Extract to a shared testutil module or make MockTransport implement UsbTransport directly.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Implemented `UsbTransport for Arc<MockTransport>` in `saleae-usb/src/mock.rs`, eliminating the identical 25-line `MockTransportWrapper` newtype that was copy-pasted across 9 test modules (adc.rs, capture.rs, eeprom.rs, ep1.rs, fpga.rs, hmcad.rs, init.rs, jesd.rs, integration.rs). Removed ~315 lines of duplicated delegation code. All 862 tests pass, zero clippy warnings. Commit: b6731cf.
<!-- SECTION:FINAL_SUMMARY:END -->
