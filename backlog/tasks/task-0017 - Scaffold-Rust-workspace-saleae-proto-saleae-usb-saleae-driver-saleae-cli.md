---
id: TASK-0017
title: 'Scaffold Rust workspace (saleae-proto, saleae-usb, saleae-driver, saleae-cli)'
status: Done
assignee: []
created_date: '2026-03-08 22:45'
updated_date: '2026-03-09 02:20'
labels: []
dependencies:
  - TASK-0001
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Create the Cargo workspace with four crates:

- saleae-proto: wire format types, packet parsing. No USB dependency. Testable with byte slices from captured USB traffic. Include cargo-fuzz targets.
- saleae-usb: USB transport layer wrapping nusb. Must be mockable via trait for testing without hardware.
- saleae-driver: high-level facade tying proto + usb together. Device discovery, firmware upload, capture API.
- saleae-cli: thin CLI binary. Commands: list-devices, upload-firmware, capture-digital, capture-analog.

The USB transport trait is critical -- it enables testing protocol parsing against recorded USB captures without physical hardware.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Scaffolded Rust workspace with four crates: saleae-proto, saleae-usb, saleae-driver, saleae-cli.\n\nsaleae-proto:\n- Device ID constants for all 6 device types (VID/PID, type IDs from RE)\n- DeviceType enum with from_usb_id(), is_fx3(), is_pro(), name()\n- DeviceState enum matching Saleae's DSInitializing/DSReady/DSError\n- USB endpoint constants (EP0, EP1 OUT, EP1 IN, EP2 IN)\n- FX3 firmware image parser (CY magic, sections, checksum validation)\n- Upload chunk iterator (4KB max per USB control transfer)\n- cargo-fuzz target for FX3 parser\n- 17 unit tests\n\nsaleae-usb:\n- UsbTransport trait (control_transfer_in/out, bulk_transfer_in/out, reset)\n- NusbTransport real implementation (feature-gated behind nusb-backend)\n- MockTransport with operation recording and response queuing\n- Device discovery by VID/PID via nusb\n- 8 unit tests + 1 doc-test\n\nsaleae-driver:\n- SaleaeDriver facade with state machine enforcement\n- FX3 firmware upload implementation tested against MockTransport\n- CaptureConfig stub struct\n- 8 unit tests\n\nsaleae-cli:\n- clap CLI with list-devices, upload-firmware, capture-digital, capture-analog\n- Tracing/logging with -v verbosity\n- 4 unit tests\n\nAll 37 tests pass, clippy clean (zero warnings), rustfmt clean.\nCommit: 3cfa4ce
<!-- SECTION:FINAL_SUMMARY:END -->
