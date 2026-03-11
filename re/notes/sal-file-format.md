# .sal Capture File Format

Reverse engineered from Saleae Logic 2 v2.4.40.

Sources:
- `app.asar` JavaScript bundle (save/load logic, version upgrades, JSON schema)
- `libgraph_server_shared.so` (binary serialization, cereal archives)
- Existing notes in `re/notes/waveform-persistence.md` (binary data format details)


## Overview

A `.sal` file is a **standard ZIP archive** (zlib compression level 1) containing:

1. A `meta.json` file with session metadata, capture settings, and a manifest
   of binary data files
2. One or more `.bin` files containing serialized waveform/digital data for
   each captured channel

The Electron app creates the ZIP using the `archiver` npm package (format: "zip",
zlib level: 1). On load, it uses the `extract-zip` npm package to decompress.


## Container Format

```
capture.sal (ZIP archive, zlib level 1)
|
+-- meta.json              # Session metadata, capture settings, file manifest
+-- digital-0.bin           # Digital channel 0 data (cereal binary archive)
+-- digital-1.bin           # Digital channel 1 data
+-- analog-0.bin            # Analog channel 0 data (cereal binary archive)
+-- analog-1.bin            # Analog channel 1 data
+-- ...                     # One .bin per captured channel
```

The extension is just "sal" (not ".sal" in the filter spec). The app uses
`archiver("zip", {zlib: {level: 1}})` for compression, choosing speed over
ratio since capture data can be large.


## File Naming Convention

Binary data files are named by channel identity. Two naming schemes exist:

### Legacy devices (Logic 4/8/Pro 8/Pro 16)

```
./${type}-${deviceChannel}.bin
```

All lowercase. Examples:
- `digital-0.bin` -- Digital channel 0
- `digital-7.bin` -- Digital channel 7
- `analog-0.bin` -- Analog channel 0
- `analog-3.bin` -- Analog channel 3

### MSO/Instrument devices

```
./instrument-${instrumentPort}-${type}-${instrumentChannel}.bin
```

All lowercase. Examples:
- `instrument-1-digital-0.bin`
- `instrument-2-analog-1.bin`


## meta.json Structure

The top-level meta.json has three fields:

```json
{
  "version": 16,
  "data": { /* session persisted data */ },
  "binData": [ /* channel-to-file mapping */ ]
}
```

### version (integer)

The current version is **16** (as of Saleae Logic 2 v2.4.40).

Version history (each version adds fields or restructures):
- Version 4: earliest supported (older versions rejected as BackendIncompatible)
- Version 5: added `measurements[]`
- Version 6: added `highLevelAnalyzers[]`, `rowsSettings`
- Version 7: restructured `analyzers` from object to array
- Version 8: flattened session data into `data` field with capture settings
- Version 9: upgraded glitch filter settings
- Version 10: restructured glitch filter to `{enabled, channels}`
- Version 11: added `dataTable`
- Version 12: removed ASCII hex from analyzer display radix
- Version 13: restructured capture settings with connectedDevice, channels to
  use `{category, type, deviceChannel}` identity format, restructured
  rowsSettings, trigger settings, measurements
- Version 14: added `comment` and `customName` to measurements
- Version 15: restructured `binData` entries to `{category, type, deviceChannel, file}`
- Version 16: current (identity function, no changes from v15)

Files saved with version < 4 are rejected with "BackendIncompatible" error.
Files saved with version > CURRENT_VERSION are rejected with "FutureVersionError".

### binData (array)

Each entry maps a channel identity to its binary data file:

```json
{
  "category": "legacy",
  "type": "Digital",
  "deviceChannel": 0,
  "file": "./digital-0.bin"
}
```

For MSO/instrument devices:

```json
{
  "category": "mso",
  "type": "Analog",
  "instrumentPort": 1,
  "instrumentChannel": 0,
  "file": "./instrument-1-analog-0.bin"
}
```

The `type` field is `"Digital"` or `"Analog"` (case-sensitive, title case).
The `category` is `"legacy"` for standard Logic devices or `"mso"` for MSO.

### data (object) -- Session Persisted Data

The `data` object contains the full session state. Required fields (validated
by JSON schema):

