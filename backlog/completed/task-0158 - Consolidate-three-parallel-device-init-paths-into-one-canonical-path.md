---
id: TASK-0158
title: Consolidate three parallel device init paths into one canonical path
status: Done
assignee: []
created_date: '2026-03-12 22:27'
updated_date: '2026-03-12 23:48'
labels:
  - driver
  - architecture
  - tech-debt
dependencies: []
references:
  - crates/saleae-driver/src/init.rs
  - crates/saleae-driver/src/fpga.rs
  - crates/saleae-driver/src/capture.rs
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Three ways to init+capture exist: (1) DeviceSession in init.rs (pcap-replay), (2) program_fpga + fpga_post_init + common_device_init in fpga.rs (modular), (3) configure_capture + start_capture in capture.rs (register-computation). They overlap significantly. CaptureParams::default() is a literal pcap replay while CaptureConfig computes registers from high-level params. Pick one canonical path, make others delegate to it. DeviceSession should use fpga.rs functions, and CaptureParams should be removed in favor of CaptureConfig.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Consolidated three parallel device init paths into one canonical path by making DeviceSession a thin orchestrator that delegates to the hardware-validated implementations:

- Phase 2: delegates to fpga::program_fpga (reset + upload + r0x7F verify) instead of bare ep1.write_config_data_for_device
- Phase 3: delegates to adc::init_ad9637 (hardware-validated ADC init) instead of inline pcap replay
- Phase 7: accepts CaptureConfig (computed registers) instead of CaptureParams (hardcoded pcap values), delegates to adc::power_on_for_capture + capture::configure_capture
- Phase 8: delegates to adc::power_down instead of inline teardown

Removed CaptureParams struct entirely -- CaptureConfig is the single capture configuration type. Net -250 lines. All 858 tests pass, clippy clean.
<!-- SECTION:FINAL_SUMMARY:END -->
