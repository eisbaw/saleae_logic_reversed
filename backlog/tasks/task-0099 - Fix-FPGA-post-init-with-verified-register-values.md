---
id: TASK-0099
title: Fix FPGA post-init with verified register values
status: Done
assignee: []
created_date: '2026-03-10 23:13'
updated_date: '2026-03-10 23:42'
labels:
  - rust
  - saleae-driver
  - implementation
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
saleae-driver/src/fpga.rs has PLACEHOLDER register values for DDR init, JESD link, GPIF configuration. TASK-0068 documented 59 FPGA register functions with actual addresses and bit layouts.

Requirements:
- Replace placeholder register values with verified ones from doc/protocol/fpga-registers.md
- DDR_INIT sequence: correct register addresses and expected ready values
- JESD204B link initialization: correct ADC init registers per device type
- GPIF state machine configuration: correct table from TASK-0071 (27 entries)
- Capture control registers: start/stop, sample rate divisor, channel enable mask
- Digital configuration address formula: CalculateDigitalConfigurationAddress from TASK-0068

Reference: doc/protocol/fpga-registers.md, re/notes/fpga-register-map.md
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Replaced PLACEHOLDER register values with verified ones from TASK-0068 decompilation.\n\nKey corrections:\n- JESD_READY_MASK: 0x63 -> 0x21 (link_up=bit0, cpll_locked=bit5)\n- JESD PHY reset: write value 2, not 1/0 toggle\n- SPI_DATA layout: RW bit 23, wire_config bit 24, clock_edge bit 26\n- Analog registers renamed: per-parameter not per-channel (sample_divider, pre_trigger, post_trigger, computed_rate, trigger_high, samples_per_chunk)\n- Added 21 FEAT_* constants for FpgaDeviceFeatures (Logic 4/8/Pro 8-bit path)\n- SmartCable address formula: 0x0120 + port*16 + sub_ch*4\n- New registers: SCOPE_CHANNEL_CONFIG (0x0194), SMART_CABLE_BASE (0x0120)\n- JESD lane masks: 4ch=0xFF, 2ch=0x5A\n\n3 values still await pcap confirmation: ADC SPI addresses, INIT_CONFIG value, CAPTURE_CONTROL initial.\n\nCommit: e79b26c"
<!-- SECTION:FINAL_SUMMARY:END -->
