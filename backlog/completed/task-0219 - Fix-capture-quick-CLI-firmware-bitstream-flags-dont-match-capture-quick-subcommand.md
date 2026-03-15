---
id: TASK-0219
title: >-
  Fix capture-quick CLI: --firmware/--bitstream flags don't match capture-quick
  subcommand
status: Done
assignee: []
created_date: '2026-03-13 21:30'
updated_date: '2026-03-13 21:31'
labels:
  - bug
  - cli
dependencies: []
references:
  - crates/saleae-cli/src/main.rs
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Running `capture-quick --firmware X --bitstream Y` fails with argument parse error:
"tip: a similar argument exists: '--firmware'"

The capture-quick subcommand uses different flag names than capture-digital/capture-analog. Need to check main.rs for the actual flag names and update the justfile recipe or fix the CLI to be consistent.

Reproduce: `just hw-capture-quick`
Expected: quick capture runs
Actual: clap argument error
<!-- SECTION:DESCRIPTION:END -->
