---
id: TASK-0159
title: Extract CLI device-open/init boilerplate into driver facade functions
status: Done
assignee: []
created_date: '2026-03-12 22:27'
updated_date: '2026-03-12 23:58'
labels:
  - cli
  - driver
  - architecture
  - tech-debt
dependencies: []
references:
  - crates/saleae-cli/src/commands.rs
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The pattern of "find device, check boot state, determine encryption mode, open, create transport, create channel, initialize" is duplicated 6+ times across commands.rs (ensure_device_ready, try_ping_ep1, attempt_reset_to_bootloader, ping, program_fpga, device_init, reset_device, capture_quick). The PRD says saleae-cli should be a "thin CLI binary" but command handlers contain 150-250 lines of driver-level orchestration. Extract into saleae-driver facade.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added `saleae_driver::facade` module with three functions that extract repeated CLI boilerplate:

- `open_and_connect()`: find device, check boot state, determine encryption mode, open USB, create + initialize EP1 channel. Returns `ConnectedDevice` bundling channel with profile/speed/device info.
- `reconnect_with_channel()`: wrap an existing channel (from `ensure_device_ready`) with device metadata.
- `init_for_capture()`: FPGA program + EEPROM cal read + ADC init + EP1 re-init + channel enables + ADC power-on.
- `encryption_mode_for_device()`: shared helper replacing 5 inline copies.

Refactored 11 functions in commands.rs. Net -161 lines from CLI. All 858 tests pass, clippy clean. Commit: ae4ed9f.
<!-- SECTION:FINAL_SUMMARY:END -->
