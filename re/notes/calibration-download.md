# Calibration Download Mechanism

Analysis date: 2026-03-11
Binary: libgraph_server_shared.so (48MB, stripped, ELF 64-bit x86-64)
Asar: resources/app.asar (bundle.js CalibrationManager class)
Source: Saleae Logic 2 v2.4.40
Task: TASK-0101


## Answer: How Does the App Get Calibration Data?

For Logic devices (Logic 4/8/Pro 8/Pro 16), calibration data is
**downloaded from Saleae's cloud servers by the Electron frontend**
(JavaScript in bundle.js). The native library (libgraph_server_shared.so)
does NOT download calibration data itself -- it receives the parsed
calibration object from the frontend via the graph action system as
serialized JSON.

For MSO devices, calibration is read from an on-device I2C EEPROM.
This is a completely separate path in a different code module.


## Evidence

### 1. No HTTP/Download URLs in the Native Binary

```
$ strings libgraph_server_shared.so | grep -iE '(https?://|saleae\.com|downloads|graphql)'
http://libusb.info
http://www.w3.org/XML/1998/namespace
http://www.w3.org/2000/xmlns/
```

No Saleae server URLs, no GraphQL endpoints, no HTTP download code.
The native binary contains only libusb and XML namespace URLs.

### 2. CalibrationManager Class in bundle.js (JS Frontend)

The `CalibrationManager` class in bundle.js implements the entire
download flow:

