---
id: TASK-0183
title: Add end-to-end capture pipeline integration test
status: Done
assignee: []
created_date: '2026-03-13 05:36'
updated_date: '2026-03-13 09:17'
labels:
  - testing
  - driver
  - capture
dependencies: []
references:
  - crates/saleae-driver/tests/integration.rs
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
No integration test exercises the full capture workflow: configure -> start -> read chunks -> disaggregate -> stop -> export. Individual steps are tested but never together. Also no test exercises full capture with EncryptionMode::XorAll (all tests use None). Add mock-based integration test covering the complete pipeline, including with encryption.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added 3 integration tests: full capture pipeline with disaggregation, encrypted capture with XorAll, and stop-flag termination.
<!-- SECTION:FINAL_SUMMARY:END -->
