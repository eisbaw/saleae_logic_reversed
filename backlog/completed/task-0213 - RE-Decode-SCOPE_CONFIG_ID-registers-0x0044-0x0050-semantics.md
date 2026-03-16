---
id: TASK-0213
title: 'RE: Decode SCOPE_CONFIG_ID registers 0x0044-0x0050 semantics'
status: Done
assignee: []
created_date: '2026-03-13 20:49'
updated_date: '2026-03-16 06:27'
labels:
  - reverse-engineering
  - fpga-registers
dependencies: []
references:
  - re/notes/capture-start-stop.md
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Registers SCOPE_CONFIG_ID_0 through _3 (0x0044, 0x0048, 0x004C, 0x0050) are written by SetConfigurationIds. An incremented value is stored at device memory offset 0x1f0. The exact semantics and format are unknown.

These may be used for identifying capture configurations or synchronizing host/device state across multiple captures.

Approach: decompile SetConfigurationIds in Ghidra. Trace where the config IDs are read back or used in capture start/stop logic.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Already documented in re/notes/config-id-semantics.md. SetConfigurationIds takes two uint64 params, packs 4 per-channel config IDs into registers 0x0044-0x0050. Monotonically incrementing counter from SetConfiguration (offset 0x1f0). FPGA embeds value in EP2 chunk header bits [31:16]. Updated fpga-registers.md with richer descriptions.
<!-- SECTION:FINAL_SUMMARY:END -->
