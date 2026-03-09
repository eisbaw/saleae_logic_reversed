---
id: TASK-0054
title: Set up remote build and deploy pipeline to nvdk-msdlab-09
status: Done
assignee: []
created_date: '2026-03-09 11:21'
updated_date: '2026-03-09 12:06'
labels:
  - infrastructure
  - hardware-testing
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The Saleae Logic Pro 8 is on nvdk-msdlab-09 but our Rust code runs locally. We need to build the saleae-cli binary and deploy it to the remote host for hardware testing.

Options:
1. Cross-compile locally and scp the binary (simplest if target arch matches)
2. Build on the remote host via SSH (needs Rust toolchain there)
3. Use cargo-cross for cross-compilation

Steps:
1. Determine nvdk-msdlab-09 architecture: `ssh nvdk-msdlab-09 'uname -m'`
2. Check if the local build targets the same arch (likely both x86_64)
3. If same arch: add `just deploy` recipe that builds release and scps to remote
4. If different: set up cross-compilation
5. Add `just run-remote` recipe that SSHes and runs the binary on the remote host
6. Verify USB permissions on the remote host (udev rules for 21a9:*)
7. Test with `just run-remote -- list-devices`

The binary must be statically linked or the remote host must have matching glibc. Consider `--target x86_64-unknown-linux-musl` for static linking.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Implemented remote deploy pipeline to nvdk-msdlab-09 where the Saleae Logic Pro 8 (21a9:1005) is attached.

## Key findings

- Both local and remote are x86_64, so no cross-compilation needed
- Nix-built binaries have hardcoded Nix store paths for ld-linux-x86-64.so.2 and libc.so.6, so they cannot run on non-Nix hosts directly
- Solution: patchelf rewrites interpreter to /lib64/ld-linux-x86-64.so.2 and rpath to /lib64
- Binary requires up to GLIBC_2.34; remote CentOS Stream 9 has exactly glibc 2.34 (tight match)
- NFS home directory on lab hosts is read-only; deploy target is /var/tmp/saleae/ which persists across reboots
- Udev rules already installed on remote at /etc/udev/rules.d/99-saleae-logic.rules with MODE=0777
- nusb does not link libudev at compile time (no dynamic dependency beyond libc + libgcc_s)

## Changes

- **justfile**: Added `deploy` recipe (build release, patchelf, scp) and `run-remote` recipe (deploy + SSH exec with arg forwarding)
- **shell.nix**: Added patchelf to buildInputs

## Verified

`just run-remote list-devices` successfully detects the Logic Pro 8 on bus 002 addr 003.
<!-- SECTION:FINAL_SUMMARY:END -->
