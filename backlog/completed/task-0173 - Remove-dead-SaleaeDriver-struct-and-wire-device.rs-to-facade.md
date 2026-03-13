---
id: TASK-0173
title: Remove dead SaleaeDriver struct and wire device.rs to facade
status: Done
assignee: []
created_date: '2026-03-13 00:49'
updated_date: '2026-03-13 00:51'
labels:
  - driver
  - tech-debt
dependencies: []
references:
  - crates/saleae-driver/src/device.rs
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
SaleaeDriver in device.rs has stub methods (program_fpga, start_capture, stop_capture) that are never called. The real implementations live in facade.rs, fpga.rs, capture.rs. The struct is dead code. Either remove it entirely or wire it to delegate to the real implementations. Since no code uses it, removal is cleanest.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Removed dead `SaleaeDriver` struct, its impl block, `CaptureConfig` stub, `NEXT_INSTANCE_ID` static, and all associated tests from `crates/saleae-driver/src/device.rs`. Removed `pub mod device;` from `lib.rs`. Grep confirmed nothing in the workspace imported from this module -- the real implementations live in facade.rs, fpga.rs, and capture.rs. All 893 workspace tests pass.
<!-- SECTION:FINAL_SUMMARY:END -->
