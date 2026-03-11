---
id: TASK-0098
title: Implement multi-device abstraction traits in saleae-driver
status: Done
assignee: []
created_date: '2026-03-10 23:13'
updated_date: '2026-03-10 23:56'
labels:
  - rust
  - saleae-driver
  - implementation
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-0073 recommended Rust abstractions for multi-device support. Implement trait-based device dispatch.

Requirements:
- DeviceDescriptor trait/struct: channel counts, supported rates, FPGA type, ADC type
- FpgaInterface trait: register read/write, bitstream upload, with variants for:
  - FpgaDeviceFeatures (8-bit addr, Logic 4/8) 
  - ProFpgaDeviceFeatures (8-bit addr, Logic Pro 8/16)
  - MsoFpga (16-bit addr, MSO -- stub only)
- Disaggregator trait: device-specific EP2 deinterleaving
- AfeController trait: analog front-end configuration per device
- DeviceFactory: VID:PID -> concrete device implementation (from TASK-0084)
  - Logic 4: FX2LP path (stub, incompatible with FX3 driver)
  - Logic 8: FpgaDeviceFeatures, 8 digital + 8 analog
  - Logic Pro 8: ProFpgaDeviceFeatures, 8 digital + 8 analog
  - Logic Pro 16: ProFpgaDeviceFeatures, 16 digital + 16 analog (ch8-15 restricted on USB 2.0)

Reference: re/notes/multi-device-differences.md, re/notes/devices-manager.md, re/notes/logic-graduate-device.md
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Multi-device abstraction implemented via DeviceProfile struct (data-driven, not trait objects).\n\nNew types: FpgaType enum (NonPro/Pro/Mso), DeviceProfile struct (10 fields: digital/analog channels, fpga_type, adc_bits, gpif_width, supports_encryption, max rates).\n\n4 static profiles: LOGIC_4_PROFILE, LOGIC_8_PROFILE, LOGIC_PRO_8_PROFILE, LOGIC_PRO_16_PROFILE.\n\nDerived methods: fpga_chunk_size(usb_speed), register_write_verifies(), validate_config().\n\nFPGA programming refactored to use DeviceProfile: chunk sizes (62 NonPro, 508 Pro USB2, 1020 Pro USB3), ACK behavior, register paths.\n\nis_lattice kept as separate parameter (per-revision, not per-model).\n\n282 tests pass, clippy clean.\n\nCommit: eeb26c4"
<!-- SECTION:FINAL_SUMMARY:END -->
