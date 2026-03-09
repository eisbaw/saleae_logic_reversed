---
id: TASK-0043
title: Investigate LogicStudentDevice inheritance anomaly
status: To Do
assignee: []
created_date: '2026-03-09 02:03'
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
