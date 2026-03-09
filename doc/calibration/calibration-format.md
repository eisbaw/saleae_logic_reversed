# Calibration Data Format

Status: **Confirmed** -- Extracted from bundle.js CalibrationManager class
(Saleae Logic 2 v2.4.40), corroborated by binary string analysis of
libgraph_server_shared.so, and **validated against a real .cal file**
downloaded from Saleae's servers (Logic Pro 8, device ID 764051B51BD4D9F1).


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

Where `deviceIdHex` is the 64-bit device ID converted to lowercase
hexadecimal via `BigInt(deviceId).toString(16)`. The device ID is a
factory-assigned 64-bit integer stored on the device (NOT the USB
serial number).

Example: device ID `8520900333282843121` -> hex `764051b51bd4d9f1`
-> URL `https://downloads.saleae.com/calibration/764051b51bd4d9f1.cal`

The application also queries a GraphQL API to get the latest download URL:

```
POST https://api.saleae.com/graphql
Content-Type: application/json

{
  "query": "query GetDeviceCalibration ($deviceId: String, $deviceType: Int, $version: String!) {\n      getDeviceCalibration(deviceId: $deviceId, deviceType: $deviceType, version: $version) {\n        deviceId\n        deviceType\n        downloadUrl\n        lastUpdated\n      }\n    }",
  "variables": {
    "deviceId": "8520900333282843121",
    "deviceType": 6,
    "version": "2.4.13"
  }
}
```

Response:
```json
{
  "data": {
    "getDeviceCalibration": {
      "deviceId": "8520900333282843121",
      "deviceType": 6,
      "downloadUrl": "downloads.saleae.com/calibration/764051b51bd4d9f1.cal",
      "lastUpdated": 1568317711765
    }
  }
}
```

Note: The `downloadUrl` is returned without the `https://` prefix. The
`lastUpdated` value is a Unix timestamp in milliseconds. The `version`
parameter is required and checked against the calibration file's
`mMinimumSoftwareVersionRequired` field.

**Confidence: Confirmed** -- Tested live against api.saleae.com


### Device Type Enum

```
Logic    = 0
Logic16  = 1
Logic4   = 4
Logic8   = 5
LogicPro8  = 6
LogicPro16 = 7
```

**Confidence: Confirmed** -- Extracted from bundle.js, validated with real data


### Devices Requiring Calibration

```
["Logic4", "Logic8", "LogicPro8", "LogicPro16"]
```

Logic (original) and Logic16 are excluded. MSO devices use a separate
EEPROM-based path.

**Confidence: Confirmed** -- `devicesRequiringCalibration` constant in bundle.js


## Calibration File Format

### File Type

Boost.Serialization XML (version 10). The JS frontend uses `xml2json`
to parse it. The root element is `boost_serialization` containing a
`DeviceCalibrationSet` element.

### XML Structure (from real .cal file)

```xml
<?xml version="1.0" encoding="UTF-8" standalone="yes" ?>
<!DOCTYPE boost_serialization>
<boost_serialization signature="serialization::archive" version="10">
<DeviceCalibrationSet class_id="0" tracking_level="0" version="1">
  <px class_id="1" tracking_level="1" version="21" object_id="_0">
    <mDeviceId class_id="2" tracking_level="0" version="21">
      <mDeviceId>8520900333282843121</mDeviceId>
      <mDeviceType>6</mDeviceType>
    </mDeviceId>
    <mCalibrationDate>9/13/2019 3:48:10 AM</mCalibrationDate>
    <mLastUpdatedDate>9/13/2019 3:48:29 AM</mLastUpdatedDate>
    <mMinimumSoftwareVersionRequired>1.1.34</mMinimumSoftwareVersionRequired>
    <mNote></mNote>
    <mCalibrationType>1</mCalibrationType>
    <mCalibrationData class_id="3" tracking_level="0" version="0">
      <count>48</count>
      <item_version>21</item_version>
      <item class_id="4" tracking_level="0" version="21">
        ...
      </item>
      <!-- 48 items total (8 channels x 6 sample rates) -->
    </mCalibrationData>
    <mFullScaleVoltageRanges class_id="7" tracking_level="0" version="0">
      <count>8</count>
      <item_version>21</item_version>
      <item class_id="8" tracking_level="0" version="21">
        ...
      </item>
      <!-- 8 items (one per channel) -->
    </mFullScaleVoltageRanges>
    <mAdcGain>0</mAdcGain>
  </px>
</DeviceCalibrationSet>
</boost_serialization>
```

Note: The Boost.Serialization attributes (`class_id`, `tracking_level`,
`version`, `object_id`) are present on first-occurrence elements. The
`count` and `item_version` elements precede vector contents.

**Confidence: Confirmed** -- Validated against real .cal file


### Per-Channel Full-Scale Voltage Range

```xml
<item class_id="8" tracking_level="0" version="21">
  <mChannel class_id="5" tracking_level="0" version="21">
    <mDeviceId>8520900333282843121</mDeviceId>
    <mChannelIndex>0</mChannelIndex>
    <mDataType>0</mDataType>
  </mChannel>
  <mFullScaleMinVoltage>-10.444521999999999</mFullScaleMinVoltage>
  <mFullScaleMaxVoltage>10.245801999999999</mFullScaleMaxVoltage>
</item>
```

These define the full ADC range in volts for each channel and are used
to compute the ADC-to-voltage linear transform. Each channel includes
`mDeviceId`, `mChannelIndex`, and `mDataType` (always 0 for analog).

