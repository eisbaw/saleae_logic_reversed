---
id: TASK-0028
title: RE ReseedDevice mechanism -- what does the device seed do?
status: Done
assignee: []
created_date: '2026-03-09 00:08'
updated_date: '2026-03-09 06:12'
labels:
  - firmware
  - investigation
dependencies:
  - TASK-0007
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
String evidence from TASK-0007: LinuxUsbDevice::ReseedDevice(UsbEndpoint &), libusb_bulk_transfer failure (reseed), TestFirmwareResponsivnessAndReseed, "Can't reset the device seed and download the firmware in one command". The reseed uses a bulk endpoint (not control transfer). May relate to serial number assignment or device identity. Needs Ghidra trace of ReseedDevice.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Completed analysis of the ReseedDevice mechanism through static binary analysis of libgraph_server_shared.so v2.4.40.\n\n## Key Finding\n\nReseedDevice is NOT PID patching and NOT serial number assignment. It is a cryptographic seed exchange: the host generates 4 random bytes (via OpenSSL RAND_bytes), sends them to the FX3 via EP1 OUT bulk transfer, and stores them at this->offset_0x64. Both host and device use this shared seed for XOR-based obfuscation of subsequent EP1 command traffic via SetControlByte() in SimpleEncryption.cpp.\n\n## Corrections Made\n\n1. Opcode 0x22 entry REMOVED from command table -- the 0x22 byte was ASCII double-quote in error string construction, not a command opcode\n2. ReseedDevice sends raw 4-byte seed, no opcode prefix\n3. PID patching claim in logic4-8-firmware.md corrected -- reseed is unrelated to PID\n4. New opcode 0x7D (Ping) discovered and documented\n\n## Deliverables\n- re/notes/reseed-mechanism.md -- full analysis\n- doc/protocol/ep1-commands.md -- corrected ReseedDevice entry, added 0x7D Ping\n- re/notes/ep1-command-protocol.md -- updated working notes\n- re/notes/logic4-8-firmware.md -- corrected PID patching claim\n- TASK-0049 created for SimpleEncryption deep dive"
<!-- SECTION:FINAL_SUMMARY:END -->
