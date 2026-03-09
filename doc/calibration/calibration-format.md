# Calibration Data Format

Status: **Confirmed** -- Extracted from bundle.js CalibrationManager class
(Saleae Logic 2 v2.4.40), corroborated by binary string analysis of
libgraph_server_shared.so (JSON field names, type names, assertion
messages). Two independent sources (JS frontend + native backend)
agree on data structure.

Awaiting: Actual `.cal` file download and byte-level verification of
the Boost.Serialization XML structure.


## Overview

Saleae Logic devices with analog channels (Logic 4, Logic 8, Logic Pro 8,
Logic Pro 16) download per-device calibration data from Saleae's servers.
The calibration compensates for analog front-end (AFE) distortion using
an IIR filter (5 biquad sections in Direct Form I, cascaded as
Second-Order Sections) and provides per-channel ADC-to-voltage transforms.


## Calibration File Source

### Server-Hosted Files

Calibration files are hosted at:
```
https://downloads.saleae.com/calibration/{deviceIdHex}.cal
```

Where `deviceIdHex` is the device serial number (numeric) converted to
hexadecimal.

The application also queries a GraphQL API to get the latest download URL:

```
POST https://api.saleae.com/graphql

query GetDeviceCalibration($deviceId: String, $deviceType: Int, $version: String!) {
  getDeviceCalibration(deviceId: $deviceId, deviceType: $deviceType, version: $version) {
    deviceId
    deviceType
    downloadUrl
    lastUpdated
  }
}
```

**Confidence: Confirmed** -- Extracted from bundle.js CalibrationManager


### Device Type Enum

```
Logic    = 0
Logic16  = 1
Logic4   = 4
Logic8   = 5
LogicPro8  = 6
LogicPro16 = 7
```

**Confidence: Confirmed** -- Extracted from bundle.js


### Devices Requiring Calibration

```
["Logic4", "Logic8", "LogicPro8", "LogicPro16"]
```

Logic (original) and Logic16 are excluded. MSO devices use a separate
EEPROM-based path.

**Confidence: Confirmed** -- `devicesRequiringCalibration` constant in bundle.js


## Calibration File Format

### File Type

Boost.Serialization XML. The JS frontend uses `xml2json` to parse it.
The root element is `boost_serialization` containing a
`DeviceCalibrationSet` element.

### XML Structure

```xml
<boost_serialization>
  <DeviceCalibrationSet>
    <px>
      <mDeviceId>
        <mDeviceId><_text>{deviceId}</_text></mDeviceId>
        <mDeviceType><_text>{deviceType}</_text></mDeviceType>
      </mDeviceId>
      <mCalibrationDate><_text>{date}</_text></mCalibrationDate>
      <mLastUpdatedDate><_text>{date}</_text></mLastUpdatedDate>
      <mMinimumSoftwareVersionRequired><_text>{version}</_text></mMinimumSoftwareVersionRequired>
      <mAdcGain><_text>{float}</_text></mAdcGain>
      <mFullScaleVoltageRanges>
        <item>...</item>
        <!-- one per channel -->
      </mFullScaleVoltageRanges>
      <mCalibrationData>
        <item>...</item>
        <!-- one per (channel, sampleRate) -->
      </mCalibrationData>
    </px>
  </DeviceCalibrationSet>
</boost_serialization>
```

**Confidence: Confirmed** -- `parseRawCalibrationFile` function in bundle.js


### Per-Channel Full-Scale Voltage Range

```xml
<item>
  <mChannel>
    <mChannelIndex><_text>{channelIndex}</_text></mChannelIndex>
  </mChannel>
  <mFullScaleMinVoltage><_text>{float}</_text></mFullScaleMinVoltage>
  <mFullScaleMaxVoltage><_text>{float}</_text></mFullScaleMaxVoltage>
</item>
```

These define the full ADC range in volts for each channel and are used
to compute the ADC-to-voltage linear transform.

**Confidence: Confirmed** -- `extractFullScaleVoltageRanges` in bundle.js


### Per-Channel, Per-Sample-Rate Calibration Set

```xml
<item>
  <mSampleRate><_text>{sampleRateHz}</_text></mSampleRate>
  <mChannel>
    <mChannelIndex><_text>{channelIndex}</_text></mChannelIndex>
  </mChannel>
  <mGroupDelay><_text>{float}</_text></mGroupDelay>
  <mImpulseLength><_text>{float}</_text></mImpulseLength>
  <mDf1SosCoefficients>
    <item><_text>{b0_section0}</_text></item>
    <item><_text>{b1_section0}</_text></item>
    <item><_text>{b2_section0}</_text></item>
    <item><_text>{a0_section0}</_text></item>   <!-- always 1.0 -->
    <item><_text>{a1_section0}</_text></item>
    <item><_text>{a2_section0}</_text></item>
    <!-- repeat for sections 1-4: 30 values total -->
  </mDf1SosCoefficients>
</item>
```

**Confidence: Confirmed** -- `extractCalibrationSets` in bundle.js

Each calibration set contains:
- `mSampleRate`: ADC sample rate in Hz (string of integer)
- `mChannelIndex`: 0-based channel index
- `mGroupDelay`: filter group delay in samples
- `mImpulseLength`: effective impulse response length
- `mDf1SosCoefficients`: 30 floating-point values (5 sections x 6 values)


## IIR Filter: DF1 SOS Biquad Cascade

### Architecture

