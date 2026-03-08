---
id: TASK-0017
title: 'Scaffold Rust workspace (saleae-proto, saleae-usb, saleae-driver, saleae-cli)'
status: To Do
assignee: []
created_date: '2026-03-08 22:45'
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
