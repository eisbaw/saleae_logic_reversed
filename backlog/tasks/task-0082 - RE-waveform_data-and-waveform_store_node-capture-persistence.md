---
id: TASK-0082
title: RE waveform_data and waveform_store_node -- capture persistence
status: Done
assignee: []
created_date: '2026-03-10 12:32'
updated_date: '2026-03-10 19:05'
labels:
  - re
  - ghidra
  - data-format
dependencies: []
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Source paths leaked:
- `/workspace/monorepo/graph-io/core/waveform_data/src/waveform_data/waveform_data_collection.cpp`
- `/workspace/monorepo/graph-io/nodes/waveform_store_node/src/waveform_store_node/waveform_store_node.cpp`
- `/workspace/monorepo/graph-io/nodes/waveform_summary_node/src/waveform_summary_node/waveform_summary_node.cpp`

The waveform persistence and summary pipeline. Reveals:
- How captured data is stored to disk (.sal file format internals)
- Waveform downsampling for zoom levels (summary node)
- Data collection lifecycle (create, append, finalize)
- Relationship to the export_node

Investigation steps:
1. Search for strings like "waveform_data", "waveform_store", ".sal"
2. Decompile the storage node
3. Document the on-disk format
4. Check if waveform_summary_node does LOD (level-of-detail) downsampling
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Waveform persistence uses cereal binary archives for serialization/deserialization on background threads.\n\nPackedWaveformData: 0x90-byte headers + variable-length int16_t analog sample arrays. Wire format: chunk_count, then per-chunk: interval + header + analog_data + stats_data.\n\nMSO metadata: 16-byte magic tokens (<SALEAE_MSOMAIN>, <SALEAE_MSOLASC>, <SALEAE_MSOAFE_>) with CRC8 + uint16_t length framing.\n\nLOD downsampling: min/max summaries at 6 power-of-2 reduction levels (16 through 512 samples).\n\nThree parallel store types (WaveformStoreNode, AnalogStore, DigitalStore) share same state machine pattern. ExportWaveformCSV confirmed as unimplemented stub.\n\n115 functions decompiled (14,610 lines). Analysis: re/notes/waveform-persistence.md"
<!-- SECTION:FINAL_SUMMARY:END -->
