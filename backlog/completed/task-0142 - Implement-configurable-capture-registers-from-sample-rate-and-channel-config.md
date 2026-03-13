---
id: TASK-0142
title: Implement configurable capture registers from sample rate and channel config
status: Done
assignee: []
created_date: '2026-03-12 14:12'
updated_date: '2026-03-12 15:04'
labels:
  - phase-2
  - data-path
dependencies: []
references:
  - re/notes/disaggregator-interleaving.md
  - crates/saleae-driver/src/capture.rs
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The full-init command currently hardcodes capture registers from the pcap (12.5 MSa/s mixed mode, all channels). Need to compute register values from user-specified sample rate and channel selection.

Key registers: r0x08 (digital enable mask), r0x06 (analog enable mask), r0x0a/0x0b/0x0c (sample rate divider), r0x0d/0x0e (timing), r0x12/0x13/0x14 (configuration). The mapping is in SetCaptureParameters (TASK-0124, done) but not yet translated to code.

Also need to wire the disaggregator settings to match the actual capture config (digital_ratio, low_speed_divider change with rate).
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Implemented in commit 3387348. Added CaptureRegisters struct and compute_capture_registers() to capture.rs. All three CLI capture commands (full_init, capture_digital, capture_quick) now use computed registers instead of hardcoded values. Tests validate against pcap reference values.
<!-- SECTION:FINAL_SUMMARY:END -->
