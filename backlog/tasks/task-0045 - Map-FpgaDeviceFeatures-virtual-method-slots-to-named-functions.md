---
id: TASK-0045
title: Map FpgaDeviceFeatures virtual method slots to named functions
status: Done
assignee: []
created_date: '2026-03-09 02:03'
updated_date: '2026-03-09 07:49'
labels:
  - re
  - class-hierarchy
  - fpga
dependencies:
  - TASK-0016
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
FpgaDeviceFeatures has 8 virtual methods, of which ProFpgaDeviceFeatures overrides slots [1] and [3]. Known methods: ReadRegisters, WriteRegisters, WriteConfigurationData, WriteRegisterBit. Decompile the 8 methods in Ghidra to determine which slot maps to which function, and understand what the Pro overrides change (likely register addressing width or bus protocol).
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Mapped all 8 FpgaDeviceFeatures vtable entries to named functions via RTTI relocation analysis and disassembly of libgraph_server_shared.so v2.4.40.\n\nVtable layout (8 entries = 2 destructors + 6 virtual methods):\n- [0] ~FpgaDeviceFeatures() D1 complete destructor\n- [1] ~FpgaDeviceFeatures() D0 deleting destructor (Pro overrides)\n- [2] WriteRegister(U8 addr, U8 val) - wrapper calling slot[3]\n- [3] WriteRegisters(vector<pair<U8,U8>>&, const S8*) (Pro overrides: adds response verification)\n- [4] ReadRegister(U8 addr, const S8*) -> U8\n- [5] ReadRegisters(vector<U8>&, const S8*) -> vector<U8>\n- [6] DownloadBitstream(const U8*, U64, callback, double)\n- [7] ResetFpga()\n\nKey findings:\n- Discovered previously undocumented EP1 opcode 0x80 (WriteRegisters batch)\n- Pro override adds response verification after register writes, does NOT change addressing width\n- WriteRegisterBit and WriteConfigurationData are non-virtual member functions\n- FpgaDeviceFeatures uses 8-bit register addressing (distinct from MsoFpga 16-bit path)\n\nDeliverables:\n- re/notes/fpga-features-vtable.md - full vtable documentation\n- re/ghidra/scripts/MapFpgaVtable.py - Ghidra annotation script\n- Updated doc/protocol/ep1-commands.md with 0x80 opcode and corrected 0x81 format\n- Resolved open question #2 in re/notes/device-class-hierarchy.md
<!-- SECTION:FINAL_SUMMARY:END -->
