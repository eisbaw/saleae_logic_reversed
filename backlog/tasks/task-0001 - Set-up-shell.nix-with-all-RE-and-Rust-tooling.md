---
id: TASK-0001
title: Set up shell.nix with all RE and Rust tooling
status: To Do
assignee: []
created_date: '2026-03-08 22:41'
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
