---
id: TASK-0124
title: RE SetCaptureParameters exact configuration mapping
status: Done
assignee: []
created_date: '2026-03-11 11:20'
updated_date: '2026-03-12 02:43'
labels:
  - re
  - ghidra
  - capture
  - protocol
dependencies: []
references:
  - re/notes/capture-start-stop.md
  - re/notes/start-packet-format.md
  - re/notes/init-sequence-pcap.md
  - crates/saleae-driver/src/capture.rs
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Logic2FpgaDevice::SetCaptureParameters(DeviceSettings &) is the function that translates a user's capture config (sample rate, channels, voltage ranges) into the exact FPGA register writes and EP1 commands that start a capture. This is the single most important function for capture correctness.

Investigation:
1. Find assertion strings referencing SetCaptureParameters
2. Decompile the function (likely large -- it dispatches per device type)
3. Document the complete mapping: DeviceSettings fields -> register writes
4. Compare against pcap Phase 7 (capture params + StartSampling)
5. Identify any fields our CaptureConfig struct is missing
6. Check how analog vs digital-only configs differ

This bridges the gap between our high-level CaptureConfig and the low-level register writes. Currently our capture.rs sends registers based on RE notes, but we haven't verified the complete mapping.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Decompile SetCaptureParameters and document register mapping
- [x] #2 Rewrite configure_capture to use WriteRegisters (0x80) not WriteConfigData (0x7F)
- [x] #3 Add device context fields to CaptureConfig
- [x] #4 Add readback verification matching binary behavior
- [x] #5 Update all unit and integration tests
- [x] #6 Cross-validate register values against pcap Phase 7
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-03-11: Pcap Phase 7 gives exact register values for one capture config. 12 registers written via WriteRegisters (0x80), NOT WriteConfigData (0x7F). The current capture.rs configure_capture() is WRONG -- it uses 0x7F. Implemented pcap-exact capture in capture_digital CLI command. Awaiting hardware test to confirm EP2 data flows.

2026-03-12: Rewrote configure_capture() to use WriteRegisters (0x80) with RE-validated register mapping from decompiled SetCaptureParameters (Ghidra 01d24ca0). CaptureConfig expanded with device context fields (total_device_channels, board_revision, is_pro, base_digital_rate, base_analog_rate, analog_channels). New build_capture_register_pairs() builds Vec<(u8, u8)> of up to 15 FPGA registers matching decompilation order. Readback verification via ReadRegisters (0x81). Removed old placeholder voltage_to_threshold_code() and build_capture_config_blob(). Updated all unit tests (capture.rs) and integration tests to use new register-based mock flow. 403 unit tests + 21 integration tests pass. Clippy clean.
<!-- SECTION:NOTES:END -->
