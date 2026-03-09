---
id: TASK-0007
title: RE DownloadFirmware -- FX3 firmware upload protocol
status: Done
assignee: []
created_date: '2026-03-08 22:43'
updated_date: '2026-03-09 00:08'
labels: []
dependencies:
  - TASK-0006
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Reverse engineer the DownloadFirmware function and its call chain.

Known facts:
- Firmware expects "CY" magic header (Cypress)
- Boot firmware must fit in 1 page (256 bytes) per string evidence
- CY_U3P_USB_* event strings indicate FX3 (USB 3.0) SuperSpeed controller
- Error string: "Can't reset the device seed and download the firmware in one command"\n\nInvestigate:\n1. What USB control transfer requests are used? (bRequest, wValue, wIndex)\n2. What is the firmware image format? (header, payload, CRC)\n3. Is firmware uploaded to RAM (volatile) or flash (non-volatile)?\n4. What is the chunking/sequencing protocol?\n5. How does the host verify successful upload?\n6. What state transitions does the device go through?\n\nSafety note: determine RAM vs flash target BEFORE any hardware testing.\nIf RAM-only, safe to test (power cycle recovers). If flash, proceed with extreme caution.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Firmware image format documented
- [x] #2 USB control transfer protocol documented
- [x] #3 RAM vs flash safety confirmed
- [x] #4 Embedded firmware images located and parsed
- [x] #5 Ghidra trace script created
<!-- AC:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Reverse engineered the FX3 firmware upload protocol by analyzing the embedded firmware images in libgraph_server_shared.so.\n\nKey findings:\n- 2 FX3 CY firmware images embedded (Logic Pro 8 at 0x0079e7a0, Logic Pro 16 at 0x007bbbb0)\n- Both are 119,824 bytes, differ by only 7 bytes (PID, power budget, timestamp, checksum)\n- Standard Cypress bootloader protocol: vendor control transfers (0x40/0xA0), 4KB chunks\n- ALL sections target volatile RAM (ITCM + SYSMEM) -- safe for hardware testing\n- 4 additional Intel HEX images found for FX2/FX2LP legacy devices\n- Device type table decoded at 0x004718e4 mapping VID:PID to device types\n- Created Ghidra script TraceFirmwareUpload.py for deeper analysis\n- Documented in re/notes/fx3-firmware-protocol.md and doc/protocol/fx3-firmware.md\n\nOpen questions: Logic 4/8 firmware mapping, reseed mechanism, FX2 firmware mapping
<!-- SECTION:FINAL_SUMMARY:END -->
