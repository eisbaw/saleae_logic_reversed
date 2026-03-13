---
id: TASK-0171
title: >-
  Add --device/--serial flag for multi-device selection and Ctrl-C graceful
  shutdown
status: To Do
assignee: []
created_date: '2026-03-12 22:28'
labels:
  - cli
  - feature
  - multi-device
  - PRD
dependencies: []
references:
  - crates/saleae-cli/src/main.rs
  - 'crates/saleae-cli/src/commands.rs:1131'
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
All CLI commands use find_first_saleae()/open_first_device() with no way to select among multiple connected devices. PRD Slice 6 requires multi-device support. Also, capture commands bail on duration<=0 with "Ctrl-C not yet supported" (line 1131). Duration=0 should mean "until Ctrl-C" per PRD. Need: (1) --device/--serial flag on all commands, (2) SIGINT handler for graceful capture shutdown.
<!-- SECTION:DESCRIPTION:END -->
