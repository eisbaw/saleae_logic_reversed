# Calibration System Analysis

Analysis date: 2026-03-09
Binary: libgraph_server_shared.so (48MB, stripped, ELF 64-bit x86-64)
Asar: resources/app.asar (bundle.js, CalibrationManager class)
Source: Saleae Logic 2 v2.4.40


## Summary

The Saleae calibration system has two distinct paths depending on device family:

1. **Logic devices (Logic 4/8/Pro 8/Pro 16)**: Calibration data is a
   Boost.Serialization XML file (`DeviceCalibrationSet`) downloaded from
   Saleae's servers. The file contains per-channel, per-sample-rate IIR
   filter coefficients in Direct Form I Second-Order Sections (DF1 SOS)
   format -- a cascade of 5 biquad filters. The Electron frontend
   downloads, parses, and sends these coefficients to the native
   `AnalogCalibrationFilter` graph node.

2. **MSO devices**: Calibration data is stored in an on-device AT24C64D
   I2C EEPROM (64Kbit). The native library reads it via
   `ReadAfeMetadata` and converts it to a `MsoCalibrationProfile`
   using `MetadataConstantsToCalibration`. The path can be overridden
   with the `SALEAE_MSO_CALIBRATION_PATH` environment variable.


## Calibration Data Source (Logic Devices)

### Server-Hosted Calibration Files

Logic devices download calibration from Saleae's cloud:

**Step 1: Query GraphQL API for download URL**

```
POST https://api.saleae.com/graphql
Content-Type: application/json

{
  "query": "query GetDeviceCalibration ($deviceId: String, $deviceType: Int, $version: String!) {
    getDeviceCalibration(deviceId: $deviceId, deviceType: $deviceType, version: $version) {
      deviceId
      deviceType
      downloadUrl
      lastUpdated
    }
  }",
  "variables": {
    "deviceId": "<device serial as string>",
    "deviceType": <integer enum>,
    "version": "2.4.13"
  }
}
```

Device type enum values:
```
Logic    = 0
Logic16  = 1
Logic4   = 4
Logic8   = 5
LogicPro8  = 6
LogicPro16 = 7
```

**Step 2: Download .cal file from returned URL**

The GraphQL response provides a `downloadUrl`. The manual download URL
format is:
```
https://downloads.saleae.com/calibration/{deviceIdHex}.cal
```
where `deviceIdHex` is `BigInt(deviceId).toString(16)`.

**Step 3: Parse the XML file**

The `.cal` file is Boost.Serialization XML:
```xml
<boost_serialization>
  <DeviceCalibrationSet>
    <px>
      <mDeviceId>
        <mDeviceId><_text>12345678</_text></mDeviceId>
        <mDeviceType><_text>6</_text></mDeviceType>
      </mDeviceId>
      <mCalibrationDate><_text>2023-01-15</_text></mCalibrationDate>
      <mLastUpdatedDate><_text>2023-01-15</_text></mLastUpdatedDate>
      <mMinimumSoftwareVersionRequired><_text>1.0.0</_text></mMinimumSoftwareVersionRequired>
      <mAdcGain><_text>1.0</_text></mAdcGain>
      <mFullScaleVoltageRanges>
        <item>
          <mChannel><mChannelIndex><_text>0</_text></mChannelIndex></mChannel>
          <mFullScaleMinVoltage><_text>-10.0</_text></mFullScaleMinVoltage>
          <mFullScaleMaxVoltage><_text>10.0</_text></mFullScaleMaxVoltage>
        </item>
        <!-- one per channel -->
      </mFullScaleVoltageRanges>
      <mCalibrationData>
        <item>
          <mSampleRate><_text>50000000</_text></mSampleRate>
          <mChannel><mChannelIndex><_text>0</_text></mChannelIndex></mChannel>
          <mGroupDelay><_text>2.5</_text></mGroupDelay>
          <mImpulseLength><_text>30.0</_text></mImpulseLength>
          <mDf1SosCoefficients>
            <item><_text>1.0</_text></item>    <!-- b0 section 0 -->
            <item><_text>-1.5</_text></item>   <!-- b1 section 0 -->
            <item><_text>0.7</_text></item>    <!-- b2 section 0 -->
            <item><_text>1.0</_text></item>    <!-- a0 section 0 (always 1.0) -->
            <item><_text>-1.3</_text></item>   <!-- a1 section 0 -->
            <item><_text>0.6</_text></item>    <!-- a2 section 0 -->
            <!-- 6 values per section, 5 sections = 30 values total -->
          </mDf1SosCoefficients>
        </item>
        <!-- one item per (channel, sampleRate) combination -->
      </mCalibrationData>
    </px>
  </DeviceCalibrationSet>
</boost_serialization>
```

