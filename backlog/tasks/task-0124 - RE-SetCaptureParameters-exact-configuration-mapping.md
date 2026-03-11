---
id: TASK-0124
title: RE SetCaptureParameters exact configuration mapping
status: To Do
assignee: []
created_date: '2026-03-11 11:20'
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
