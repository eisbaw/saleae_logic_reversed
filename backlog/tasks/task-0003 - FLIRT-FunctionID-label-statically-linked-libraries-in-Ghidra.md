---
id: TASK-0003
title: 'FLIRT/FunctionID: label statically-linked libraries in Ghidra'
status: To Do
assignee: []
created_date: '2026-03-08 22:42'
labels: []
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
libgraph_server_shared.so statically links libusb-1.0, OpenSSL, Python, Tcl, SQLite, BerkeleyDB, lzma, zlib. These account for ~80% of the 48MB binary.

Apply FLIRT signatures or Ghidra FunctionID databases to identify and label all functions from these known libraries. This carves out the noise and reveals Saleae's own code.\n\nHypothesis: once known libraries are labeled, the remaining ~20% of functions are Saleae business logic organized around the device class hierarchy (UsbDevice, Logic2FpgaDevice, ProFpgaDeviceFeatures, etc.).\n\nDocument the address ranges of each identified library in re/notes/library-map.md.
<!-- SECTION:DESCRIPTION:END -->
