---
id: TASK-0103
title: Implement capture data export (CSV + binary) in saleae-cli
status: Done
assignee: []
created_date: '2026-03-10 23:14'
updated_date: '2026-03-11 01:19'
labels:
  - rust
  - saleae-cli
  - implementation
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-0077 documented the two export formats. Implement them so captured data can be saved to files.

Requirements:
- CSV export: digital.csv (timestamp, channel values) + analog.csv (timestamp, voltage values)
- Binary export: digital_N.bin (per-channel raw bits) + analog_N.bin (per-channel raw f32 voltages)
- saleae-cli subcommands: capture-digital --output=dir/ and capture-analog --output=dir/
- Streaming write during capture (don't buffer entire capture in memory)
- Include header row in CSV with channel names and sample rate

Reference: re/notes/export-node.md
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Implemented capture data export in saleae-proto/src/export.rs with 4 streaming writers: DigitalCsvWriter, AnalogCsvWriter, DigitalBinaryWriter, AnalogBinaryWriter. Convenience types DigitalExporter/AnalogExporter dispatch by ExportFormat. CLI updated with --output and --format flags on capture-digital/capture-analog commands. 19 tests. Binary analog uses f32 to match Saleae format.
<!-- SECTION:FINAL_SUMMARY:END -->