```javascript
// Simplified from minified bundle.js

class CalibrationManager {
    BASE_FOLDER = "calibrations"

    // Step 1: Query GraphQL for download URL
    fetchMetaFromServer = async () => {
        const variables = {
            deviceId: this.deviceId,
            deviceType: this.deviceType,
            version: "2.4.13"
        };
        const body = JSON.stringify({
            query: `query GetDeviceCalibration (...) { ... }`,
            variables
        });
        const response = await axios.post(
            "https://api.saleae.com/graphql",
            body,
            { headers: { "Content-Type": "application/json" }, timeout: 10000 }
        );
        return this.parseMetaResponse(response);
    }

    // Step 2: Download or load from cache
    downloadOrLoadCalibrationData = async (meta) => {
        const fileName = `${this.deviceId}-${meta.lastUpdated}-1.cal`;
        let cached = doesFileExistInAppStorage(fileName, this.BASE_FOLDER);
        if (cached) {
            const data = await readFileFromAppStorage(fileName, this.BASE_FOLDER);
            return JSON.parse(data).data;
        }
        // Download the .cal file
        const { downloadUrl } = meta;
        const url = downloadUrl.startsWith("http") ? downloadUrl : `https://${downloadUrl}`;
        const response = await axios.get(url);
        return this.handleDownloadedCalibrationFile(fileName, response.data);
    }

    // Step 3: Parse Boost XML, cache as JSON
    handleDownloadedCalibrationFile = async (fileName, xmlData) => {
        const parsed = this.parseRawCalibrationFile(xmlData);
        const json = JSON.stringify({ version: "1", data: parsed });
        await writeFileToAppStorage(fileName, this.BASE_FOLDER, json, true);
        return parsed;
    }

    // Step 4: Main entry point with retry logic
    getCalibration = async () => {
        if (inMemoryCache[this.deviceId]) return inMemoryCache[this.deviceId];
        let meta, retries = 3;
        while (retries > 0) {
            retries--;
            try {
                if (!meta) meta = await this.fetchMetaFromServer();
                return await this.downloadOrLoadCalibrationData(meta);
            } catch (e) {
                if (retries === 0) {
                    // Final fallback: search disk for any cached file
                    const offline = await this.searchAndLoadCalibrationOnDisk();
                    if (offline) return (inMemoryCache[this.deviceId] = offline, offline);
                    throw e;
                }
            }
        }
    }

    // Manual download URL (for user-triggered file selection)
    getManualDownloadUrl(deviceId) {
        return `https://downloads.saleae.com/calibration/${value2hex(deviceId)}.cal`;
    }
}
```

### 3. Native Library Receives Calibration via Graph Action JSON

Ghidra decompilation of the SetCalibration action deserializer
(FUN_02dd0200 at ELF 0x2cd0200):

```c
void SetCalibration_Deserialize(CalibrationData *cal, JSON *json) {
    deserialize_string(cal->deviceId,    "/deviceId",                   json);
    deserialize_array (cal->voltageRanges, "/fullScaleVoltageRanges",   json);
    deserialize_array (cal->calibrationSets, "/calibrationSets",       json);
    deserialize_float (cal->adcGain,     "/hittiteAdcGainRegisterValue", json);
}
```

The native library uses JSON pointer paths to extract fields from
the JSON action payload. The fields match exactly what the JS
frontend sends:

| JSON Path                      | Description                     |
|-------------------------------|---------------------------------|
| `/deviceId`                   | Factory device ID (64-bit int)  |
| `/fullScaleVoltageRanges`     | Per-channel voltage ranges      |
| `/calibrationSets`            | Per-(channel,rate) biquad coeffs|
| `/hittiteAdcGainRegisterValue`| ADC gain register value         |

Within each calibration set, these paths are used:
| Path                  | Description                     |
|----------------------|---------------------------------|
| `/df1SosCoefficients`| 30 biquad coefficients          |
| `/groupDelay`        | Filter group delay (samples)    |
| `/impulseLength`     | Effective impulse length        |

### 4. "reading calibration from {}" is MSO-Only

Ghidra xref analysis shows that `reading calibration from {}`,
`SALEAE_MSO_CALIBRATION_PATH`, and
`Scope calibration is missing constants` all live in the SAME function
(FUN_02e10ef0 at ELF 0x2d10ef0). This is the MSO EEPROM calibration
loader, not the Logic device calibration path.

The `reading calibration from {}` log message prints the file path
when MSO calibration is overridden via the `SALEAE_MSO_CALIBRATION_PATH`
environment variable.


## Complete Calibration Data Flow

```
 [Factory]
   |
   | Saleae calibrates each device at the factory.
   | Calibration data is uploaded to Saleae's servers.
   | Device ID (64-bit integer) is stored in device EEPROM.
   |
   v
 [Saleae Cloud]
   |
   | GraphQL API: POST https://api.saleae.com/graphql
   |   query: GetDeviceCalibration(deviceId, deviceType, version)
   |   returns: { downloadUrl, lastUpdated }
   |
   | .cal file: https://downloads.saleae.com/calibration/{hex_id}.cal
   |   format: Boost.Serialization XML
   |   content: DeviceCalibrationSet with per-channel biquad coefficients
   |
   v
 [Electron Frontend (bundle.js)]
   |
   | CalibrationManager.getCalibration():
   |   1. Check in-memory cache (JS object keyed by deviceId)
   |   2. Query GraphQL API for latest metadata (downloadUrl + lastUpdated)
   |   3. Check local disk cache: ~/.config/Logic/calibrations/{id}-{ts}-1.cal
   |   4. If not cached: HTTP GET downloadUrl -> parse Boost XML -> cache as JSON
   |   5. On failure (3 retries): scan calibrations/ dir for any matching file
   |   6. Manual override: user can load .cal file from File > Open dialog
   |
   | Parse Boost XML into JS object:
   |   - xml2json library converts XML to JSON
   |   - Extract fullScaleVoltageRanges[channel] -> {min, max} voltage
   |   - Extract calibrationSets[sampleRate][channel] -> 5 biquad sections
   |   - Extract hittiteAdcGainRegisterValue
   |
   | Cache to disk:
   |   ~/.config/Logic/calibrations/{deviceId}-{lastUpdated}-1.cal
   |   Content: {"version":"1","data":{...parsed calibration...}}
   |
   v
 [Graph Node Action System (IPC)]
   |
   | SessionDeviceManager.sendCalibrationToGraph():
   |   For each analog channel:
   |     1. calibrationFilterNode.setCallibration(calibrationData)
   |        -> action type: "Saleae::Graph::AnalogCalibrationFilterActions::SetCalibration"
   |        -> payload: full calibration object as JSON
   |     2. calibrationFilterNode.setChannel(channelIndex)
   |        -> action type: "Saleae::Graph::AnalogCalibrationFilterActions::SetChannel"
   |        -> payload: { channel: N }
   |   If hittiteAdcGainRegisterValue != 0:
   |     3. graphClient.postMessage({
   |          type: "Saleae::Graph::LogicDevice::SetAdcFullScaleGain",
   |          fullScaleGain: value
   |        })
   |
   v
 [Native Library (libgraph_server_shared.so)]
   |
   | SetCalibration deserializer (ELF 0x2cd0200):
   |   Extracts JSON fields via JSON pointer paths
   |   Builds native CalibrationData struct:
   |     - deviceId (string)
   |     - fullScaleVoltageRanges[] -> VoltageTransform { mScale, mOffset }
   |     - calibrationSets[] -> per-(channel, sampleRate) biquad coefficients
   |     - hittiteAdcGainRegisterValue (float)
   |
   | SetChannel handler:
   |   Selects which channel's coefficients to use for filtering
   |
   | Coefficient lookup (ELF 0x1ad2aa0):
   |   When SamplingConfigurationData arrives:
   |   - Match sample rate against calibrationSets
   |   - Copy matching biquad coefficients (0xd8 bytes per set)
   |   - Error: "The provided sample rate does not match the sample rates
   |     in our calibration data" if no match found
   |
   | Filter application (AnalogCalibrationFilter graph node):
   |   Pipeline: raw ADC (int16) -> 5 biquad SOS cascade -> filtered int16
   |   SIMD-optimized: SimdPrototype_Static2By3IirFilter
   |
   | Voltage transform (ELF 0x2d4a280):
   |   voltage = adc_code * mScale + mOffset
   |   Log: "Ch {} transform, gain={}, offset={}"
   |   Assertions:
   |     - cal.mAdcToVoltageTransform.mScale != 0.0
   |     - isfinite(hw_channel.mAdcToVoltageTransform.mScale)
   |     - cal.mAdcToDacVoltageScale != 0.0
   |
   v
 [Calibrated analog output]
