---
id: TASK-0195
title: Add CaptureResult teardown error reporting and filter settling time handling
status: Done
assignee: []
created_date: '2026-03-13 05:37'
updated_date: '2026-03-13 10:50'
labels:
  - driver
  - capture
  - calibration
  - quality
dependencies: []
references:
  - 'crates/saleae-driver/src/capture.rs:1467'
  - crates/saleae-proto/src/biquad.rs
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Two capture pipeline gaps: (1) Capture teardown errors (write_registers, stop_stream) are logged as warnings but never surfaced to caller. CaptureResult has no field to indicate teardown failures. Add teardown_warnings or similar. (2) IIR filter has settling time of ~impulse_length samples (e.g., 3381 at 50MHz). First N samples contain transient artifacts. Neither pipeline discards these startup samples. Consider marking or discarding the initial transient region.
<!-- SECTION:DESCRIPTION:END -->
