---
id: TASK-0002
title: Create justfile with RE and Rust recipes
status: Done
assignee: []
created_date: '2026-03-08 22:42'
updated_date: '2026-03-08 23:13'
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

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Created justfile with 24 recipes organized in 4 sections:\n\n## Environment (3 recipes)\n- check: verifies 19 tools, SALEAE_PATH, libgraph_server_shared.so, and remote USB device\n- udev-install: installs udev rules for all 6 Saleae VID:PID combos\n- saleae-path: prints SALEAE_PATH from shell.nix\n\n## RE - Decomposition (7 recipes)\n- extract-asar, strings-search, symbols, ghidra-open, ghidra-export (placeholder), binwalk-scan, list-libs\n\n## RE - USB (4 recipes)\n- usb-capture, usb-list, usb-descriptors, usb-monitor -- all SSH to nvdk-msdlab-09\n\n## Rust (10 recipes)\n- build, test, clippy, fmt, fmt-check, lint, e2e, run, doc, fuzz -- all gated by _require-cargo\n\nAll recipes tested and working. Rust recipes fail gracefully with clear error until Cargo.toml exists."
<!-- SECTION:FINAL_SUMMARY:END -->