| Field | Type | Description |
|-------|------|-------------|
| `analyzers` | array | Protocol analyzer configurations |
| `captureProgress` | object | `{maxCollectedTime, processedInterval, memoryUsedMb}` |
| `captureSettings` | object | Device type, sample rate, channels, trigger config |
| `dataTable` | object | Data table column layout |
| `digitalTriggerTime` | number | Trigger time in seconds |
| `highLevelAnalyzers` | array | HLA configurations |
| `measurements` | array | Measurement regions |
| `name` | string | Capture name (tab label) |
| `renderViewState` | object | `{leftEdgeTimeSec, timeSecPerPixel, ...}` |
| `rowsSettings` | array | Channel row display settings |
| `timingMarkers` | object | Timing marker/pair state |

Optional fields:

| Field | Type | Description |
|-------|------|-------------|
| `captureStartTime` | object | `{unixTimeMilliseconds, fractionalMilliseconds}` |
| `msoSettings` | object | MSO-specific oscilloscope settings |
| `msoDevice` | object | MSO device metadata |
| `analyzerTrigger` | object | Analyzer-based trigger configuration |
| `timeManager` | object | Time display mode (wall clock / etc.) |
| `captureNotes` | string | User-written notes for the capture |

#### captureSettings Details

```json
{
  "bufferSizeMb": 300,
  "captureMode": "AfterTime",
  "stopAfterSeconds": 5.0,
  "trimAfterCapture": false,
  "trimTimeSeconds": 0,
  "digitalTriggerSettings": {
    "selectedChannel": {"category": "legacy", "type": "Digital", "deviceChannel": 0},
    "linkedChannels": [
      {"channel": {"category": "legacy", "type": "Digital", "deviceChannel": 0}, "state": "X"}
    ],
    ...
  },
  "connectedDevice": {
    "deviceId": "F4F31",
    "name": "Logic Pro 8",
    "deviceType": "LogicPro8",
    "isSimulation": false,
    "capabilities": {
      "channelCapabilities": [...],
      "sampleRateOptions": [...],
      "digitalThresholdOptions": [...],
      "isPhysicalDevice": true
    },
    "settings": {
      "enabledChannels": [
        {"type": "Digital", "index": 0},
        {"type": "Analog", "index": 0}
      ],
      "sampleRate": {"digital": 25000000, "analog": 6250000},
      "digitalThreshold": {"name": "3.3+ Volts", "voltage": 1.65}
    }
  },
  "glitchFilter": {
    "enabled": false,
    "channels": []
  }
}
```

`captureMode` is one of: `"AfterTime"`, `"Circular"`, `"StopOnDigitalTrigger"`.

`deviceType` is one of: `"Logic"`, `"Logic16"`, `"Logic4"`, `"Logic8"`,
`"LogicPro16"`, `"LogicPro8"`.

#### captureStartTime Details

```json
{
  "unixTimeMilliseconds": 1741651200000,
  "fractionalMilliseconds": 0.123456789
}
```

Represents the wall-clock time when capture started. Uses a GraphTime struct
that combines millisecond-precision Unix time with sub-millisecond fractional
component for picosecond-level precision.


## Binary Data Files (.bin)

Each `.bin` file contains channel data serialized by the C++ graph server
(`libgraph_server_shared.so`) using **cereal binary archives**
(`SaleaeBinaryOutputArchive` / `SaleaeBinaryInputArchive`).

The cereal binary format is a sequential little-endian binary stream with no
field tags. Values are written in traversal order. Containers are prefixed
with a uint64_t size.

There are three store types that produce .bin files:

| Store Node | Data Type | Channel Type |
|------------|-----------|--------------|
| DigitalStore | DigitalStoreState2 | Digital channels |
| AnalogStore | AnalogStoreState2 | Analog channels |
| WaveformStoreNode | PackedWaveformData | MSO oscilloscope waveforms |

### Digital Channel .bin Format

Digital data is stored as **Run-Length Encoded (RLE)** transitions. The
DigitalStore serializes via cereal, writing the RLE data and lookup tables.
The `Saleae::Digital::ByteArray` class handles the serialization, loading
via `ByteArray::load<SaleaeBinaryInputArchive>`.

Key data:
- Initial bit state (high or low)
- Transition timestamps as RLE-encoded run lengths
- Lookup table for efficient random access

The DigitalStore reports "Size of serialized data: {}" before writing and
"serialization complete. total size = " after completion.