**Step 4: Cache locally**

Parsed calibration is cached to app storage as JSON:
```
calibrations/{deviceId}-{lastUpdated}-1.cal
```
The file contains `{ "version": "1", "data": <parsed object> }`.


### Calibration File Caching Flow

```
1. Check in-memory cache (E[deviceId])
2. If miss: query GraphQL API for downloadUrl and lastUpdated
3. Check local file: calibrations/{deviceId}-{lastUpdated}-1.cal
4. If cached file exists: load and parse JSON wrapper
5. If not cached: HTTP GET downloadUrl -> parse XML -> cache as JSON
6. On complete failure (3 retries): fall back to offline search
7. Offline search: scan calibrations/ dir for matching deviceId
```


## Parsed Calibration Data Structure

After parsing, the JS CalibrationManager produces this structure:

```typescript
interface CalibrationData {
  deviceId: string;
  calibrationDate: string;
  lastUpdatedDate: string;
  minimumSoftwareVersionRequired: string;
  note: string;
  fullScaleVoltageRanges: FullScaleVoltageRange[];
  calibrationSets: CalibrationSets;
  hittiteAdcGainRegisterValue: number;
}

interface FullScaleVoltageRange {
  minimumVoltage: number;  // e.g., -10.0
  maximumVoltage: number;  // e.g., 10.0
}

// Keyed by sample rate string, values are arrays indexed by channel
type CalibrationSets = {
  [sampleRate: string]: ChannelCalibration[];
};

interface ChannelCalibration {
  groupDelay: number;       // filter group delay in samples
  impulseLength: number;    // filter impulse response length
  df1SosCoefficients: BiquadSection[];  // 5 biquad sections
}

interface BiquadSection {
  b0: number;
  b1: number;
  b2: number;
  a1: number;  // a0 is always 1.0 (normalized), not stored
  a2: number;
}
```


## IIR Filter Architecture (DF1 SOS)

The calibration filter uses a cascade of **5 second-order sections (SOS)**
in **Direct Form I (DF1)** topology. Each section is a biquad filter:

```
        b0 + b1*z^-1 + b2*z^-2
H(z) = -------------------------
        1  + a1*z^-1 + a2*z^-2
```

The full filter is H_total(z) = H_0(z) * H_1(z) * H_2(z) * H_3(z) * H_4(z)

### Why SOS (not direct FIR)?

- SOS/biquad cascade is numerically more stable than high-order IIR
- Each biquad is 2nd order, so 5 biquads = 10th order IIR filter
- DF1 is the simplest form: two delay lines (for x and y)
- The binary contains `SimdIirFilterPrototype` and
  `SimdPrototype_Static2By3IirFilter`, confirming SIMD-optimized
  IIR filter implementations

### Note on "FIR" terminology

The PRD and earlier notes referred to "FIR filters" based on string
analysis (`AnalogCalibrationFilter`). However, the actual implementation
is **IIR (Infinite Impulse Response)** using biquad SOS cascade, not
FIR. This is a correction. The `mImpulseLength` field suggests the
designers characterized the effective impulse response length of the
IIR filter, which is finite in practice due to coefficient decay.

### Per-Channel, Per-Sample-Rate

Calibration coefficients vary by:
- **Channel index** (0-7 for Pro 8, 0-15 for Pro 16, etc.)
- **Sample rate** (e.g., 50 MHz, 25 MHz, 12.5 MHz, etc.)

