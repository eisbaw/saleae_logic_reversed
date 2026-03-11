---
id: TASK-0127
title: RE ParseFirmwareImage and firmware image format validation
status: To Do
assignee: []
created_date: '2026-03-11 11:21'
labels:
  - re
  - ghidra
  - firmware
dependencies: []
references:
  - re/notes/firmware-extraction.md
  - re/notes/fx3-firmware-protocol.md
  - crates/saleae-driver/src/firmware.rs
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Saleae::ParseFirmwareImage(gsl::span&lt;const uint8_t&gt;) parses firmware images before upload. We upload firmware blobs but don't fully validate them. Understanding this function reveals:
- The exact firmware header format beyond the "CY" magic
- How bitstream CRCs are validated (10 distinct CRC values found in strings)
- How the correct firmware variant is selected per device type
- Whether there are version checks or compatibility guards

Related: UsbDevice::GetFirmwareForDeviceType(::DeviceType) selects which firmware blob to use.

Investigation:
1. Decompile ParseFirmwareImage
2. Decompile GetFirmwareForDeviceType
3. Document header format, CRC validation, version fields
4. Document the firmware selection logic per DeviceType
5. Compare against our firmware.rs upload implementation
<!-- SECTION:DESCRIPTION:END -->
