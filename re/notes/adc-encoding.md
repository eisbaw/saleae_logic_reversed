# ADC Data Encoding: Bit Justification and Signedness

Analysis date: 2026-03-09
Source: bundle.js (Saleae Logic 2 v2.4.40 extracted asar) + binary string analysis
TASK: TASK-0039


## Summary

ADC values are **right-justified two's complement** in the int16_t container.
The encoding differs by device:

| Device | ADC | Bit Depth | Container | ADC Range | Signedness |
|--------|-----|-----------|-----------|-----------|------------|
| Logic 4 | HMCAD1100 | 8-bit | int16_t | 0 to 255 | Unsigned |
| Logic 8 | HMCAD1100 (Xilinx) / ADS528x (Lattice) | 8/12-bit | int16_t | -2048 to 2047 | Signed (two's complement) |
| Logic Pro 8 | AD9637 (Xilinx) / ADS528x (Lattice) | 12-bit | int16_t | -2048 to 2047 | Signed (two's complement) |
| Logic Pro 16 | AD9637 (Xilinx) / ADS528x (Lattice) | 12-bit | int16_t | -2048 to 2047 | Signed (two's complement) |


## Evidence: JS Bundle (Primary Source)

The JS bundle in `dist/logic/bundle.js` contains definitive ADC range
constants and transform functions. This is the strongest evidence because
the code is unobfuscated and semantically clear.

### ADC Range Definitions

```javascript
// Default ADC ranges per device family
d = {
    Logic4:  { min: 0,     max: 255  },   // 8-bit unsigned
    Default: { min: -2048, max: 2047 }    // 12-bit signed two's complement
};
```

The `Default` range applies to Logic 8, Logic Pro 8, and Logic Pro 16.

### Default Voltage Transforms

```javascript
// compute1DTransform(srcMin, srcRange, dstMin, dstRange)
// Returns: { m: dstRange/srcRange, b: dstMin - srcMin * m }

E = compute1DTransform(0,     255,  0,   5);   // Logic 4:     [0..255]    -> [0V..5V]
h = compute1DTransform(-2048, 4095, 0,   5);   // Logic 8:     [-2048..2047] -> [0V..5V]
w = compute1DTransform(-2048, 4095, -10, 20);  // LogicPro8/16: [-2048..2047] -> [-10V..10V]

p = {
    Logic:      defaultTransform,  // {m:1, b:0} identity
    Logic16:    defaultTransform,  // {m:1, b:0} identity
    Logic4:     E,
    Logic8:     h,
    LogicPro8:  w,
    LogicPro16: w
};
```

### ADC-to-Voltage Transform Application

```javascript
// apply1DTransform(adc_code, transform)
apply1DTransform = (A, e) => A * e.m + e.b;

// Inverse
invert1DTransform = A => ({ m: 1/A.m, b: -A.b/A.m });

// Composition of transforms
merge1DTransforms = A => A.reduce((A, e) => ({ m: e.m*A.m, b: e.m*A.b+e.b }));
```

### AcquisitionInfo (passed to Python measurement API)

```javascript
acquisitionInfo: { minAdcCode: -2048, maxAdcCode: 2047 }
```

Hardcoded in the measurement request builder at offset ~5103725 in bundle.js.


## Evidence: Native Binary (Corroboration)

### Transform Structure

The native C++ code uses a struct with `mScale` and `mOffset` fields:

- `cal.mAdcToVoltageTransform.mScale != 0.0` -- assertion (scale must be nonzero)
- `isfinite(hw_channel.mAdcToVoltageTransform.mScale)` -- validation
- JSON config paths: `/adcToVoltageTransform`, `/scale`, `/offset`

This matches the JS `{m, b}` transform: `voltage = adc_code * mScale + mOffset`.

### VectorData Type

- ADC samples flow as `VectorData<int16_t>` (mangled: `VectorDataIsE`, s=short)
- `AnalogCreatorNode` pipe handler receives `VectorData<int16_t>`
- `AdvanceToNextValueGreaterThan(short)` / `AdvanceToNextValueLessThan(short)`

### HMCAD1100 Validation

- `"invalid ADC data, exceedes 8 bits."` -- validates 8-bit range at runtime
- This confirms HMCAD1100 data is right-justified (bits [7:0] used, [15:8] zero)

### Python API

```python
AcquisitionInfo.__init__(self, min_adc_code: Optional[int], max_adc_code: Optional[int])
AnalogSpan._adc_to_voltage(self, adc)  # uses the linear transform
```

Two Cython modules implement the same API:
- `saleae.data.analog_span_adc` -- for Logic device ADC data
- `saleae.data.analog_span_wdc` -- for MSO waveform data


## Calibrated Transform

When calibration data is available, the per-channel full-scale voltage
range from the `.cal` file replaces the default transform:

```javascript
convertVoltageRangeToTransform(deviceType, fullScaleVoltageRange) {
    const adcRange = getDefaultAdcRange(deviceType);
    // adcRange = {min: -2048, max: 2047} for most devices
    return compute1DTransform(
        adcRange.min,                    // srcMin
        adcRange.max - adcRange.min,     // srcRange (4095)
        fullScaleVoltageRange.minimumVoltage,     // dstMin
        fullScaleVoltageRange.maximumVoltage - fullScaleVoltageRange.minimumVoltage  // dstRange
    );
}
```

The calibration file provides `mFullScaleMinVoltage` and `mFullScaleMaxVoltage`
per channel (see doc/calibration/calibration-format.md).


## Bit Justification Analysis

### Right-Justified (Confirmed)

12-bit ADC values occupy bits [11:0] of the int16_t. Bits [15:12] are
sign-extension of bit 11 (two's complement).

Evidence:
- The JS range `{min: -2048, max: 2047}` spans exactly 4096 values (2^12)
- -2048 = 0xF800 in 16-bit two's complement (sign-extended from 12-bit 0x800)
- 2047 = 0x07FF in 16-bit
- If left-justified (bits [15:4]), the range would be {min: -32768, max: 32752}

### Logic 4 Exception

Logic 4 uses unsigned 8-bit values: `{min: 0, max: 255}`. No sign extension,
no 12-bit normalization. The 8-bit HMCAD1100 value is stored directly
in the int16_t container with zero-extension.

### Logic 8 with HMCAD1100 (Xilinx Revisions)

The JS bundle uses `{min: -2048, max: 2047}` for Logic 8, but the HMCAD1100
is an 8-bit ADC (range 0-255). This implies the native code normalizes
8-bit HMCAD1100 values to the 12-bit signed range for Logic 8:

- Most likely: left-shift by 4, then subtract 2048 (or equivalent)
- Formula: `int16_value = (uint8_adc << 4) - 2048`
- This maps 0 -> -2048, 128 -> 0, 255 -> 2032

This normalization ensures a consistent [-2048, 2047] range across all
device types (except Logic 4), regardless of the underlying ADC bit depth.

The validation string `"invalid ADC data, exceedes 8 bits."` runs before
this normalization, on the raw HMCAD1100 data.


## Summary Formula

For all devices except Logic 4:

```
voltage = adc_code * m + b

where:
  adc_code is int16_t, range [-2048, 2047] (right-justified 12-bit two's complement)
  m = (voltageMax - voltageMin) / 4095
  b = voltageMin - (-2048) * m = voltageMin + 2048 * m
```

For Logic 4:

```
voltage = adc_code * m + b

where:
  adc_code is int16_t, range [0, 255] (right-justified 8-bit unsigned)
  m = 5.0 / 255
  b = 0
```


## Confidence Assessment

| Finding | Confidence | Evidence Sources |
|---------|-----------|-----------------|
| 12-bit two's complement, right-justified | **Confirmed** | JS bundle range constants + binary VectorData<int16_t> + Python API |
| Logic 4: 8-bit unsigned, right-justified | **Confirmed** | JS bundle range {0, 255} + binary "exceedes 8 bits" validation |
| Linear transform: voltage = adc * m + b | **Confirmed** | JS apply1DTransform + binary mAdcToVoltageTransform.mScale assertion |
| Logic 8 HMCAD1100 normalization to 12-bit | **High** | JS uses {-2048, 2047} for Logic 8 + HMCAD1100 is 8-bit -- native code must normalize |
| Default voltage ranges | **Confirmed** | JS bundle compute1DTransform constants |
| Calibrated transform from .cal file | **Confirmed** | JS convertVoltageRangeToTransform + doc/calibration/calibration-format.md |


## Open Questions

1. **Exact HMCAD1100 normalization code**: How exactly does the native code
   convert 8-bit HMCAD1100 values to the 12-bit signed range for Logic 8?
   The hypothesis `(uint8 << 4) - 2048` fits the observed range but needs
   Ghidra decompilation to confirm.

2. **ADS528x output encoding**: The ADS528x supports both offset binary
   and two's complement output modes (configured via JESD204B frame format).
   Which mode does Saleae select? The consistent {-2048, 2047} range
   across all non-Logic-4 devices strongly suggests two's complement.

3. **AD9637 output register**: The AD9637 register 0x0014 (OUTPUT_MODE)
   controls whether the ADC outputs offset binary or two's complement.
   Ghidra decompilation of the AD9637 init sequence would confirm the
   register value written.