```


## Architecture Summary

| Component | Role | Download? |
|-----------|------|-----------|
| Saleae servers | Host .cal files | N/A |
| bundle.js (CalibrationManager) | Download, parse XML, cache | YES |
| bundle.js (SessionDeviceManager) | Send to native via graph actions | No |
| libgraph_server_shared.so | Receive JSON, apply IIR filter | NO |
| Device EEPROM (FX3) | Store device ID (for lookup) | N/A |

The native library NEVER downloads anything. It receives calibration
data as a JSON object via the graph action system. The Electron
frontend handles all network communication.


## Implications for Rust Implementation

1. **Direct download is simplest**: Our Rust driver can skip the
   GraphQL API entirely and download .cal files directly from:
   `https://downloads.saleae.com/calibration/{device_id_hex}.cal`
   This URL format is confirmed working.

2. **No need to implement the graph action system**: The native
   library's JSON deserialization is just an IPC transport detail.
   Our Rust code parses the .cal XML directly using the existing
   `saleae-proto::calibration::parse_calibration()` function.

3. **Cache format is optional**: The Saleae app caches parsed data
   as JSON (`{"version":"1","data":{...}}`). We cache the raw XML
   instead, which is simpler and eliminates a format dependency.

4. **Device ID is needed for download**: The 64-bit device ID is
   stored in the FX3 EEPROM and read during device initialization.
   Our `CommonDeviceInit` already reads this (TASK-0060).

5. **Offline support**: Keep a local cache of .cal files. On
   download failure, search the cache for any file matching the
   device ID. This matches the Saleae app's fallback behavior.

6. **Manual load**: Support loading .cal files from disk for
   offline use or testing.


## Attenuator Transforms (Pro Devices Only)

The native binary also has JSON paths `/attn100Transform`,
`/attn10Transform`, `/adcToVoltageTransform`, and
`/adcToDacVoltageScale`. These are in separate functions
(ELF 0x2d8abd0, 0x2d8b7b0) from the main calibration path.

These likely support per-attenuator-setting voltage transforms
for Pro devices, which have switchable input attenuators (10x, 100x).
The voltage transform changes depending on which attenuator stage
is active. This is separate from the IIR filter calibration and
is applied on the native side based on capture configuration.


## MSO Calibration (Separate Path, Documented for Completeness)

MSO devices use on-device EEPROM, not cloud download:

```
MsoDeviceInterface::BuildAfeEeprom()
  -> Eeprom_AT24C64D (64Kbit I2C EEPROM at FX3 I2C address)
  -> ReadAfeMetadata() / WriteAfeMetadata()
  -> MsoAfeMetadata::CalibrationConstants
  -> MetadataConstantsToCalibration()
  -> MsoCalibrationProfile
  -> MsoOscilloscopeInstrument::Initialize(MsoCalibrationProfile)
```

The `SALEAE_MSO_CALIBRATION_PATH` environment variable overrides
the EEPROM read with a file path. When set, the log prints
`reading calibration from {path}`.

All MSO calibration code is in a single function (FUN_02e10ef0 at
ELF 0x2d10ef0), confirmed by Ghidra xref analysis showing all three
MSO-related strings reference the same function.


## Function Map (ELF VAs)

| ELF VA     | Purpose |
|------------|---------|
| 0x2cd0200  | SetCalibration deserializer (JSON -> native struct) |
| 0x2cd00e0  | /calibration path handler wrapper |
| 0x2cd2010  | df1SosCoefficients array parser |
| 0x1ad2aa0  | Coefficient lookup by sample rate |
| 0x1ad6130  | /calibration path handler (second reference) |
| 0x2d10ef0  | MSO calibration loader (EEPROM + file override) |
| 0x2d43b70  | Voltage transform assertion checks |
| 0x2d4a280  | Per-channel transform application + logging |
| 0x2d8abd0  | Attenuator transform handler (attn10/attn100) |
| 0x2d8b7b0  | adcToVoltageTransform + adcToDacVoltageScale handler |
| 0x2d8ac60  | adcToVoltageTransform + adcToDacVoltageScale (second ref) |
