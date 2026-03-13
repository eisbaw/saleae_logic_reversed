---
id: TASK-0161
title: Add HardwareRevision and AdcType domain types
status: Done
assignee: []
created_date: '2026-03-12 22:27'
updated_date: '2026-03-12 22:45'
labels:
  - proto
  - domain-model
  - architecture
dependencies: []
references:
  - crates/saleae-proto/src/device.rs
  - crates/saleae-driver/src/init.rs
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
ADC type depends on hardware revision (EEPROM), not device model. DeviceProfile has fixed adc_bits but Logic 4 can have HMCAD1100 (Xilinx rev) or ADS528x (Lattice rev). Need: (1) AdcType enum { Ad9637, Hmcad1100, Ads528x }, (2) HardwareRevision struct (FPGA variant + ADC type + board revision from EEPROM), (3) runtime profile modification after EEPROM read. Also rename FpgaType to DeviceFeatureClass (it describes software interface class, not FPGA silicon).
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added AdcType, FpgaVariant, and HardwareRevision domain types to saleae-proto. Renamed FpgaType to DeviceFeatureClass (with backward-compatible type alias). Moved HwRevision parsing from saleae-driver/fpga.rs to saleae-proto/device.rs as HardwareRevision, which now classifies FPGA variant and ADC type from the EEPROM major revision byte. Updated all references across disaggregator, capture, ep1, and fpga modules. All 858 tests pass, clippy clean.
<!-- SECTION:FINAL_SUMMARY:END -->
