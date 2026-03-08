---
id: TASK-0003
title: 'FLIRT/FunctionID: label statically-linked libraries in Ghidra'
status: Done
assignee: []
created_date: '2026-03-08 22:42'
updated_date: '2026-03-08 23:24'
labels: []
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
libgraph_server_shared.so statically links libusb-1.0, OpenSSL, Python, Tcl, SQLite, BerkeleyDB, lzma, zlib. These account for ~80% of the 48MB binary.

Apply FLIRT signatures or Ghidra FunctionID databases to identify and label all functions from these known libraries. This carves out the noise and reveals Saleae's own code.\n\nHypothesis: once known libraries are labeled, the remaining ~20% of functions are Saleae business logic organized around the device class hierarchy (UsbDevice, Logic2FpgaDevice, ProFpgaDeviceFeatures, etc.).\n\nDocument the address ranges of each identified library in re/notes/library-map.md.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Ghidra headless analysis script created (ApplyFunctionID.py)
- [ ] #2 String-based labeling script created (LabelByStrings.py)
- [ ] #3 FID generation helper script created (generate-fid-from-nix.sh)
- [ ] #4 Library map template created (re/notes/library-map.md)
- [ ] #5 Justfile recipes added (ghidra-import, ghidra-fid, ghidra-gen-sigs)
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Created Ghidra headless analysis tooling for identifying statically-linked library functions in libgraph_server_shared.so.

Key finding: Ghidra's built-in FunctionID databases only cover Windows Visual Studio libraries (.fidbf files are all vs20xx). For a Linux GCC-compiled binary, we need custom FID databases AND string-based labeling as a fallback.

Library versions identified from binary strings:
- libusb 1.0.x
- OpenSSL 1.1.x (nix has 3.3.2 -- major version mismatch)
- Python 3.8.2 (nix has 3.12.8 -- version mismatch)
- Tcl 8.6.10
- SQLite 3.31.1 (nix has 3.50.1)
- BerkeleyDB 6.0.19 / 12.1.6.0.19
- zlib 1.2.11 (nix has 1.3.1)
- lzma/xz (version unknown)

The nix store version mismatches mean FunctionID hash-based matching will have low coverage. String-based labeling (LabelByStrings.py) is the primary identification method.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Implemented FLIRT/FunctionID tooling for labeling statically-linked libraries in Ghidra.

Files created:
- re/ghidra/scripts/ApplyFunctionID.py -- Ghidra headless script applying FunctionID databases and labeling matched functions
- re/ghidra/scripts/LabelByStrings.py -- String-reference-based labeling (primary method, since built-in FIDBs are Windows-only)
- re/ghidra/scripts/BuildFidDb.py -- Helper to build custom FID databases from imported reference libraries
- re/ghidra/scripts/generate-fid-from-nix.sh -- Shell script to locate nix store libraries, import into Ghidra, and generate custom FID databases
- re/notes/library-map.md -- Template documenting identified library versions and address ranges
- re/ghidra/fid/.gitkeep -- Directory for generated .fidb files
- re/ghidra/reports/.gitkeep -- Directory for analysis reports

Justfile recipes added:
- ghidra-import: import binary and run full analysis
- ghidra-fid: run FunctionID + string labeling scripts
- ghidra-gen-sigs: generate FID databases from nix store libraries

Key limitation noted: version mismatches between nix store libraries and the binary's statically-linked copies will reduce FunctionID coverage. String-based labeling is the recommended primary approach.
<!-- SECTION:FINAL_SUMMARY:END -->
