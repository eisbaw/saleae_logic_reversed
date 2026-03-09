---
id: TASK-0064
title: 'End-to-end validation: compare Rust output against Saleae Logic 2 captures'
status: To Do
assignee: []
created_date: '2026-03-09 11:22'
labels:
  - validation
  - hardware-testing
  - e2e
dependencies:
  - TASK-0061
  - TASK-0063
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The ultimate validation: capture the same signal with both Saleae Logic 2 and our Rust driver, compare sample-by-sample.

Steps:
1. Set up a test signal on nvdk-msdlab-09 (e.g., 1kHz square wave from a signal generator, or a simple GPIO toggle from another device)
2. Capture with Saleae Logic 2 (export as CSV or binary)
3. Capture with saleae-cli (same sample rate, same channels, same duration)
4. Compare:
   - Digital: bit-exact match expected
   - Analog: match within calibration tolerance (< 1 LSB after IIR filter)
5. Test at multiple sample rates: 1MHz, 10MHz, 50MHz
6. Test with multiple channel configurations
7. Document any discrepancies and their root causes

This is the deliverable defined in the PRD: "correct digital and analog samples from the device, with calibration properly applied."

Success criteria:
- Digital captures are bit-exact
- Analog captures match within 1% of Saleae Logic 2 output
- No USB transfer overflow at sustained capture rates
<!-- SECTION:DESCRIPTION:END -->
