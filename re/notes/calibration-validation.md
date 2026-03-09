# Calibration File Validation

Validation date: 2026-03-09
Source: Real calibration file for a Logic Pro 8 device
Device ID: 8520900333282843121 (hex: 764051B51BD4D9F1)
Device Type: 6 (LogicPro8)
Calibration Date: 2019-09-13
File size: 112,961 bytes (Boost.Serialization XML)
File location: re/calibration-764051b51bd4d9f1.cal

This document validates the TASK-0013 calibration format documentation
(doc/calibration/calibration-format.md and re/notes/calibration-system.md)
against a real calibration file downloaded from Saleae's servers.


## Data Sources

### Source 1: Local cached file
Path: `~/.config/Logic/calibrations/8520900333282843121-1568317711765-1.cal`
This is the JSON-parsed and cached version from a previous Saleae Logic 2
session. The filename format matches our documented `{deviceId}-{lastUpdated}-1.cal`.

### Source 2: Raw .cal download
Downloaded from: `https://downloads.saleae.com/calibration/764051b51bd4d9f1.cal`
The GraphQL API returned this URL when queried with deviceId=8520900333282843121
and deviceType=6.

### Source 3: GraphQL API
Query:
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

Note: The GraphQL query requires newlines (`\n`) in the query string to
work. Without them, the API returns 400 Bad Request. The Content-Type
and Accept headers must both be `application/json`.


## Validation Results

### CONFIRMED: XML Root Structure

The XML structure matches our documentation:
```xml
<boost_serialization signature="serialization::archive" version="10">
  <DeviceCalibrationSet class_id="0" tracking_level="0" version="1">
    <px class_id="1" tracking_level="1" version="21" object_id="_0">
```

Boost.Serialization attributes (class_id, tracking_level, version,
object_id) are present on many elements. These were not in our docs but
are standard Boost.Serialization XML.


### CORRECTION: No _text Wrappers in Actual XML

Our documentation shows XML like:
```xml
<mDeviceId><_text>12345678</_text></mDeviceId>
```

The actual XML is:
```xml
<mDeviceId>8520900333282843121</mDeviceId>
```

The `_text` wrapper is an artifact of the `xml2json` library used by the
JS frontend to convert XML to JSON. It does NOT appear in the actual .cal
file. Our documentation incorrectly conflated the xml2json representation
with the actual XML structure.


### CONFIRMED: Device Type Enum

mDeviceType = 6 for LogicPro8, matching our documented enum.


### CONFIRMED: 48 Calibration Entries (8 channels x 6 sample rates)

- Channels: 0, 1, 2, 3, 4, 5, 6, 7 (8 total)
- Sample rates: 781250, 1562500, 3125000, 6250000, 12500000, 50000000 Hz
- Total entries: 48


### CONFIRMED: 30 Coefficients Per Entry (5 sections x 6 values)

Every calibration entry has exactly 30 DF1 SOS coefficients.


### CONFIRMED: a0 is Always 1.0

All a0 values (indices 3, 9, 15, 21, 27 in the 30-coefficient array)
are exactly 1.0 across all 48 entries. The JS parser correctly skips
these.


### CONFIRMED: Coefficient Layout

b0, b1, b2, a0, a1, a2 per section, as documented.


### CONFIRMED: Full-Scale Voltage Ranges

8 entries, one per channel, each with mFullScaleMinVoltage and
mFullScaleMaxVoltage. Actual values are slightly asymmetric around
+/-10V due to per-channel factory calibration:

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

The default for LogicPro8 is +/-10V. The calibrated values show
~3-5% variation from the nominal range, which is typical for
analog front-end component tolerances.


### CONFIRMED: mAdcGain = 0

For this device, mAdcGain is 0. This means the default HMCAD1100
gain register value is used (no gain adjustment needed for this
particular device).


## New Findings Not in TASK-0013 Documentation

### 1. Device ID is a 64-bit Integer

The deviceId is `8520900333282843121` (decimal), which in hex is
`764051B51BD4D9F1`. This is NOT related to the USB serial number.
The USB serial "0000000004BE" from TASK-0033 is a different value
(likely read from the FX3 SPI boot EEPROM or assigned by FX3 firmware).

