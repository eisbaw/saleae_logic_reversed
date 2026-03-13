---
id: TASK-0200
title: 'Identify EP2 stats flags bits 8, 11, 13, 14, 15 semantics'
status: Done
assignee: []
created_date: '2026-03-13 10:09'
updated_date: '2026-03-13 11:17'
labels:
  - RE
  - EP2
  - Ghidra
dependencies: []
references:
  - crates/saleae-proto/src/capture_data.rs
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The stats header flags bitfield (u16) in EP2 chunks has bits 9 (overflow_meta), 10 (debug_log), and 12 (overflow_data) identified. Bits 8, 11, 13, 14, 15 are extracted in the binary but their semantic meaning is unknown.

Need to trace downstream usage of these flags in Ghidra to determine if they indicate error states, mode changes, ADC saturation, or other conditions. The stats parser is at VA 0x2d41510.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Each flag bit's purpose documented or marked as definitively unused
- [ ] #2 capture_data.rs StatsHeader updated with named flag fields
<!-- AC:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Identified semantics of EP2 stats header flag bits 8, 11, 13, 14, 15 by decompiling HandleWaveformData (VA 0x2d44b80) and tracing the log format string arguments through the logging function FUN_02e4cef0.

Results:
- bit 8 = "end" (end-of-waveform segment) -- High confidence (log literal)
- bit 9 = "start" (renamed from overflow_meta) -- High confidence (log literal)
- bit 10 = "software" (renamed from debug_log) -- High confidence (log literal)
- bit 11 = reserved (extracted but unused in known callers) -- Medium confidence
- bit 12 = overflow_data (unchanged) -- High confidence
- bit 13 = unknown (inverted and stored in chunk descriptor) -- Low confidence
- bit 14 = "prev_overflow" (single-bit boolean) -- High confidence (log literal)
- bit 15 = "cur_overflow" (single-bit boolean) -- High confidence (log literal)

Added stats_flags constant module, 8 new accessor methods (with backwards-compatible aliases), and 6 new tests. Commit: f932247.
<!-- SECTION:FINAL_SUMMARY:END -->
