---
id: TASK-0097
title: Implement software trigger engine in saleae-proto
status: Done
assignee: []
created_date: '2026-03-10 23:13'
updated_date: '2026-03-11 00:36'
labels:
  - rust
  - saleae-proto
  - implementation
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-0067 documented the software trigger engine. Implement edge and pulse triggers.

Requirements:
- EdgeTrigger: iterate digital data for transitions matching multi-channel pattern
  - Rising, Falling, Any edge detection
  - Multi-channel AND pattern (all channels must match simultaneously)
  - Returns 60-bit timestamp of trigger point
- PulseTrigger: state machine measuring edge-to-edge duration
  - Min/max pulse width constraints
  - Active-high and active-low pulse detection
- TriggerConfig: Vec of trigger conditions (edge or pulse per channel)
- Pre-trigger buffer management: keep N samples before trigger point
- No hardware trigger support needed (confirmed MSO-only by TASK-0091)
- Unit tests with synthetic digital waveforms

Reference: re/notes/trigger-engine.md
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Implemented software trigger engine in `crates/saleae-proto/src/trigger.rs` with 21 unit tests.\n\nTypes: EdgeType (Rising/Falling/AnyEdge), PulsePolarity (ActiveHigh/ActiveLow), TriggerCondition (Edge/Pulse), TriggerConfig.\n\nComponents:\n- EdgeDetector: tracks previous digital values per channel, detects transitions matching configured edge type. Multi-channel AND logic requires all conditions to fire on the same sample.\n- PulseDetector: state machine (WaitingForStart -> Measuring -> Triggered) that measures pulse width and triggers when duration falls within [min_samples, max_samples]. Fires at trailing edge.\n- TriggerEngine: combines edge and pulse detectors with AND logic. One-shot trigger with reset/re-arm. Pre-trigger sample offset applied via saturating subtraction.\n\nAll algorithms match the binary's behavior documented in TASK-0067 RE analysis (EdgeTrigger_Detect at 0x01be1960, PulseTrigger_Detect at 0x01be23c0).
<!-- SECTION:FINAL_SUMMARY:END -->
