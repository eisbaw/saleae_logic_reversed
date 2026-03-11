---
id: TASK-0097
title: Implement software trigger engine in saleae-proto
status: Done
assignee: []
created_date: '2026-03-10 23:13'
updated_date: '2026-03-11 00:37'
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
Software trigger engine in saleae-proto/src/trigger.rs.\n\nEdgeType (Rising/Falling/AnyEdge), PulsePolarity (ActiveHigh/ActiveLow), TriggerCondition enum, TriggerConfig with AND logic.\n\nEdgeDetector: tracks previous value per channel, check_all() returns true only when ALL edge conditions match on same sample (multi-channel AND from FUN_01be3a10).\n\nPulseDetector: state machine WaitingForStart -> Measuring -> Triggered. Measures edge-to-edge duration, accepts [min_samples, max_samples]. Early rejection on exceeded max.\n\nTriggerEngine: combines edge+pulse detectors, one-shot trigger, pre-trigger offset via saturating subtraction, reset/re-arm.\n\n21 tests covering all edge types, multi-channel AND, pulse filtering, mixed conditions, pre-trigger, reset."
<!-- SECTION:FINAL_SUMMARY:END -->