The calibration device ID appears to be a random 64-bit value assigned
at factory calibration time and stored somewhere on the device (likely
in the I2C EEPROM accessible via the FX3 I2C tunnel).

The URL conversion is: `BigInt(deviceId).toString(16)` -> lowercase hex.


### 2. mCalibrationType Field

```xml
<mCalibrationType>1</mCalibrationType>
```

This field was not in our documentation. Value is `1` for this device.
Likely an enum indicating the calibration data version/type.


### 3. mFirTaps Field (Duplicate of mDf1SosCoefficients)

Each calibration entry contains BOTH `mDf1SosCoefficients` and
`mFirTaps`, and they contain identical values:

```xml
<mDf1SosCoefficients>
  <count>30</count>
  <item_version>0</item_version>
  <item>1.1470743525205651</item>
  ...
</mDf1SosCoefficients>
<mFirTaps>
  <count>30</count>
  <item_version>0</item_version>
  <item>1.1470743525205651</item>
  ...
</mFirTaps>
```

The `mFirTaps` name is a historical artifact from when the filter was
FIR (or when Saleae considered using FIR). The JS parser only reads
`mDf1SosCoefficients` and ignores `mFirTaps`. The Boost.Serialization
format preserves both for backward compatibility.


### 4. mDataType Field Per Channel

Each channel reference includes `mDataType`:
```xml
<mChannel>
  <mDeviceId>8520900333282843121</mDeviceId>
  <mChannelIndex>0</mChannelIndex>
  <mDataType>0</mDataType>
</mChannel>
```

The value is `0` for all entries. This likely distinguishes analog (0)
from digital channels or different data formats. The JS parser ignores
this field.


### 5. Channel Reference Includes Device ID

Each `mChannel` element includes the full `mDeviceId`, not just the
channel index. This was not in our documentation but makes sense for
Boost.Serialization's reference-tracking system.


### 6. Section 0 is First-Order (b2=0, a2=0)

Section 0 always has b2=0 and a2=0 across ALL 48 entries. This makes
it effectively a first-order IIR filter:

```
H_0(z) = (b0 + b1*z^-1) / (1 + a1*z^-1)
```

The total filter is therefore 9th order (1 + 2 + 2 + 2 + 2 = 9), not
10th order as stated in our documentation. The "5 biquad sections"
description is technically correct for the code path (which processes
all 5 as biquads), but the effective filter order is 9.


### 7. Sections 1-4 are Shared Across All Channels

At any given sample rate, sections 1-4 are IDENTICAL for all 8
channels. Only section 0 varies per channel.

This reveals the calibration architecture:
- **Section 0**: Per-channel correction for AFE component variations
  (gain, offset, frequency response differences between channels)
- **Sections 1-4**: Per-sample-rate correction for the ADC/decimation
  filter response (anti-aliasing filter characteristics that depend
  on sample rate but not on which channel)

This means only 30 unique coefficient values per sample rate for
sections 1-4, plus 3 unique values per channel per rate for section 0
(b0, b1, a1 -- since b2=0 and a2=0 are constant).

Sections 1-4 at each sample rate:

| Rate (Hz) | Section 1 b0     | Section 1 a1     | Section 1 a2     |
|-----------|-----------------|-----------------|-----------------|
| 781250    | 0.000845        | -0.537134       | 0.077260        |
| 1562500   | 0.000845        | -0.537134       | 0.077260        |
| 3125000   | 0.000845        | -0.537134       | 0.077260        |
| 6250000   | 0.000845        | -0.537134       | 0.077260        |
| 12500000  | 0.000845        | -0.537134       | 0.077260        |
| 50000000  | 0.004047        | -0.211443       | 0.017001        |

Interesting: sections 1-4 are the SAME for rates 781.25 kHz through
12.5 MHz, and only change at 50 MHz. This suggests the decimated
rates (781.25k to 12.5M) share one analog filter configuration while
50 MHz uses a different one.


### 8. Sample Rate Pattern

The actual sample rates are:
```
50,000,000 Hz  (50 MHz)     -- base rate
12,500,000 Hz  (12.5 MHz)   -- 50M / 4
 6,250,000 Hz  (6.25 MHz)   -- 50M / 8
 3,125,000 Hz  (3.125 MHz)  -- 50M / 16
 1,562,500 Hz  (1.5625 MHz) -- 50M / 32
   781,250 Hz  (0.78125 MHz)-- 50M / 64
```

