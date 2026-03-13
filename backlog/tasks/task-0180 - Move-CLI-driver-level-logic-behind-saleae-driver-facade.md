---
id: TASK-0180
title: Move CLI driver-level logic behind saleae-driver facade
status: Done
assignee: []
created_date: '2026-03-13 05:35'
updated_date: '2026-03-13 09:17'
labels:
  - architecture
  - cli
  - driver
  - tech-debt
dependencies: []
references:
  - crates/saleae-cli/src/commands.rs
  - crates/saleae-cli/src/diag.rs
  - crates/saleae-driver/src/facade.rs
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The CLI directly depends on saleae-usb and nusb, bypassing the driver layer. commands.rs calls saleae_usb::discovery functions directly (6 locations), diag.rs uses nusb::Interface directly. ensure_device_ready implements complex auto-recovery logic (boot state check, firmware upload, EP1 reset) that belongs in the driver. The analog calibration pipeline construction (BiquadFilter + VoltageTransform + AnalogProcessor) also belongs in the driver. Goal: CLI should only depend on saleae-driver, with dependency graph cli -> driver -> usb -> proto.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Moved ensure_device_ready auto-recovery logic into facade::connect_or_recover(). Added PowerCycleRequired error variant. CLI reduced by 170+ lines.
<!-- SECTION:FINAL_SUMMARY:END -->
