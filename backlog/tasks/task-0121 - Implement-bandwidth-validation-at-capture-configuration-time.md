---
id: TASK-0121
title: Implement bandwidth validation at capture configuration time
status: Done
assignee: []
created_date: '2026-03-11 07:55'
updated_date: '2026-03-12 08:46'
labels:
  - rust
  - saleae-driver
  - usb
dependencies: []
references:
  - re/notes/digital-compression.md
  - crates/saleae-driver/src/capture.rs
  - crates/saleae-proto/src/sample_rate.rs
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-0114 discovered there is no on-device compression. Bandwidth is managed by rejecting sample rate / channel combinations that exceed USB speed limits:

- USB 2.0 (High Speed): 300,000,000 bps (37.5 MB/s)
- USB 3.0 (Super Speed): 3,000,000,000 bps (375 MB/s)

The driver must validate: rate * active_channels * bytes_per_sample <= max_bitrate

Also from TASK-0114:
- hasLimitedBandwidth flag at device descriptor offset +0x59
- GetInterfaceMaximumBitRate() lookup table
- LowSpeedDivider for bus-width padding removal

Implementation:
1. Add validate_bandwidth(config, usb_speed) -> Result to saleae-driver
2. Check during configure_capture() before sending to device
3. Return clear error message listing the constraint violation
4. Include the FPGA packet size limits: Xilinx min=256/max=7680, Altera min=64/max=6016

Reference: re/notes/digital-compression.md (actually documents bandwidth, not compression)
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Already implemented in capture.rs. CaptureConfig::validate_bandwidth() checks USB 2.0 (37.5 MB/s) and USB 3.0 (375 MB/s) limits, FPGA packet size constraints (Xilinx 256-7680, Altera 64-6016), and rejects Low/Full speed. Called from configure_capture() before EP1 register writes. 13+ unit tests covering all edge cases. 454 workspace tests pass.
<!-- SECTION:FINAL_SUMMARY:END -->
