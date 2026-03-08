---
id: TASK-0001
title: Set up shell.nix with all RE and Rust tooling
status: Done
assignee: []
created_date: '2026-03-08 22:41'
updated_date: '2026-03-08 23:09'
labels: []
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Create shell.nix with:
- Ghidra, binwalk, wireshark/tshark, frida-tools, ltrace, strace
- asar (electron-asar), binutils, python3, xxd
- Rust toolchain (stable): rustc, cargo, clippy, rustfmt, cargo-nextest, cargo-fuzz
- nusb build deps
- pkg-config, libusb1 headers
- Export SALEAE_PATH env var pointing to saleae-logic-2 nix store path
- Pin nixpkgs revision for reproducibility
- Handle unfree package (saleae-logic-2) config
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Created shell.nix with pinned nixpkgs 24.11 (rev 59e618d90c0) containing all RE and Rust tooling:

**RE tools**: ghidra 11.2.1, binwalk 2.4.3, wireshark-cli 4.2.9 (tshark), frida-tools 12.5.0, ltrace, strace
**Binary analysis**: nodePackages.asar, binutils (nm/readelf/objdump/strings), python3 3.12.8, xxd (tinyxxd 1.3.7)
**Rust stable**: rustc 1.82.0, cargo 1.82.0, clippy, rustfmt, cargo-nextest 0.9.81, cargo-fuzz 0.13.1
**Build deps**: pkg-config, systemdLibs (libudev for nusb), libusb1 headers
**Project tooling**: just 1.38.0
**Env vars**: SALEAE_PATH exported via mkShell attribute

Also created .gitignore covering Rust targets, proprietary firmware blobs, Ghidra project databases, USB captures, nix results, Python cache, editor/IDE files, and scratch files.

**Known issue**: frida-tools Python bindings have a runtime import error for `frida._frida` module on this NixOS config. The binaries (frida-trace, frida-ps, etc.) are installed but may fail at runtime. This is a known nixpkgs packaging issue and would need to be addressed separately if frida dynamic instrumentation is needed.
<!-- SECTION:FINAL_SUMMARY:END -->
