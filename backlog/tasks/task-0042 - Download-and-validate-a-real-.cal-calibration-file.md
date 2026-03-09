---
id: TASK-0042
title: Download and validate a real .cal calibration file
status: To Do
assignee: []
created_date: '2026-03-09 01:02'
labels:
  - calibration
  - validation
dependencies:
  - TASK-0013
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Download a real calibration file from downloads.saleae.com/calibration/{deviceIdHex}.cal using the Logic Pro 8 device connected to nvdk-msdlab-09. Parse the Boost.Serialization XML and validate:
1. XML structure matches documented format
2. Count biquad sections per channel per sample rate (expected: 5)
3. Verify a0 is always 1.0
4. Document available sample rates in the calibration data
5. Verify fullScaleVoltageRanges per channel
6. Check mAdcGain value
7. This validates TASK-0013 findings against real data
<!-- SECTION:DESCRIPTION:END -->
