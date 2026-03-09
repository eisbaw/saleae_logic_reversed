---
id: TASK-0039
title: RE ADC data encoding (bit justification and signedness)
status: To Do
assignee: []
created_date: '2026-03-09 00:50'
labels:
  - analog
  - adc
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Determine whether 12-bit ADC values are left-justified (bits [15:4]) or right-justified (bits [11:0]) in the int16_t container. Also determine if the encoding is offset binary or two's complement.

Approach: Ghidra decompilation of AnalogCreatorNode or the disaggregator analog output path. The _adc_to_voltage function in the Python layer may also reveal the transform.

For HMCAD1100 (8-bit): The validation "invalid ADC data, exceedes 8 bits" suggests it checks whether the int16_t value fits in 8 bits, meaning it is likely right-justified zero-extended.

Discovered during TASK-0012.
<!-- SECTION:DESCRIPTION:END -->