**Confidence: Confirmed** -- Validated against real .cal file


### Per-Channel, Per-Sample-Rate Calibration Set

```xml
<item class_id="4" tracking_level="0" version="21">
  <mChannel class_id="5" tracking_level="0" version="21">
    <mDeviceId>8520900333282843121</mDeviceId>
    <mChannelIndex>0</mChannelIndex>
    <mDataType>0</mDataType>
  </mChannel>
  <mSampleRate>50000000</mSampleRate>
  <mDf1SosCoefficients>
    <count>30</count>
    <item_version>0</item_version>
    <item>1.1470743525205651</item>   <!-- b0 section 0 -->
    <item>-1.1446541192031381</item>  <!-- b1 section 0 -->
    <item>0</item>                     <!-- b2 section 0 -->
    <item>1</item>                     <!-- a0 section 0 (always 1.0) -->
    <item>-0.99757976668257287</item>  <!-- a1 section 0 -->
    <item>0</item>                     <!-- a2 section 0 -->
    <!-- ... 24 more values for sections 1-4 -->
  </mDf1SosCoefficients>
  <mFirTaps>
    <count>30</count>
    <item_version>0</item_version>
    <item>1.1470743525205651</item>
    <!-- ... identical to mDf1SosCoefficients -->
  </mFirTaps>
  <mGroupDelay>3</mGroupDelay>
  <mImpulseLength>3381</mImpulseLength>
</item>
```

**Confidence: Confirmed** -- Validated against real .cal file

Each calibration set contains:
- `mChannel`: channel reference with `mDeviceId`, `mChannelIndex`, `mDataType`
- `mSampleRate`: ADC sample rate in Hz (integer)
- `mDf1SosCoefficients`: 30 floating-point values (5 sections x 6 values)
- `mFirTaps`: duplicate of mDf1SosCoefficients (historical artifact, ignored by parser)
- `mGroupDelay`: filter group delay in samples
- `mImpulseLength`: effective impulse response length in samples


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

In practice, section 0 always has b2=0 and a2=0, making it effectively
first-order. The total filter is therefore **9th order** (1 + 2 + 2 +
2 + 2 = 9), though the code processes all 5 sections as biquads.

### Coefficient Layout

The XML stores 6 values per section in order: `b0, b1, b2, a0, a1, a2`

a0 is always exactly 1.0 (normalized). The JS parser extracts 5 values
per section, skipping a0:

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

Note: In the JS code, `coefficients[A]._text` uses the `_text` property
because the `xml2json` library wraps text content in `_text` keys. The
actual XML has plain text: `<item>1.147</item>`, not
`<item><_text>1.147</_text></item>`.

### Section Sharing (Validated)

Sections 1-4 are **identical for all channels** at a given sample rate.
Only section 0 varies per channel. This reveals the calibration design:

- **Section 0**: Per-channel AFE correction (gain matching, offset
  compensation for component tolerances between channels)
- **Sections 1-4**: Per-sample-rate filter correction (anti-aliasing
  filter / decimation filter response, same for all channels)

### Practical Dimensions (LogicPro8)

For a Logic Pro 8 with 8 analog channels and 6 sample rates:
- 8 * 6 = 48 calibration sets
- Each set: 5 biquad sections with 5+1 coefficients + groupDelay + impulseLength
- Total: 48 * 30 = 1440 coefficient values
- Unique values: 6 * 24 (shared sections) + 48 * 3 (section 0) = 288

**Confidence: Confirmed** -- Validated against real .cal file


### Available Sample Rates (LogicPro8)

| Rate (Hz)  | MHz    | Division | Group Delay | Impulse Length |
|------------|--------|----------|-------------|----------------|
| 50000000   | 50.000 | /1       | 3           | ~3370-3381     |
| 12500000   | 12.500 | /4       | 5           | ~993-994       |
| 6250000    | 6.250  | /8       | 5           | ~511-524       |
| 3125000    | 3.125  | /16      | 5           | ~256-262       |
| 1562500    | 1.5625 | /32      | 5           | ~128-131       |
| 781250     | 0.781  | /64      | 5           | ~64-66         |

All rates are power-of-2 divisions of the 50 MHz base ADC clock. The
25 MHz rate (50M/2) is not present. Impulse length scales approximately
linearly with sample rate.

**Confidence: Confirmed** -- Validated against real .cal file


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

### Calibrated Voltage Ranges (LogicPro8, real data)

| Channel | Min Voltage  | Max Voltage  | Span     |
|---------|-------------|-------------|----------|
| 0       | -10.444522  | +10.245802  | 20.690V  |
| 1       | -10.424817  | +10.313864  | 20.739V  |
| 2       | -10.480572  | +10.256573  | 20.737V  |
| 3       | -10.432200  | +10.372678  | 20.805V  |
| 4       | -10.367207  | +10.491253  | 20.858V  |
| 5       | -10.324269  | +10.317355  | 20.642V  |
| 6       | -10.424414  | +10.231961  | 20.656V  |
| 7       | -10.427620  | +10.405477  | 20.833V  |

The calibrated values show ~3-5% variation from the nominal +/-10V
range, reflecting per-channel AFE component tolerances.

**Confidence: Confirmed** -- Validated against real .cal file


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

For the validated device, mAdcGain = 0 (default gain, no adjustment).

**Confidence: Confirmed** -- bundle.js and validated against real data


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

**Confidence: Confirmed** -- Validated: file
`8520900333282843121-1568317711765-1.cal` found in
`~/.config/Logic/calibrations/`


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
