---
id: TASK-0102
title: RE .sal capture file format for import/export interop
status: Done
assignee: []
created_date: '2026-03-10 23:14'
updated_date: '2026-03-11 01:08'
labels:
  - re
  - ghidra
  - data-format
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-0077 confirmed .sal is NOT in ExportNode. TASK-0082 found WaveformStoreNode uses cereal binary archives with 0x90-byte headers. But the full .sal file format (the ZIP/directory structure Saleae uses for capture files) is not documented.

Investigation steps:
1. Create a capture in Saleae Logic 2, save as .sal file
2. Examine the .sal file structure (is it a ZIP? directory? single binary?)
3. Search binary for ".sal" string references and decompile handlers
4. Search Electron bundle.js for save/load capture logic
5. Document the container format and how waveform data is packed inside
6. Check if there's a version header or magic number

This enables our Rust driver to produce captures that Saleae Logic 2 can open, and read existing .sal captures for validation.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Document container format (ZIP with meta.json + .bin files)
- [x] #2 Document meta.json schema and version history
- [x] #3 Document binary data file format for digital and analog channels
- [x] #4 Document save/load flow
<!-- AC:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Fully reverse engineered the .sal capture file format from Saleae Logic 2 v2.4.40.

Key findings:
- .sal is a standard ZIP archive (zlib level 1) containing meta.json + per-channel .bin files
- meta.json has version (currently 16), session data, and binData channel-to-file mapping
- .bin files are cereal binary archives serialized by DigitalStore/AnalogStore/WaveformStoreNode
- Digital data uses RLE encoding, analog data uses int16_t sample arrays with LOD statistics
- File naming: digital-N.bin / analog-N.bin for legacy devices, instrument-P-type-C.bin for MSO
- Version upgrade chain supports versions 4-16 with automatic migration
- Full JSON schema for meta.json data field extracted from JS bundle
- Save flow: serialize stores to temp dir, write meta.json, zip to .sal
- Load flow: unzip to temp dir, parse meta.json, upgrade version, deserialize stores

Documented in re/notes/sal-file-format.md.
<!-- SECTION:FINAL_SUMMARY:END -->
