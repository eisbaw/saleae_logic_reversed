---
id: TASK-0004
title: Export Ghidra decompiled C code for AI traversal
status: Done
assignee: []
created_date: '2026-03-08 22:42'
updated_date: '2026-03-08 23:29'
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

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Implemented ExportDecompiled.py Ghidra headless script that:

1. Iterates all functions, skipping those labeled by ApplyFunctionID.py / LabelByStrings.py as library functions (checks plate comments for [LIBUSB], [OPENSSL], etc. tags, plus name-prefix matching for known library APIs)
2. Decompiles each remaining function via Ghidra's DecompInterface with 60s timeout per function
3. Exports individual .c files to re/ghidra/decompiled/by-address/ with rich metadata headers (address, size, callers, callees)
4. Creates index.txt (master function list), xrefs.txt (call graph), classes.txt (detected C++ class/namespace groupings)
5. Creates by-class/ directory with symlinks grouping methods by detected class name
6. Logs decompilation failures to failures.txt
7. Reports progress with rate and ETA during the (potentially hours-long) export

Updated justfile ghidra-export recipe to run the script via analyzeHeadless. Added re/ghidra/decompiled/ to .gitignore.
<!-- SECTION:FINAL_SUMMARY:END -->
