---
id: TASK-0039
title: RE ADC data encoding (bit justification and signedness)
status: Done
assignee: []
created_date: '2026-03-09 00:50'
updated_date: '2026-03-09 07:08'
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

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
## ADC Data Encoding: Resolved

### Bit Justification
12-bit ADC values are **right-justified** in the int16_t container: bits [11:0] contain the ADC data, bits [15:12] are sign extension.

### Signedness
**Two's complement** encoding for all devices except Logic 4:
- Logic 4 (HMCAD1100): unsigned 8-bit, range [0, 255]
- Logic 8 (HMCAD1100/ADS528x): signed 12-bit, range [-2048, 2047]
- Logic Pro 8 (AD9637/ADS528x): signed 12-bit, range [-2048, 2047]
- Logic Pro 16 (AD9637/ADS528x): signed 12-bit, range [-2048, 2047]

### ADC-to-Voltage Transform
Linear transform: `voltage = adc_code * m + b`

Default transforms (no calibration):
- Logic 4: [0..255] -> [0V..5V]
- Logic 8: [-2048..2047] -> [0V..5V]
- Logic Pro 8/16: [-2048..2047] -> [-10V..10V]

With calibration: per-channel `fullScaleVoltageRanges` from .cal file replace defaults.

### Key Evidence
- JS bundle `compute1DTransform` calls with explicit range constants
- JS `acquisitionInfo: {minAdcCode: -2048, maxAdcCode: 2047}` hardcoded
- Native `cal.mAdcToVoltageTransform.mScale` assertions
- `VectorData<int16_t>` template instantiation for ADC pipeline
- `"invalid ADC data, exceedes 8 bits."` runtime validation for HMCAD1100

### Files
- `re/notes/adc-encoding.md` -- full analysis
- Updated: `re/notes/analog-capture-pipeline.md`, `re/notes/adc-initialization.md`, `doc/protocol/analog-capture.md`
<!-- SECTION:FINAL_SUMMARY:END -->
