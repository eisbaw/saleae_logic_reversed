---
id: TASK-0130
title: RE AnalogCalibrationFilter node -- coefficient computation from .cal data
status: Done
assignee: []
created_date: '2026-03-11 11:21'
updated_date: '2026-03-12 02:26'
labels:
  - re
  - ghidra
  - analog
  - calibration
dependencies: []
references:
  - re/notes/calibration-system.md
  - re/notes/calibration-download.md
  - crates/saleae-proto/src/calibration.rs
  - crates/saleae-proto/src/biquad.rs
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The AnalogCalibrationFilter graph node receives calibration data via SetCalibration action and computes IIR/FIR filter coefficients. This is the bridge between the downloaded .cal file and the actual filter that processes analog samples.

We know:
- Calibration downloads as XML (Boost.Serialization) from downloads.saleae.com
- Filter is applied per-channel, per-sample-rate
- SimdIirFilterPrototype and SimdPrototype_Static2By3IirFilter are the filter implementations (TASK-0118)

What we DON'T know:
- How .cal XML data is transformed into filter coefficients
- Whether coefficients are pre-computed in the .cal file or computed at runtime
- The exact mapping: .cal fields -> biquad section coefficients

Investigation:
1. Decompile AnalogCalibrationFilter::SetCalibration handler
2. Trace the data flow from CalibrationData to filter coefficients
3. Document the coefficient computation algorithm
4. Compare against our calibration.rs and biquad.rs

This is separate from TASK-0118 (which RE's the filter execution). This task RE's how coefficients are derived from calibration data.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
RE complete. Key findings:

1. Coefficients are PRE-COMPUTED in the .cal file -- the binary does NOT compute biquad coefficients at runtime.

2. The SetCalibration handler (FUN_01bd3660) stores calibration data as-is. The SetChannel handler (FUN_01bd3700) stores the channel index.

3. On SamplingConfigurationData (FUN_01bd2aa0), the binary looks up the calibration set by sample rate (red-black tree search), copies 0xD8 bytes (27 doubles) of coefficients, then builds a SIMD filter.

4. The only runtime transformation is FUN_01bd8e50 (SplitCoefficients, 2260 bytes) which converts DF1 SOS biquad coefficients into companion matrix form for SIMD processing -- a mechanical linear algebra operation, not DSP design.

5. Two SIMD paths exist: SimdIirFilterPrototype (SSE4, 4 samples/iter) and Static2By3IirFilter (AVX/FMA, 8 samples/iter). Selection via CPUID.

6. Filter operates on i16 ADC samples: i16 -> f32 -> matrix mult -> f32 -> i16 (via packssdw saturation).

7. ADC-to-voltage transform is a SEPARATE graph node, not part of this filter.

Deliverables:
- Decompiled output: re/ghidra/decompiled-calibration-filter.txt
- Updated doc: doc/calibration/calibration-format.md (SIMD Implementation section)
- Added process_block_i16() to BiquadFilter with saturation semantics
- All 403 proto tests pass
<!-- SECTION:NOTES:END -->