Each (channel, sampleRate) combination has its own set of 5 biquad
sections plus groupDelay and impulseLength metadata. The error string
`"The provided sample rate does not match the sample rates in our
calibration data"` confirms sample rate matching is strict.

### Group Delay

Each calibration set includes `mGroupDelay` -- the filter's group delay
in samples. This is important for time-alignment of analog and digital
channels: the IIR filter introduces a delay that must be compensated
so analog and digital edges align correctly.

### Implementation in the Graph Pipeline

The filter sits in the processing graph as a node:

```
DisaggregatorNode -> VectorData<int16_t> -> AnalogCalibrationFilter
                                              -> VectorData<int16_t> (filtered)
                                              -> AnalogCreatorNode
```

The filter node accepts two actions:
1. `SetCalibration` -- loads the full calibration data object
2. `SetChannel` -- sets the channel index for coefficient selection

And two pipe inputs:
1. `VectorData<int16_t>` -- raw ADC samples
2. `SamplingConfigurationData` -- sample rate for coefficient lookup

The JS frontend calls both actions simultaneously per channel:
```javascript
this.calibrationFilterNode.setCallibration(calibrationData)
this.calibrationFilterNode.setChannel(channelIndex)
```


## ADC-to-Voltage Transform

### Full-Scale Voltage Ranges

Each channel has a factory-calibrated full-scale voltage range:
```
fullScaleVoltageRanges[channelIndex] = {
  minimumVoltage: float,  // e.g., -10.0
  maximumVoltage: float   // e.g., 10.0
}
```

### Default ADC Ranges (before calibration)

```javascript
Logic4:   { min: 0,     max: 255 }    // 8-bit unsigned
Default:  { min: -2048, max: 2047 }   // 12-bit signed
```

### Default Transforms (no calibration)

```javascript
// voltage = adc_code * m + b
Logic:     { m: 1, b: 0 }              // identity
Logic16:   { m: 1, b: 0 }              // identity
Logic4:    compute1DTransform(0, 255, 0, 5)        // 0-255 -> 0-5V
Logic8:    compute1DTransform(-2048, 4095, 0, 5)   // -2048..2047 -> 0-5V
LogicPro8: compute1DTransform(-2048, 4095, -10, 20) // -2048..2047 -> -10..10V
LogicPro16: compute1DTransform(-2048, 4095, -10, 20)
```

Where `compute1DTransform(adcMin, adcRange, voltMin, voltRange)`:
```javascript
m = voltRange / adcRange
b = voltMin - adcMin * m
```

### Calibrated Transform

When calibration data is available, the transform is computed from
the per-channel fullScaleVoltageRanges:
```javascript
convertVoltageRangeToTransform(deviceType, voltageRange) {
  const adcRange = getDefaultAdcRange(deviceType);
  return compute1DTransform(
    adcRange.min,
    adcRange.max - adcRange.min,
    voltageRange.minimumVoltage,
    voltageRange.maximumVoltage - voltageRange.minimumVoltage
  );
}
```

### Native-Side Transform

In the native library, the transform struct has:
```cpp
struct VoltageTransform {
    double mScale;   // multiplier (m)
    double mOffset;  // additive offset (b)
};
```

With assertions:
- `cal.mAdcToVoltageTransform.mScale != 0.0`
- `isfinite(hw_channel.mAdcToVoltageTransform.mScale)`
- `cal.mAdcToDacVoltageScale != 0.0`

The native log string `"Ch {} transform, gain={}, offset={}"` confirms
per-channel transform application.


## Hittite ADC Gain (HMCAD1100)

The calibration file includes `mAdcGain` (float), extracted as
`hittiteAdcGainRegisterValue`. When non-zero, this is sent to the
native library as:

```javascript
{
  messageType: "sendLogicDeviceRequest",
  request: {
    type: "Saleae::Graph::LogicDevice::SetAdcFullScaleGain",
    fullScaleGain: calibrationData.hittiteAdcGainRegisterValue
  },
  sessionId: sessionId
}
```

This adjusts the ADC's internal full-scale gain register (the HMCAD1100
has programmable gain). The binary string `/hittiteAdcGainRegisterValue`
confirms this is a JSON field name used in the calibration data path.


