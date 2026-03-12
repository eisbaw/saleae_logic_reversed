---
id: TASK-0127
title: RE ParseFirmwareImage and firmware image format validation
status: Done
assignee: []
created_date: '2026-03-11 11:21'
updated_date: '2026-03-12 03:45'
labels:
  - re
  - ghidra
  - firmware
dependencies: []
references:
  - re/notes/firmware-extraction.md
  - re/notes/fx3-firmware-protocol.md
  - crates/saleae-driver/src/firmware.rs
  - re/ghidra/decompiled-firmware-parsing.txt
  - re/captures/fx3-firmware-pro8.bin
  - re/captures/fx3-firmware-pro16.bin
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

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
## RE Findings

### ParseFirmwareImage (ELF VA 0x2db9130)
- Validates CY magic (bytes 0-1) and image_type == 0xB0 (byte 3)
- Does NOT check image_ctl (byte 2) at all
- Does NOT verify checksum (stores it but never validates)
- No version checking, no max section count limit
- Sections store raw pointers into the original firmware blob (zero-copy parse)
- Output: vector<Section> + entry_address (u32) + checksum (u32)

### GetFirmwareForDeviceType (ELF VA 0x2d9ad60)
- Jump table with 8 entries (DeviceType enum 0-7)
- Types 0,1,4,5: IHex format (FX2/8051 firmware) for Logic Original, Logic 16, Logic 4, Logic 8
- Types 2,3: "firmware not available" (MSO devices)
- Types 6,7: CY/FX3 format for Logic Pro 8 and Logic Pro 16
- Both Pro firmwares: 119,824 bytes, 4 sections, entry 0x4000e020, image_ctl=0x1C

### Firmware Image Format (confirmed)
- Header: 'C' 'Y' image_ctl image_type (4 bytes)
- image_ctl=0x1C: checksum disabled, SPI 4-bit, fast boot, no auto-execute
- Sections: dLength(u32le) + dAddress(u32le) + data[dLength*4]
- Terminator: dLength=0, entry_address(u32le), checksum(u32le)
- Pro 8 vs Pro 16: only 7 bytes differ (4 code + 3 checksum)

### Code Changes
- Updated Fx3Header docs with image_ctl bit layout table from AN76405
- Added spi_width() and fast_boot() methods to Fx3Header
- Updated firmware/mod.rs with complete DeviceType-to-firmware format mapping
- Added 4 new unit tests: saleae-style header, 4-section layout, real Pro 8/16 parsing
- Extracted Pro 8 and Pro 16 firmware binaries for testing
- Created detailed RE report at re/ghidra/decompiled-firmware-parsing.txt

### Key Finding: Our parser is MORE strict than Saleae's
Our parser validates checksums (when enabled), enforces max section count, and validates section sizes. Saleae's parser does none of this. Our parser correctly handles all real Saleae firmware.
<!-- SECTION:FINAL_SUMMARY:END -->
