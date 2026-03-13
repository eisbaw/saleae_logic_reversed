---
id: TASK-0178
title: 'Fix capture-sequence.md: SetCaptureParameters uses 0x80 not 0x7F'
status: Done
assignee: []
created_date: '2026-03-13 05:35'
updated_date: '2026-03-13 07:01'
labels:
  - docs
  - protocol
  - bug
dependencies: []
references:
  - doc/protocol/capture-sequence.md
  - 'crates/saleae-driver/src/capture.rs:20'
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
doc/protocol/capture-sequence.md Section 3 claims SetCaptureParameters uses WriteConfigurationData (opcode 0x7F), but the actual implementation uses WriteRegisters (opcode 0x80). capture.rs:20-22 explicitly documents the correct path. The pcap also shows WriteRegisters commands in Phase 7. This is a critical protocol documentation error.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Fixed capture-sequence.md Section 3: SetCaptureParameters uses WriteRegisters (opcode 0x80) to write individual 8-bit FPGA configuration registers, not WriteConfigurationData (opcode 0x7F) which is only for FPGA bitstream upload. Also updated confidence table, clarified chunk size limits apply to bitstream only, and marked Open Question 2 as resolved. Commit 9705428.
<!-- SECTION:FINAL_SUMMARY:END -->
