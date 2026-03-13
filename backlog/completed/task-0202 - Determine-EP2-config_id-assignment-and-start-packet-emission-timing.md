---
id: TASK-0202
title: Determine EP2 config_id assignment and start packet emission timing
status: Done
assignee: []
created_date: '2026-03-13 10:10'
updated_date: '2026-03-13 11:39'
labels:
  - RE
  - EP2
dependencies: []
references:
  - crates/saleae-proto/src/capture_data.rs
  - doc/protocol/capture-sequence.md
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Two EP2 framing questions:

1. **config_id** (16-bit field in chunk header): Used for stream synchronization and detecting configuration changes. Not clear if it's a counter, hash, or mode ID, how it's assigned at capture start, or what triggers a change.

2. **Start packet emission timing**: Start packets (is_start=1) contain FPGA timestamp + host time. Unknown if emitted once per capture, once per configuration change, periodically, or on other events.

Both affect robust stream parsing — if config_id can change mid-capture, the parser needs to handle it.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 config_id assignment logic documented
- [x] #2 Start packet emission conditions documented
- [x] #3 Parser handles config_id changes if needed
<!-- AC:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
## Findings

### config_id is a monotonically incrementing counter

Decompiled `MsoOscilloscopeInstrument::SetConfiguration()` (ELF VA 0x2d477d0) reveals a counter at object offset 0x1f0 that increments by 1 on each call. The return type is `ConfigurationId`. Protected by mutex.

### FPGA register write path (MSO only)

`SetConfigurationIds` (ELF VA 0x2d64230, mso_fpga.cpp) writes config_id values to 4 FPGA registers: SCOPE_CONFIG_ID_0..3 at addresses 0x0044-0x0050. One uint32 per channel. The FPGA embeds this in chunk header bits [31:16].

### Logic Pro 8 (Spartan-6) has no config_id registers

`Logic2FpgaDevice::SetCaptureParameters` (ELF VA 0x1c24fb9) writes registers 0x06-0x14, 0x1A-0x1B but NO config_id registers. The 8-bit register space does not include SCOPE_CONFIG_ID equivalents. config_id in chunk headers from these devices is expected to be 0.

### Start packet emission: once per channel per capture

Start packets (is_start=1) are emitted once per enabled channel at capture start. The DigitalCreatorNode handler (ELF VA 0x2d51710) manages per-channel state (stride 0x30 bytes at base 0x4d8). On config_id mismatch, a warning is logged but packets are NOT dropped. On duplicate start packets, time ordering is checked and state is reset.

## Files changed

- `crates/saleae-proto/src/capture_data.rs` -- expanded ChunkHeader and StartPacketFields doc comments with config_id semantics and start packet timing
- `doc/protocol/capture-sequence.md` -- added resolved questions for config_id and start packet timing
- `re/notes/start-packet-format.md` -- moved config_id from open to resolved questions
- `re/notes/config-id-semantics.md` -- NEW: full analysis document
- `re/ghidra/scripts/DecompileConfigId.java` -- NEW: Ghidra script
- `re/ghidra/decompiled-config-id.txt` -- NEW: decompilation output
<!-- SECTION:FINAL_SUMMARY:END -->
