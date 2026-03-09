---
id: TASK-0043
title: Investigate LogicStudentDevice inheritance anomaly
status: Done
assignee: []
created_date: '2026-03-09 02:03'
updated_date: '2026-03-09 07:31'
labels:
  - re
  - class-hierarchy
dependencies:
  - TASK-0016
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
RTTI proves LogicStudentDevice inherits from LogicAnalyzerDevice, NOT from Logic2FpgaDevice as previously hypothesized. Yet LogicStudentDevice uses Logic2FpgaDevice patterns (FakeReadThread, SetCaptureParameters). Investigate whether this is via composition, mixin, or a codebase refactoring. This matters for our Rust device model -- we need to know if Logic 4 shares code with Logic 8 or is structurally different.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Resolved by TASK-0027: Logic 4 (PID 0x1003) is FX2LP-based, not FX3. LogicStudentDevice correctly inherits from LogicAnalyzerDevice (FX2 base class). The FakeReadThread and SetCaptureParameters patterns are shared via the FX2 code path, not Logic2FpgaDevice. See re/notes/logic4-8-firmware.md.
<!-- SECTION:FINAL_SUMMARY:END -->
