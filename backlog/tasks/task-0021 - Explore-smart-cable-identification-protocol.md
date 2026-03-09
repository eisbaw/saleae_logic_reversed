---
id: TASK-0021
title: Explore smart cable identification protocol
status: Done
assignee: []
created_date: '2026-03-08 22:46'
updated_date: '2026-03-09 03:11'
labels: []
dependencies: []
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
String analysis found references to "SmartCable" in digital configuration.

Hypothesis: Saleae's newer products use "smart cables" that contain identification or signal conditioning components (possibly I2C EEPROM). The device may query the cable type and adjust behavior.\n\nInvestigate:\n1. What is a smart cable physically? (resistor ID? I2C EEPROM? active electronics?)\n2. How does the device identify the cable type?\n3. Does cable type affect channel configuration or voltage levels?\n4. Is this relevant only for specific device models?
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
## Smart Cable Investigation Results

Smart cables are an **MSO-only feature** -- completely irrelevant to Logic Pro 8/16, Logic 8, and Logic 4.

### Key findings:

1. **MSO-exclusive**: All smart cable code lives under `MsoDeviceInterface`, `MsoFpga`, and `MsoLogicAnalyzerInstrument` classes. No references in Logic/LogicPro device classes.

2. **Protocol**: Smart cables communicate via USB Power Delivery Unstructured Vendor Defined Messages (PD UVDM) over USB Type-C sideband channel. They expose a register interface (`SCRegister`) read/written through PD UVDM transfers.

3. **Identity**: Cables report vid, pid, bcd version, hwVersion, fwVersion, plug_count, and orientation. Board and assembly part numbers are also tracked (`MsoLaSmartCableBoardPartNumber`, `MsoLaSmartCableAssemblyPartNumber`).

4. **Physical**: Each cable provides 4 digital channels, has two IO expanders (A/B) and a DAC. The MSO supports multiple hot-pluggable cable ports.

5. **No action needed**: Logic Pro devices use `Fx3DeviceFeatures` and `ProFpgaDeviceFeatures` for I2C EEPROM access -- a completely different path. Smart cables require no further investigation for current project scope.

### Files created:
- `re/notes/smart-cable.md` -- detailed findings
<!-- SECTION:FINAL_SUMMARY:END -->
