---
id: TASK-0004
title: Export Ghidra decompiled C code for AI traversal
status: To Do
assignee: []
created_date: '2026-03-08 22:42'
labels: []
dependencies:
  - TASK-0003
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Export decompiled C code from Ghidra for all Saleae-owned functions (after FLIRT labeling removes library functions). Output to re/ghidra/decompiled/.

This enables Claude Code to search, traverse, and analyze the decompiled source alongside the human analyst. Write a Ghidra headless script (Python or Java) that:
1. Iterates all non-library functions
2. Exports decompiled C for each
3. Preserves function names, addresses, and cross-references
4. Organizes output by address range or class grouping

Add a just ghidra-export recipe to run this.
<!-- SECTION:DESCRIPTION:END -->
