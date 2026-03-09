---
id: TASK-0020
title: Explore fan control and hardware monitoring in firmware
status: Done
assignee: []
created_date: '2026-03-08 22:45'
updated_date: '2026-03-09 03:06'
labels: []
dependencies: []
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
String analysis found:
- "Enabling fan control in firmware."
- "Enabling hardware monitoring in firmware."

Hypothesis: Logic Pro devices (and possibly MSO) have active cooling fans and temperature sensors. The firmware controls fan speed and monitors hardware health (temperature, voltage rails).

This is relevant because:
1. Analog calibration may be temperature-dependent
2. The driver may need to enable/configure fan control after firmware upload
3. Hardware monitoring data may be available over a USB endpoint

Investigate: what USB commands enable fan control and hardware monitoring? What monitoring data is returned?
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Investigation complete. Fan control and hardware monitoring are exclusively MSO device features -- they do not apply to Logic Pro 8/16, Logic 8, or Logic 4.

Key findings:
- EnableFanControl and EnableHardwareMonitoring are called from MsoDeviceInterface during MSO initialization
- The MSO has 5 temperature sensors (AFE, ADC, FPGA, fan controller 0, fan controller 1)
- Two thermal threshold levels: hot and critical, reported via ThermalEvent (0x0007)
- MsoFanFault and MsoThermalFault surface as user-facing errors
- MSO has a TPS650864 PMIC with 6 buck regulators + Vtt
- LogicProfessionalDevice has zero fan/thermal/monitoring references
- No relevance to the Logic Pro RE effort; documented in re/notes/fan-hw-monitoring.md for completeness
<!-- SECTION:FINAL_SUMMARY:END -->
