---
id: TASK-0175
title: Add non-equal digital/analog sample rate capture config support
status: Done
assignee: []
created_date: '2026-03-13 00:49'
updated_date: '2026-03-13 11:05'
labels:
  - driver
  - capture
  - RE
dependencies:
  - TASK-0064
references:
  - 'crates/saleae-driver/src/capture.rs:923'
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
capture.rs:923 has TODO "More pcap data needed for non-equal rate configs." Currently only equal digital+analog rates are computed correctly. Need pcap captures with different digital and analog sample rates to validate the register computation for mixed-rate configs.
<!-- SECTION:DESCRIPTION:END -->
