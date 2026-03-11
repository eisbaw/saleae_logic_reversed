---
id: TASK-0077
title: RE export_node/exporter -- Saleae data export format
status: Done
assignee: []
created_date: '2026-03-10 12:31'
updated_date: '2026-03-10 18:00'
labels:
  - re
  - ghidra
  - data-format
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Source path leaked: `/workspace/monorepo/graph-io/nodes/export_node/src/exporter.cpp`

The data export node handles writing captured data to files. Understanding this is valuable if we want to:
- Produce Saleae Logic 2 compatible output files (.sal format?)
- Import captures into Saleae for visual analysis
- Understand the on-disk capture format for interop

Investigation steps:
1. Search binary for strings like "export", "exporter", ".sal", ".csv" to find entry points
2. Decompile the exporter class
3. Document supported export formats (CSV, binary, raw, etc.)
4. Map the data flow: which nodes feed into the exporter
5. Identify file format headers, metadata, compression
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Export subsystem RE complete. Two formats only: CSV (digital.csv + analog.csv) and Binary (digital_N.bin + analog_N.bin per channel). No .sal format here -- that's in WaveformStoreNode.\n\nKey findings:\n- ExportWaveformCSV at ELF 0x2c8a570 is dead code (throws 'not implemented')\n- Export state machine: NotReady(0) -> Ready(1) -> Exporting(2)\n- Thread-based, one export at a time, background thread with cancel support\n- ExportNode struct: per-channel arrays at +0x128 (analog), +0x228 (digital), format at +0x38, cancel flag at +0xc0\n- gRPC protobuf messages recovered from Electron bundle.js for ExportDataRequest/Response\n\nAnalysis: re/notes/export-node.md"
<!-- SECTION:FINAL_SUMMARY:END -->
