---
id: TASK-0182
title: Add unit tests for ADC init functions (zero coverage on 5 public functions)
status: Done
assignee: []
created_date: '2026-03-13 05:36'
updated_date: '2026-03-13 07:19'
labels:
  - testing
  - driver
  - adc
dependencies: []
references:
  - crates/saleae-driver/src/adc.rs
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
adc.rs has 5 public functions with ZERO test coverage: configure_ad9637(), set_ad9637_enabled_true/false(), init_ad9637(), power_on_for_capture(), power_down(). These perform complex multi-step hardware register sequences. Register ordering bugs would cause silent hardware misconfiguration. Need mock-based unit tests verifying command sequences, register values, and error paths.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added 15 mock-based unit tests for all 5 public ADC functions in adc.rs, bringing coverage from zero to comprehensive sequence verification.

Tests added:
- configure_ad9637: verifies exact 9-register write sequence (reset, power, test mode, output adjust, DCO, output modes, clock, VREF) and error propagation on mid-sequence failure
- set_ad9637_enabled_false: verifies SPI transfer commit followed by 5-step JESD cascade and status reads
- set_ad9637_enabled_true: verifies power cycle sequence, DCM lock checks (main DCM, slow DCM), and bitslip detection error paths
- power_on_for_capture: verifies bank power write (r0x15=0x03) followed by full enable sequence
- power_down: verifies bank power off (r0x15=0x00) followed by disable sequence, plus error propagation
- init_ad9637: verifies full 59-command init sequence count, correct ordering of sub-phases, and DCM lock failure propagation
- reset_main_dcm: verifies DcmLockResult interpretation for locked vs unlocked states

All tests use the existing MockTransport pattern with composable helper functions for response setup and payload assertion. Commit: 0239c98
<!-- SECTION:FINAL_SUMMARY:END -->