### Analog Channel .bin Format

Analog data is stored as **int16_t sample arrays** with min/max statistics
at multiple LOD levels. The AnalogStore serializes via cereal, writing
packed sample data in chunks.

See `re/notes/waveform-persistence.md` for the detailed wire format of the
cereal archive, including the PackedWaveformData header (0x90 bytes) and
the chunk serialization order.

Summary of the analog binary format:

```
[8 bytes]  chunk_count (int64_t)

For each chunk:
  [16 bytes] sample_interval (2x int64_t: start, end)
  [~90 bytes] PackedWaveformData header fields (cereal traversal order)
  [8 bytes]  analog_data_count (int64_t)
  [N*2 bytes] analog_data (int16_t samples)
  [8 bytes]  stats_count (int64_t)
  [M*2 bytes] stats_data (int16_t min/max pairs)
```

The int16_t analog values are raw ADC or post-calibration values. Conversion
to volts happens at the rendering/export layer using VoltageScale from the
channel configuration.


## Save Flow

The save process (implemented in Electron JS):

1. Create a temp directory
2. For each channel with data, send `NodeActions::StartSerialization` to the
   graph server with the temp directory path and channel filename
3. The C++ graph server serializes each DigitalStore/AnalogStore/WaveformStoreNode
   to its respective `.bin` file using cereal binary archives
4. Wait for all stores to report SerializationComplete
5. Verify all expected `.bin` files exist in the temp directory
6. Build the `meta.json` object with `{version, data, binData}`
7. Write `meta.json` to the temp directory
8. ZIP the temp directory contents into the `.sal` file using `archiver("zip", {zlib: {level: 1}})`
9. Clean up the temp directory


## Load Flow

The load process (implemented in Electron JS):

1. Validate the file extension is `.sal`
2. Create a temp directory
3. Unzip the `.sal` file into the temp directory using `extract-zip`
4. Read and parse `meta.json` from the temp directory
5. Run `upgradeToCurrent(meta)` to migrate from any supported old version (4+) to current (16)
6. Validate the `data` field against the JSON schema
7. Create a new session with the persisted data
8. For each entry in `binData`, send `NodeActions::StartDeserialization` to the
   graph server with the file path in the temp directory
9. Wait for all stores to report DeserializationComplete
10. Apply persisted render view state, analyzers, measurements, etc.
11. Clean up the temp directory


## Relationship to Automation API

The Saleae gRPC automation API provides `LoadCapture` and `SaveCapture` RPCs
that use the same underlying save/load functions. The automation API accepts
a filepath and calls `loadCapture()` / `saveCaptureTo()` directly.


## Key Source Paths (leaked via assertions/strings)

```
/workspace/monorepo/graph-io/nodes/digital_store/src/digital_store.cpp
/workspace/monorepo/graph-io/nodes/waveform_store_node/src/waveform_store_node/waveform_store_node.cpp
/workspace/monorepo/graph-io/core/saleae_base/include/serialize_data.h
/workspace/monorepo/graph-io/core/digital/include/digital.h
```


## Confidence Assessment

| Finding | Confidence | Source |
|---------|-----------|--------|
| .sal is a ZIP archive | **Confirmed** | JS source: `archiver("zip")`, `unzipFile()` |
| Compression level 1 (speed) | **Confirmed** | JS source: `{zlib: {level: 1}}` |
| Contains meta.json + *.bin | **Confirmed** | JS source: save/load flow explicitly constructs these |
| Version is 16 | **Confirmed** | JS source: `e.CURRENT_VERSION=16` |
| Supports versions 4-16 | **Confirmed** | JS source: upgrade chain `g={4:..., ..., 16:A=>A}` |
| meta.json JSON schema | **Confirmed** | Extracted directly from JS bundle |
| .bin files use cereal binary archives | **Confirmed** | Binary strings + decompiled serialization code |
| fileForChannel naming | **Confirmed** | JS source: explicit string template |
| Channel identity structure | **Confirmed** | JS source: version 15 upgrade + schema |
| Analog data is int16_t arrays | **High** | Decompiled serialization (waveform-persistence.md) |
| Digital data is RLE-encoded | **High** | Binary strings + decompiled code |
| MSO instrument naming scheme | **Medium** | JS source pattern, not tested with real MSO files |
