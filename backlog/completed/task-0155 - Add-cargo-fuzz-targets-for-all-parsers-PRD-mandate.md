---
id: TASK-0155
title: Add cargo-fuzz targets for all parsers (PRD mandate)
status: Done
assignee: []
created_date: '2026-03-12 22:27'
updated_date: '2026-03-13 00:15'
labels:
  - testing
  - proto
  - PRD
dependencies: []
references:
  - 'PRD.md:251'
  - crates/saleae-proto/src/calibration.rs
  - crates/saleae-proto/src/firmware/fx3.rs
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
PRD mandates "cargo-fuzz: parser fuzzing (mandatory for all packet parsers)" but no fuzz/ directory exists. Need fuzz targets for: parse_calibration() (untrusted XML from server), Fx3Image::parse() (firmware binary), ChunkHeader/StartPacketFields/StatsTrailer::parse() (EP2 wire data), Disaggregator (raw EP2 byte streams).
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added 9 cargo-fuzz targets covering every parser entry point in saleae-proto:

- fx3_image: FX3 firmware binary parser (pre-existing)
- parse_calibration: Boost.Serialization XML from cal server
- chunk_header: 4-byte EP2 chunk header
- start_packet_fields: 12-byte start packet extension
- stats_header: 20-byte stats trailer
- parse_data_chunk: composite EP2 data chunk parser (parameterized)
- disaggregator_pro: Pro device deinterleaver via process_chunk
- sal_reader: .sal zip archive parser
- obfuscation: EP1 encrypt_command/decrypt_response

All targets compile on stable. Requires nightly to run (`cargo fuzz run <target>`).
Fuzz crate excluded from workspace via its own `[workspace]` key.
Commit: 0fcc263
<!-- SECTION:FINAL_SUMMARY:END -->
