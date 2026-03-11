---
id: TASK-0089
title: RE DeviceTypeToString and GetVidPidForDevice -- complete device enum
status: Done
assignee: []
created_date: '2026-03-10 14:15'
updated_date: '2026-03-10 14:52'
labels:
  - re
  - ghidra
  - device-class
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Two functions that together form the Rosetta Stone for device type mapping:

```
std::string_view Saleae::DeviceTypeToString(Saleae::DeviceType)
void DevicesManager::GetVidPidForDevice(DeviceType, U16 &, U16 &)
```

Source: `/workspace/monorepo/graph-io/core/saleae_base/src/device_type.cpp`

Decompiling these gives us:
- The complete DeviceType enum values and their names
- The VID:PID mapping for every device type
- Which device types are "Unknown" or "Mso" (assertion: "DeviceType::Mso is not valid", "DeviceType::Unknown is not valid")

This resolves the Graduate vs Student vs Logic4 vs Logic8 ambiguity definitively.

Also decompile the factory function in DevicesManager that dispatches on DeviceType to create the right device subclass (LogicDevice, LogicProfessionalDevice, LogicStudentDevice, LogicGraduateDevice, etc.).
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Decompiled DeviceTypeToString and GetVidPidForDevice. Found two DeviceType representations (external sparse: 0,1,4,5,6,7,12 and internal compact: 0-7). Confirmed all 6 known VID:PIDs. Graduate=Logic8 class name, Student=Logic4 class name. MSO has no USB VID:PID. Created re/notes/device-type-enum.md, doc/devices/device-types.md, and DecompileDeviceType.java script.
<!-- SECTION:FINAL_SUMMARY:END -->