## Devices Requiring Calibration

From the JS constant:
```javascript
devicesRequiringCalibration = ["Logic4", "Logic8", "LogicPro8", "LogicPro16"]
```

All four Logic device types with analog channels require calibration.
Logic (original) and Logic16 (legacy) do not appear in this list.
MSO devices use a separate EEPROM-based calibration path.


## MSO Calibration Path (Out of Scope, but documented)

MSO devices store calibration in an on-device AT24C64D EEPROM:

```
MsoDeviceInterface::BuildAfeEeprom()
  -> Eeprom_AT24C64D (64Kbit I2C EEPROM)
  -> ReadAfeMetadata / WriteAfeMetadata
  -> MsoAfeMetadata::CalibrationConstants
  -> MetadataConstantsToCalibration()
  -> MsoCalibrationProfile
  -> MsoOscilloscopeInstrument::Initialize(MsoCalibrationProfile)
```

Key constants from assertions:
- `MsoAfeMetadata::AFE_CONFIG_COUNT` -- number of AFE configurations
- `MsoAfeMetadata::MAX_AFE_CHANNEL_COUNT` -- max analog channels
- `MsoConstants::TotalAfeConfigurations` -- total config count

The calibration metadata array is:
  `MAX_AFE_CHANNEL_COUNT * AFE_CONFIG_COUNT` entries

The MSO path can be overridden via:
  `SALEAE_MSO_CALIBRATION_PATH` environment variable

Error: `"Scope calibration is missing constants for one or more AFE settings"`


## EEPROM Details (Logic and MSO)

### Logic Devices (FX2/FX3)

```
FX2 EEPROM: 256 bytes ("eeprom is only 256 bytes")
FX3 EEPROM: larger (EepromSizeMax)
```

EEPROM read/write goes through USB vendor control transfers (I2C tunnel):
- `Fx2DeviceFeatures::ReadEeprom(U16 addr, U8* buf, U16 len)`
- `Fx3DeviceFeatures::ReadEeprom(U16 addr, U8* buf, U16 len)`
- `Fx2DeviceFeatures::WriteEeprom(U16 addr, vector<U8>&)`
- `Fx3DeviceFeatures::WriteEeprom(U16 addr, vector<U8>&)`

Assertions:
- `begin_address + data.size() <= EepromSize`
- `buffer.size() <= EepromPageSize()`
- `destination_address < EepromSizeMax()`
- `starting_address < EepromSizeMax()`
- `num_bytes <= EepromPageSize()`

Important: The Logic devices' EEPROMs store device metadata (serial
number, etc.) but NOT calibration coefficients. Calibration data for
Logic devices comes from the cloud server, not the EEPROM.

### MSO Devices (AT24C64D)

The AT24C64D is an 8KB (64Kbit) I2C EEPROM:
- Source: `eeprom_at24c64d.cpp`
- Function: `void Saleae::Eeprom_AT24C64D::Write(uint32_t, gsl::span<uint8_t>)`
- Contains AFE metadata (calibration constants)
- Read via `ReadAfeMetadata`, written via `WriteAfeMetadata`
- I2C address accessed through the FX3 I2C tunnel

The MSO EEPROM calibration path reads `MsoAfeMetadata::CalibrationConstants`
from the EEPROM and converts them to a `MsoCalibrationProfile` via
`MetadataConstantsToCalibration`.


## Calibration Flow Summary (Logic Devices)

```
1. Device connects
2. Frontend checks: devicesRequiringCalibration.includes(deviceType)?
3. If yes:
   a. Query GraphQL API for calibration metadata (URL + lastUpdated)
   b. Check local cache: calibrations/{deviceId}-{lastUpdated}-1.cal
   c. If cached: load JSON
   d. If not cached: download .cal file (Boost XML), parse, cache as JSON
   e. Retry up to 3 times; fall back to any cached file for this device
4. Extract:
   - fullScaleVoltageRanges[channel] -> voltage transform {m, b}
   - calibrationSets[sampleRate][channel] -> 5 biquad sections
   - hittiteAdcGainRegisterValue -> ADC gain register
5. For each analog channel:
   a. Send SetCalibration action to AnalogCalibrationFilter node
   b. Send SetChannel action with channel index
6. If hittiteAdcGainRegisterValue is set:
   a. Send SetAdcFullScaleGain to LogicDeviceNode
7. Voltage transforms applied at display/export time:
   voltage = adc_code * m + b
```


