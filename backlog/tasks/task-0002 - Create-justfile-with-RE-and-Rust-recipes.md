---
id: TASK-0002
title: Create justfile with RE and Rust recipes
status: To Do
assignee: []
created_date: '2026-03-08 22:42'
labels: []
dependencies:
  - TASK-0001
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Organize recipes by section:

## Environment
- check: verify tools, usbmon, USB permissions
- udev-install: install udev rules (sudo)
- saleae-path: print nix store path

## RE - Decomposition
- extract-asar: extract app.asar to re/extracted-asar/
- strings-search PATTERN: search across all Saleae binaries
- symbols LIB: dump symbols (nm, readelf)
- ghidra-open: launch Ghidra with project
- ghidra-export: export decompiled C code for AI analysis
- binwalk-scan: entropy/signature scan on libgraph_server_shared.so
- list-libs: list all .so files with sizes and types

## RE - USB
- usb-capture DURATION: tshark USB capture filtered to Saleae VIDs
- usb-list: lsusb with Saleae highlight
- usb-descriptors: full USB descriptor dump
- usb-monitor: live tshark with display filter

## Rust
- build, test, clippy, fmt, fmt-fix, lint, e2e, run, doc, fuzz
<!-- SECTION:DESCRIPTION:END -->
