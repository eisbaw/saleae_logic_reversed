---
id: TASK-0069
title: RE DisaggregatorSettingsData and mixed analog+digital interleaving
status: Done
assignee: []
created_date: '2026-03-10 12:23'
updated_date: '2026-03-10 15:59'
labels:
  - re
  - ghidra
  - data-format
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Decompile the data disaggregation layer that separates interleaved analog and digital samples from the USB stream. Partially covered by TASK-0035 and TASK-0038 but gaps remain. Need:

- Full DisaggregatorSettingsData struct layout
- DisaggregatorNode processing logic
- How analog and digital samples are interleaved at different sample rates
- Per-channel sample extraction from interleaved stream
- Handling of mixed-rate configurations (e.g. digital at 100MHz, analog at 50MHz)
- Chunk boundary alignment rules

This is needed to correctly parse captures that use both analog and digital channels simultaneously.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Decompiled DisaggregatorNode::ProcessChunk (ELF 0x1aea5d0) with device-type dispatch to 4 deinterleaving functions. Pro 8/16 interleaves digital_group_bytes + analog_group_bytes per cycle, with byte-swap for endianness and SSE psraw(4) for 12-bit ADC sign extension. DeviceCalculateMixRate uses GCD for digital/analog sample ratios. Settings struct is 0xa0 bytes with channel enable arrays (32 slots). Three GPIF/ADC combos: 16-bit/10-bit, 16-bit/12-bit, 32-bit/12-bit. Created re/notes/disaggregator-interleaving.md (600 lines) and 5 Ghidra scripts.
<!-- SECTION:FINAL_SUMMARY:END -->
