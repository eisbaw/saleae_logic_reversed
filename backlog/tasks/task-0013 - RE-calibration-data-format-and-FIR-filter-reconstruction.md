---
id: TASK-0013
title: RE calibration data format and FIR filter reconstruction
status: To Do
assignee: []
created_date: '2026-03-08 22:44'
labels: []
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Reverse engineer the calibration system -- this is critical for correct analog output.

Known from string/symbol analysis:
- afeConfigCalibrations -- AFE calibration data structure
- "ADC failed to finish foreground calibration"
- Per-channel calibration
- FIR filter coefficients derived from calibration

Key insight: analog sampling is imperfect. The analog front-end introduces frequency-dependent distortion. Saleae compensates this with FIR filters whose coefficients come from factory calibration data. Without these filters, analog output will be wrong.

Investigate:
1. Where is calibration data stored? (server-side download? device EEPROM? embedded in app?)
2. What is the calibration data format? (binary? JSON? protobuf?)
3. What parameters are calibrated per channel? (gain, offset, frequency response)
4. How are FIR filter coefficients derived from calibration data?
5. How many FIR taps? Is it per-sample-rate or one set of coefficients?
6. What is the AFE configuration that calibration modifies?
7. Is there a calibration file on disk after first run?

This is the highest-difficulty RE task. The FIR filter reconstruction determines whether our analog output matches Saleae's.
<!-- SECTION:DESCRIPTION:END -->