These are all power-of-2 divisions of the 50 MHz base ADC clock.
Note: 25 MHz is NOT present (50M / 2 is skipped).


### 9. Group Delay Pattern

| Rate (Hz) | Group Delay (samples) |
|-----------|----------------------|
| 781250    | 5.0                  |
| 1562500   | 5.0                  |
| 3125000   | 5.0                  |
| 6250000   | 5.0                  |
| 12500000  | 5.0                  |
| 50000000  | 3.0                  |

The group delay is 5 samples for all decimated rates and 3 samples
for the base 50 MHz rate. This reflects different filter
characteristics at the highest rate.


### 10. Impulse Length Scaling

| Rate (Hz)  | Impulse Length (samples) |
|------------|------------------------|
| 781250     | 64 - 66                |
| 1562500    | 128 - 131              |
| 3125000    | 256 - 262              |
| 6250000    | 511 - 524              |
| 12500000   | 993 - 994              |
| 50000000   | 3370 - 3381            |

Impulse length scales approximately linearly with sample rate, which
is expected: the same temporal response requires proportionally more
samples at higher sample rates. The small variation across channels
reflects per-channel differences in section 0 coefficients.


## Corrections Required in Documentation

### doc/calibration/calibration-format.md

1. **Remove _text wrappers from XML examples** -- The actual XML uses
   plain elements like `<mDeviceId>value</mDeviceId>`, not
   `<mDeviceId><_text>value</_text></mDeviceId>`.

2. **Add mCalibrationType field** to the XML structure.

3. **Add mFirTaps field** to the calibration entry structure (note
   that it duplicates mDf1SosCoefficients and is ignored by the parser).

4. **Add mDataType field** to channel references.

5. **Correct filter order** from "10th order" to "effectively 9th
   order" since section 0 is always first-order.

6. **Add actual sample rates** for LogicPro8: 781250, 1562500,
   3125000, 6250000, 12500000, 50000000 Hz.

7. **Document section sharing** -- sections 1-4 are identical across
   channels at a given sample rate.

8. **Add Boost.Serialization attributes** documentation (class_id,
   tracking_level, version, object_id, count, item_version).

9. **Document device ID format** -- 64-bit integer, not related to
   USB serial number.

### re/notes/calibration-system.md

1. **Add mCalibrationType** to the XML structure description.

2. **Add section sharing analysis**.

3. **Correct group delay** values (5.0 for decimated, 3.0 for 50 MHz).


## Cross-Reference with Cached JSON

The locally cached JSON file
(`8520900333282843121-1568317711765-1.cal`) was compared against the
parsed XML. All values match exactly (within floating-point
representation differences), confirming that the JS xml2json parser
correctly extracts all calibration data from the Boost.Serialization
XML format.

Key JSON structure observations:
- The JSON is nested as `{"version":"1","data":{...}}`
- `calibrationSets` is keyed by sample rate string (e.g., "781250")
- Each sample rate maps to an array of 8 channel calibrations
- Coefficients are stored as `df1SosCoefficients` array of objects
  with `b0, b1, b2, a1, a2` (a0 stripped)
- `hittiteAdcGainRegisterValue` is `0`
- `fullScaleVoltageRanges` is an array of `{minimumVoltage, maximumVoltage}`


## GraphQL API Behavior

The GraphQL API at `https://api.saleae.com/graphql`:
- Returns `null` for unknown device IDs (not an error)
- Returns the download URL without the `https://` prefix
  (just `downloads.saleae.com/calibration/...`)
- The `version` variable is required (String!) and is checked against
  the `mMinimumSoftwareVersionRequired` field
- Response includes `lastUpdated` as a Unix timestamp in milliseconds

When queried with the device serial "0000000004BE" (converted to
decimal as "1214"), the API returned null. This confirms that the
calibration device ID is NOT the USB serial number from the SPI boot
state.


## Files

- `re/calibration-764051b51bd4d9f1.cal` -- raw .cal file from Saleae's
  server (Boost.Serialization XML, 112,961 bytes)
- `~/.config/Logic/calibrations/8520900333282843121-1568317711765-1.cal`
  -- cached JSON version from Saleae Logic 2