The calibration filter is a cascade of **5 second-order sections (SOS)**,
each a biquad filter in Direct Form I (DF1):

```
        b0 + b1*z^-1 + b2*z^-2
H(z) = -------------------------
         1 + a1*z^-1 + a2*z^-2
```

Total filter: H(z) = H_0(z) * H_1(z) * H_2(z) * H_3(z) * H_4(z)

This is a 10th-order IIR filter (5 sections x 2nd order each).

### Coefficient Layout

The XML stores 6 values per section in order: `b0, b1, b2, a0, a1, a2`

The JS parser extracts 5 values per section (skipping a0 at index 3):

```javascript
for (let A = 0, e = 0; A < coefficients.length; A += 6, e++) {
  sections[e] = {
    b0: parseFloat(coefficients[A]._text),
    b1: parseFloat(coefficients[A+1]._text),
    b2: parseFloat(coefficients[A+2]._text),
    // coefficients[A+3] = a0 = 1.0 (skipped, normalized)
    a1: parseFloat(coefficients[A+4]._text),
    a2: parseFloat(coefficients[A+5]._text),
  };
}
```

**5 sections x 6 values = 30 values in the XML**

### Practical Dimensions

For a device with 8 analog channels and 6 sample rates:
- 8 * 6 = 48 calibration sets
- Each set: 5 biquad sections with 5 coefficients + groupDelay + impulseLength
- Total: 48 * 30 = 1440 coefficient values

**Confidence: Confirmed** -- Direct extraction from bundle.js parsing code


### SIMD Implementation

The binary contains SIMD-optimized IIR filter implementations:
- `SimdIirFilterPrototype` (RTTI type name)
- `SimdPrototype_Static2By3IirFilter` (RTTI type name)

These suggest SSE/AVX-optimized biquad processing, likely processing
multiple samples per cycle.

**Confidence: High** -- RTTI type names in binary


## ADC-to-Voltage Transform

### Transform Formula

```
voltage = adc_code * m + b
```

Where `m` (scale) and `b` (offset) are computed from the calibration
data's fullScaleVoltageRanges.

### Computation

```
m = (maxVoltage - minVoltage) / (adcMax - adcMin)
b = minVoltage - adcMin * m
```

### Default ADC Ranges

| Device | ADC Min | ADC Max | Bit Depth |
|--------|---------|---------|-----------|
| Logic4 | 0 | 255 | 8-bit unsigned |
| Logic8 | -2048 | 2047 | 12-bit signed |
| LogicPro8 | -2048 | 2047 | 12-bit signed |
| LogicPro16 | -2048 | 2047 | 12-bit signed |

### Default Voltage Ranges (without calibration)

| Device | Min Voltage | Max Voltage |
|--------|-------------|-------------|
| Logic4 | 0 V | 5 V |
| Logic8 | 0 V | 5 V |
| LogicPro8 | -10 V | 10 V |
| LogicPro16 | -10 V | 10 V |

**Confidence: Confirmed** -- `getDefaultTransformForDevice`,
`getDefaultAdcRange` in bundle.js


### Native-Side Transform Struct

```cpp
struct VoltageTransform {
    double mScale;   // multiplier (m)
    double mOffset;  // additive offset (b)
};
```

Assertions in binary:
- `cal.mAdcToVoltageTransform.mScale != 0.0`
- `isfinite(hw_channel.mAdcToVoltageTransform.mScale)`

**Confidence: Confirmed** -- Multiple assertion strings in binary


## HMCAD1100 ADC Gain Register

The calibration file contains `mAdcGain` (float), sent to the native
library as `hittiteAdcGainRegisterValue` via:

```json
{
  "type": "Saleae::Graph::LogicDevice::SetAdcFullScaleGain",
  "fullScaleGain": <value from mAdcGain>
}
```

This writes to the HMCAD1100's programmable gain register. The field
name "hittite" refers to Hittite Microwave (now Analog Devices), the
original manufacturer of the HMCAD ADC line.

**Confidence: Confirmed** -- bundle.js `sendCalibrationToGraph` and
binary string `/hittiteAdcGainRegisterValue`


## Local Caching

Parsed calibration data is cached as JSON in the application's storage
directory:

```
calibrations/{deviceId}-{lastUpdated}-1.cal
```

File content:
```json
{
  "version": "1",
  "data": { /* parsed CalibrationData object */ }
}
```

The cache key includes `lastUpdated` (timestamp from server), so new
calibrations from the server replace old cached versions.

**Confidence: Confirmed** -- `generateFileName`, `downloadOrLoadCalibrationData`
in bundle.js


## Calibration Application Sequence

```
1. Device connects
2. JS frontend checks if device type requires calibration
3. CalibrationManager.getCalibration():
   a. Check in-memory cache
   b. Query GraphQL API for latest metadata
   c. Check local file cache
   d. Download .cal file if needed
   e. Parse Boost XML -> JSON data structure
   f. Cache locally
4. SessionDeviceManager.sendCalibrationToGraph():
   a. For each analog channel:
      - Send SetCalibration action (full calibration object)
      - Send SetChannel action (channel index)
   b. If hittiteAdcGainRegisterValue set:
      - Send SetAdcFullScaleGain to LogicDeviceNode
5. Calibration state transitions:
   "Not set" -> "In progress" -> "Set" (or "Error")
```

**Confidence: Confirmed** -- Complete flow extracted from bundle.js
