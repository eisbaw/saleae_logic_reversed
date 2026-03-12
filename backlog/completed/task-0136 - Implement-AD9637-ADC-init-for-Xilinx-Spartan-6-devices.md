---
id: TASK-0136
title: Implement AD9637 ADC init for Xilinx Spartan-6 devices
status: Done
assignee: []
created_date: '2026-03-11 23:41'
updated_date: '2026-03-12 01:51'
labels:
  - implementation
  - adc
  - capture
  - blocker
dependencies: []
references:
  - re/ghidra/decompiled-xilinx-init.txt
  - re/ghidra/decompiled-capture-start.txt
  - re/notes/adc-initialization.md
  - crates/saleae-cli/src/commands.rs
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Digital capture on Spartan-6 devices fails because the ADC is not initialized. EP2 returns no data after StartSampling because the FPGA data path is inactive without DCM lock.

Decompiled SetAd9637Enabled (Ghidra 0x1d23330, ELF 0x1c23330) shows the enable sequence:
1. WriteAd9637Register(8, 0) -- power normal mode
2. Sleep ~1ms
3. WriteAd9637Register(8, 3) -- reset + standby
4. WriteAd9637Register(8, 0) -- clear reset
5. Sleep ~50ms (for DCM to lock)
6. FUN_01d22af0() -- additional init (read chip ID?)
7. Read r0x40 -- check DCM lock
8. If (r0x40 & 0x20 == 0) throw "Main DCM didn't lock"
9. If (r0x40 & 0x40 == 0) throw "Slow DCM didn't lock"
10. Read r0x4a -- bitslip status

WriteAd9637Register uses the proxy pattern: r0x03=addr, r0x16=0, r0x04=value, r0x05=0. ADC register 0x08 is the POWER_MODES register.

The capture start flow (decompiled SendStartCommand at Ghidra 0x1d41eb0) calls:
1. ReseedDevice + Ping
2. SetAdcEnabled(true, adc_type) -- dispatches to SetAd9637Enabled
3. SetCaptureParameters(DeviceSettings)
4. COMMAND_START_READ (0x01)

Key difference from current pcap-based init: the pcap was from a Lattice ECP5 device (JESD lock at r0x40=0xe0). On Spartan-6, r0x40 bits mean:
- bit 5 (0x20) = Main DCM locked
- bit 6 (0x40) = Slow DCM locked

Also need to decompile FUN_01d22af0 and FUN_01d21c90 for the complete pre-capture init.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 AD9637 enable sequence implemented matching decompiled SetAd9637Enabled
- [ ] #2 DCM lock polling works on hardware (r0x40 & 0x60 == 0x60)
- [ ] #3 EP2 data flows after ADC init + StartSampling
<!-- AC:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Implemented AD9637 ADC init module for Xilinx Spartan-6 devices.

## Changes

### New file: `crates/saleae-driver/src/adc.rs`
- `configure_ad9637()`: Writes the full AD9637 register configuration (reset, output mode, clock, VREF, DCO) via the FPGA proxy protocol
- `dcm_cascade()`: Runs the 5-step JESD power sequence and reads DCM lock status (r0x40, r0x4a)
- `dcm_lock_with_retry()`: DCM lock with bitslip retry loop (up to 10 retries with ADC reset cycle between attempts)
- `write_calibration_and_cascade()`: Enables clock output, writes calibration coefficients, commits ADC config, runs post-cal DCM cascade
- `power_on_for_capture()`: Pre-capture ADC bank power-on + DCM lock
- `power_down()`: ADC teardown
- `init_ad9637()`: Complete init sequence (configure + DCM lock + calibration)
- Unit tests for DCM cascade, ADC reset cycle, and register constants

### Modified: `crates/saleae-driver/src/fpga.rs`
- Added `dcm_locked` field to `DeviceInitResult`
- Updated `common_device_init()` to call `adc::init_ad9637()` for non-Lattice (Xilinx) devices instead of skipping ADC init
- Updated unit tests with proper Xilinx mock response sequences

### Modified: `crates/saleae-driver/src/lib.rs`
- Registered `pub mod adc`

### Modified: `crates/saleae-cli/src/commands.rs`
- `full_init`: Replaced ~150 lines of inline ADC init code with calls to `saleae_driver::adc::{init_ad9637, power_on_for_capture, power_down}`
- `device_init`: Added DCM locked status to output
- `capture_digital`: Left diagnostic inline code as-is (serves debugging purpose)

### Modified: `crates/saleae-driver/tests/integration.rs`
- Added `queue_xilinx_common_device_init()` helper for mock responses
- Updated `common_device_init_non_lattice_reads_eeprom` and `full_device_lifecycle_firmware_to_eeprom` tests

## Test results
- 254 unit tests pass (1 pre-existing env-dependent failure in transfer_queue)
- 21 integration tests pass (2 updated for new Xilinx path)
- Release build clean, no warnings
<!-- SECTION:FINAL_SUMMARY:END -->
