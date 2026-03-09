---
id: TASK-0054
title: Set up remote build and deploy pipeline to nvdk-msdlab-09
status: To Do
assignee: []
created_date: '2026-03-09 11:21'
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
