---
id: TASK-0162
title: >-
  Make EP1 write_registers ACK-read conditional on device type for non-Pro
  support
status: Done
assignee: []
created_date: '2026-03-12 22:27'
updated_date: '2026-03-12 23:12'
labels:
  - driver
  - feature
  - multi-device
dependencies: []
references:
  - 'crates/saleae-driver/src/ep1.rs:276'
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
ep1.rs:276 always reads ACK response after write_registers, but non-Pro devices don't send one. The comment says "Since our target is Pro 8, we always expect the ACK." Similarly write_config_data:365 hardcodes read_ack:true. When Logic 4/8 support is added, this will hang. The Pro/non-Pro distinction should drive ACK behavior. write_config_data_for_device:499 partially fixes this but the default methods remain traps.
<!-- SECTION:DESCRIPTION:END -->
