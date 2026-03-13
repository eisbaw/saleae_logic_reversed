---
id: TASK-0149
title: Preserve DriverError chain instead of stringifying in CLI capture call sites
status: Done
assignee: []
created_date: '2026-03-12 16:40'
updated_date: '2026-03-12 16:57'
labels:
  - cli
  - error-handling
dependencies: []
references:
  - crates/saleae-cli/src/commands.rs
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Three CLI call sites convert DriverError to anyhow via `.map_err(|e| anyhow::anyhow!("capture failed: {e}"))`, which discards the structured error. Should implement `From<DriverError> for anyhow::Error` or use `.map_err(anyhow::Error::from)` to preserve the error chain for better diagnostics.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Replaced 7 stringifying .map_err() calls with .context()/.with_context() to preserve the full DriverError chain through anyhow.
<!-- SECTION:FINAL_SUMMARY:END -->