## Key Implications for Rust Implementation

1. **No need to reverse-engineer server protocol in detail**: The
   calibration file format is well-understood (Boost XML). We can
   download `.cal` files directly from
   `https://downloads.saleae.com/calibration/{deviceIdHex}.cal`
   or parse user-provided files.

2. **IIR filter, not FIR**: The calibration applies a 10th-order IIR
   filter (5 biquad sections), not FIR. This is computationally cheaper
   but requires careful state management (the filter has memory).

3. **Per-channel, per-sample-rate**: A typical calibration file may
   contain coefficients for ~8 channels x ~6 sample rates = ~48 sets
   of 5 biquad sections each.

4. **Group delay compensation**: The `groupDelay` field tells us how
   many samples of delay the filter introduces. This must be
   compensated when aligning analog and digital channels.

5. **Voltage transform is separate**: The ADC-to-voltage transform
   is a simple linear scale+offset, computed from the
   fullScaleVoltageRanges. It is NOT part of the IIR filter.

6. **ADC gain register**: For HMCAD1100-based devices, a gain
   register value from the calibration file must be written to the
   ADC before capture.


## Open Questions

1. **Exact SIMD filter implementation**: The `SimdPrototype_Static2By3IirFilter`
   name is cryptic. "2By3" might mean 2 samples processed in parallel
   with 3 coefficients, or 2 sections with 3 taps each. Ghidra
   decompilation would clarify.

2. **Group delay compensation mechanism**: How does the pipeline
   align analog (delayed by IIR filter) with digital channels? Is
   the group delay subtracted from timestamps, or are initial
   samples discarded?

3. **Calibration for ADS528x devices**: The newer Lattice ECP5 devices
   use ADS528x ADCs. Do they use the same server-based calibration
   format, or does the calibration data differ? The `hittiteAdcGainRegisterValue`
   field name suggests the calibration format was designed for
   HMCAD1100 originally.

4. **IIR filter coefficient precision**: Are coefficients double or
   float in the native implementation? The XML stores them as text,
   the JS parses with `parseFloat()`, and the binary has
   SIMD-optimized filter code that might use float32 for performance.

5. **Per-AFE-config calibration**: The MSO path has calibration
   constants per AFE configuration number
   (`AFE_CONFIG_COUNT * MAX_AFE_CHANNEL_COUNT` entries). Do Logic
   devices also vary coefficients by AFE config (voltage range), or
   only by channel and sample rate?

6. **Exact EEPROM layout for Logic devices**: What data is stored in
   the Logic device EEPROM? Serial number, device type, hardware
   revision? Is there any calibration data stored on the device at
   all, or is it purely server-side?


## New Leads (Backlog Candidates)

- The `reading calibration from {}` log message in the native library
  suggests a file-based calibration path even in the native code, not
  just the JS-side download. This could be the `SALEAE_MSO_CALIBRATION_PATH`
  override, or it could indicate an alternate calibration loading path
  for Logic devices.

- The `mMinimumSoftwareVersionRequired` field in the calibration file
  suggests calibration format versioning. Our parser should check this.

- The `xml2json` library used by the JS to parse the Boost XML could
  be replaced by a proper Boost.Serialization XML parser in Rust, or
  we could just use a simple XML parser since the format is
  straightforward.

- The `CalibrationToMetadataConstants` / `MetadataConstantsToCalibration`
  functions convert between two representations. Tracing these in Ghidra
  would reveal the exact binary layout of the EEPROM calibration data.

- The `attn100Transform` and `attn10Transform` JSON fields suggest
  per-attenuator-setting calibration transforms. These may modify the
  voltage transform based on which attenuator stage is active.
