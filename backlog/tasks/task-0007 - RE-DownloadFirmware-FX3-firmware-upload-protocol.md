---
id: TASK-0007
title: RE DownloadFirmware -- FX3 firmware upload protocol
status: To Do
assignee: []
created_date: '2026-03-08 22:43'
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
