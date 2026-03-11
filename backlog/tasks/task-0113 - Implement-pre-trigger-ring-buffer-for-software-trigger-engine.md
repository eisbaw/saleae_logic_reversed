---
id: TASK-0113
title: Implement pre-trigger ring buffer for software trigger engine
status: Done
assignee: []
created_date: '2026-03-11 05:59'
updated_date: '2026-03-11 06:40'
labels:
  - rust
  - saleae-driver
  - trigger
dependencies: []
references:
  - re/notes/trigger-engine.md
  - crates/saleae-proto/src/trigger.rs
  - crates/saleae-driver/src/capture.rs
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-0097 implemented the trigger detection algorithms (EdgeDetector, PulseDetector, TriggerEngine) but there's no pre-trigger buffer management. The Saleae device uses software-only triggers -- the host must buffer samples before the trigger fires and then keep N samples after.

Implementation:
1. Pre-trigger ring buffer: fixed-size circular buffer that holds the last N samples
2. Post-trigger counter: after trigger fires, collect exactly M more samples then stop
3. TriggerCapture orchestrator that wraps TriggerEngine + ring buffer:
   - Accumulating phase: fill pre-trigger buffer, run trigger detection on each sample
   - Triggered phase: drain pre-trigger buffer + collect post-trigger samples
   - Done: return combined pre+post samples
4. Integration with capture pipeline: TriggerCapture sits between disaggregator output and export writers
5. Configurable pre-trigger and post-trigger sample counts

Reference: re/notes/trigger-engine.md, crates/saleae-proto/src/trigger.rs
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Created trigger_buffer.rs in saleae-proto with: RingBuffer&lt;T&gt; (generic circular buffer, fixed capacity, push/drain/overflow), TriggerCapture (3-phase state machine: Accumulating->Collecting->Done wrapping TriggerEngine + RingBuffer), TriggerCaptureResult (pre_trigger_data, post_trigger_data, trigger_sample_index, trigger_channel). 29 tests covering all edge cases including capacity=0, immediate trigger, large buffers, multi-channel, falling edge.
<!-- SECTION:FINAL_SUMMARY:END -->
