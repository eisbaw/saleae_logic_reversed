---
id: TASK-0021
title: Explore smart cable identification protocol
status: To Do
assignee: []
created_date: '2026-03-08 22:46'
labels: []
dependencies: []
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
String analysis found references to "SmartCable" in digital configuration.

Hypothesis: Saleae's newer products use "smart cables" that contain identification or signal conditioning components (possibly I2C EEPROM). The device may query the cable type and adjust behavior.\n\nInvestigate:\n1. What is a smart cable physically? (resistor ID? I2C EEPROM? active electronics?)\n2. How does the device identify the cable type?\n3. Does cable type affect channel configuration or voltage levels?\n4. Is this relevant only for specific device models?
<!-- SECTION:DESCRIPTION:END -->
