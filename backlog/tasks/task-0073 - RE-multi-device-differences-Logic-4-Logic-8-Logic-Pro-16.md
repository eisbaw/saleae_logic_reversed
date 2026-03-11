---
id: TASK-0073
title: 'RE multi-device differences (Logic 4, Logic 8, Logic Pro 16)'
status: Done
assignee: []
created_date: '2026-03-10 12:23'
updated_date: '2026-03-10 17:05'
labels:
  - re
  - ghidra
  - multi-device
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Current implementation is validated only against Logic Pro 8 (21a9:1005) with Lattice ECP5 FPGA. Need to decompile device-specific code paths for:

- Logic 4 (21a9:1003) -- channel count, analog support (may have none), FPGA variant
- Logic 8 (21a9:1004) -- non-Pro differences, HMCAD1100 ADC init vs ADS528x
- Logic Pro 16 (21a9:1006) -- 16 digital + 8 analog channels, any protocol differences
- Older Xilinx Spartan-6/7-series hardware revisions (AD9637 ADC)
- Per-device GetDeviceSettings differences beyond sample rates
- Per-device firmware/bitstream selection logic
- Per-device FPGA register address differences

Goal: identify what breaks if we try to use the Pro 8 code path on other devices.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Comprehensive multi-device comparison written to re/notes/multi-device-differences.md (commit dba4733).

Synthesized findings from 20+ existing RE notes into a compatibility matrix covering all 4 device classes across 15 comparison dimensions.

Key findings:
1. Logic 4 (FX2LP, 8051) CANNOT share the FX3 driver -- completely different USB controller, firmware format (Intel HEX vs FX3 CY image), no encryption, no FPGA register verification.
2. Logic 8 (FX3) shares significant infrastructure with Pro 8 but requires per-device dispatch for: encryption mode (firmware-only vs all-command XOR), register write verification (absent on non-Pro), disaggregator type (type 1 vs type 4/5/6), sample rate tier (AnalogBasic vs AnalogAdvanced), and AFE control (fixed gain vs full VGA/attenuator pipeline).
3. Logic Pro 16 is nearly identical to Pro 8 -- parameterize channel count (16 digital, 8 analog), use 16-slot (not 32-slot) deinterleaving, handle USB 2.0 channel restriction (channels 8-15 become digital-only).
4. ADC type is determined by RevisionDetail (hardware revision), not device model -- same device model can have different ADCs across manufacturing runs.
5. Recommended Rust trait hierarchy: DeviceDescriptor (static config), FpgaInterface (register I/O with optional verification), Disaggregator (per-device deinterleaving), and AfeController (gain/offset pipeline).
<!-- SECTION:FINAL_SUMMARY:END -->
