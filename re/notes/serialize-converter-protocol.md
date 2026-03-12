# SerializeConverter Protocol and Data Type Wire Formats

Status: Confirmed (string analysis, Ghidra decompilation, vtable analysis)
Source: TASK-0129
Binary: libgraph_server_shared.so v2.4.40

## Summary

The IPC between Electron and `libgraph_server_shared.so` uses **JSON** over
WebSocket (macOS) or direct function call (Linux). The native side uses
**RapidJSON** for serialization. Data types are serialized using
`Saleae::SerializeConverter<T>` template specializations that read/write
JSON fields via **RapidJSON Pointer** paths (slash-prefixed, e.g., `/sampleRate`).

There are **144 SerializeConverter specializations** total. This document
focuses on the driver-relevant types.

## Serialization Architecture

### Key Types

```
Saleae::ConvertableObject         -- Base class for all serializable types
Saleae::SerializeData             -- Wrapper around rapidjson::Document
Saleae::SerializeConverter<T>     -- Template class with Serialize/Deserialize
Saleae::Serialize(T&, SerializeData&)  -- Free function template for primitives
```

### SerializeConverter Vtable Layout

Each specialization has a vtable with 3 entries:
```
vtable[0] = destructor (thunk_FUN_02eaee00)
vtable[1] = scalar deleting destructor
vtable[2] = base class method (thunk_FUN_02daf820)
```

The actual serialize/deserialize logic is NOT in virtual methods. Instead, it is
**inlined by the compiler** into type-specific static functions that use RapidJSON
Pointer paths to read/write fields. The pattern is:

```cpp
void DeserializeLogicDeviceSettings(SettingsStruct* out, const char* name, SerializeData& doc) {
    DeserializeField(out + 0x00, "/enabledChannels", doc);
    DeserializeField(out + 0x18, "/sampleRate", doc);
    DeserializeField(out + 0x38, "/digitalThreshold", doc);
    DeserializeField(out + 0x58, "/led", doc);
}
```

### Source Path

```
/workspace/monorepo/graph-io/core/saleae_base/include/serialize_data.h
```

Build dependencies: RapidJSON (via vcpkg), Boost.Asio, libc++ (LLVM).

## Driver-Relevant Data Types

### 1. LogicDevice Settings (SetSampleRate, EnableChannels, etc.)

**Deserialized at**: ELF 0x1bf4190 (FUN_01cf4190), 461 bytes

The common settings structure has this JSON layout:

| Offset | JSON Pointer | Type | Purpose |
|--------|-------------|------|---------|
| 0x00 | `/enabledChannels` | object | Which channels are active |
| 0x18 | `/sampleRate` | uint32 | Sample rate in Hz |
| 0x38 | `/digitalThreshold` | float | Digital voltage threshold |
| 0x58 | `/led` | object | LED configuration |

Each field is deserialized by a type-specific function:
- `FUN_01cf5360` -- deserializes `/enabledChannels`
- `FUN_01cf54f0` -- deserializes `/sampleRate`
- `FUN_01cf56c0` -- deserializes `/digitalThreshold`
- `FUN_01cf5850` -- deserializes `/led`

### 2. SettingResponse

**Constructor at**: ELF 0x1bf3530

Contains two sub-objects:
1. A `SettingResponse` header (vtable at 0x2f459e8)
2. A nested `LogicDeviceConfiguration` (vtable at 0x2f44be8)

The `LogicDeviceConfiguration` type name string:
`"Saleae::Graph::LogicDevice::LogicDeviceConfiguration"`

### 3. StartCaptureResponse / StopCaptureResponse

Both have identical layout to SettingResponse -- they also embed a nested
`LogicDeviceConfiguration`. This means capture start/stop responses include
the full current device configuration in the response.

### 4. AcquireLogicDeviceResult

**Constructor at**: ELF 0x1bf3b40, 335 bytes

The largest response type. Contains:
1. `AcquireLogicDeviceResult` header
2. Nested `LogicDeviceConfiguration` (at offset +3)
3. Nested `LogicDeviceState` (at offset +0x1f)

Type name: `"Saleae::Graph::AcquireLogicDeviceResult"`

### 5. LogicDeviceState

**Constructor at**: ELF 0x1bf33d0, 100 bytes

Contains device state information. Has its own vtable (at 0x2f44c20) and
calls `FUN_01d11f70` for sub-object initialization.

### 6. Channel Capabilities (MSO Device)

**Deserialized at**: ELF 0x2d77af0 (FUN_02e77af0), 181 bytes

JSON structure for device channel capabilities:

| JSON Pointer | Type | Purpose |
|-------------|------|---------|
| `/minAdcCode` | int16 | Minimum ADC code value |
| `/maxAdcCode` | int16 | Maximum ADC code value |
| `/analogChannelCount` | uint32 | Number of analog channels |
| `/smartCablePortCount` | uint32 | Smart cable port count |
| `/clockRateHz` | uint64 | Base clock rate |
| `/analogSampleRate` | uint32 | Analog sample rate |
| `/scopeDigitalSampleRate` | uint32 | Digital sample rate (scope mode) |
| `/minAnalogSamplesPerPacket` | uint64 | Min analog samples per USB packet |
| `/maxAnalogSamplesPerPacket` | uint64 | Max analog samples per USB packet |
| `/minDacVoltage` | double | Minimum DAC voltage |
| `/maxDacVoltage` | double | Maximum DAC voltage |
| `/dacBitDepth` | uint32 | DAC bit depth |
| `/ddrMemoryCapacityBytes` | uint64 | DDR memory capacity |
| `/fpgaImageType` | string | FPGA image type identifier |
| `/analogLimit_MaxDownsampleRatio` | uint32 | Max analog downsample ratio |
| `/analogLimit_MinTotalTriggerBufferLength` | uint64 | Min trigger buffer |
| `/analogLimit_MaxTotalTriggerBufferLength` | uint64 | Max trigger buffer |
| `/analogLimit_MinTriggerBufferIndex` | uint32 | Min trigger buffer index |
| `/analogLimit_MaxTriggerBufferIndex` | uint32 | Max trigger buffer index |
| `/analogLimit_MaxHoldoffSamples` | uint64 | Max holdoff samples |
| `/analogLimit_MaxTimeoutSamples` | uint64 | Max timeout samples |
| `/digitalLimit_MinGlitchFilterLength` | uint32 | Min glitch filter length |
| `/digitalLimit_MaxGlitchFilterLength` | uint32 | Max glitch filter length |

Note: This is from the MSO device path, but the Logic device likely shares
many of these fields.

### 7. Channel Configuration Array

**Deserialized at**: ELF 0x2d87100 (FUN_02e87100)

Expects a JSON array of exactly 4 elements (for MSO). Each element contains:
- `/analogSettings` -- analog channel settings
- `/digitalSettings` -- digital channel settings

The array is iterated with stride 0x58 bytes per element.

### 8. Export Configuration

**Deserialized at**: ELF 0x1b15570 (FUN_01c15570), 181 bytes

| JSON Pointer | Purpose |
|-------------|---------|
| `/format` | Export file format |
| `/exportFilepath` | Output file path |
| `/analogChannels` | Analog channels to export |
| `/digitalChannels` | Digital channels to export |
| `/graphInterval` | Time interval for graph |
| `/analogDownsample` | Analog downsample factor |
| `/useWallClockTime` | Use wall clock timestamps |
| `/zeroTime` | Zero-reference time |

## Complete JSON Field Catalog (Driver-Relevant)

All JSON Pointer paths found in the binary that relate to device configuration:

### Core Settings
- `/sampleRate` -- sample rate in Hz
- `/analogSampleRate` -- analog-specific sample rate
- `/scopeDigitalSampleRate` -- digital sample rate in scope mode
- `/sampleRateOptions` -- available sample rate options
- `/enabledChannels` -- enabled channel bitmask/list
- `/digitalThreshold` -- digital voltage threshold
- `/digitalThresholdOptions` -- available threshold options
- `/led` -- LED state
- `/fullScaleGain` -- ADC full-scale gain setting
- `/calibration` -- calibration data

### Channel Configuration
- `/analogChannels` -- analog channel list
- `/digitalChannels` -- digital channel list
- `/channels` -- generic channel list
- `/channelCapabilities` -- device channel capabilities
- `/channelConfigurations` -- per-channel config
- `/channelStates` -- current channel states

### Capture Control
- `/duration` -- capture duration
- `/enabled` -- capture/trigger enable flag
- `/disabled` -- disabled state

### Analog Processing
- `/analogSamplingConfiguration` -- full analog sampling config
- `/analogSettings` -- analog channel settings
- `/analogDownsample` -- downsample configuration
- `/downsampleMode` -- downsample mode selection
- `/downsampleRatio` -- downsample ratio
- `/analogLimit_MaxDownsampleRatio` -- max allowed downsample

### Digital Processing
- `/digitalSettings` -- digital channel settings
- `/digitalPrimitives` -- digital primitive data
- `/digitalChannelMaxGlitchWidth` -- max glitch filter width

### Calibration
- `/calibration` -- calibration data block
- `/calibrationSets` -- calibration set collection
- `/afeChannels` -- AFE channel configurations
- `/afeConfigCalibrations` -- AFE config calibration data
- `/adcToVoltageTransform` -- ADC to voltage transform
- `/adcToDacVoltageScale` -- ADC to DAC voltage scale
- `/vgaLinearGain` -- VGA linear gain
- `/voltageTransform` -- voltage transform
- `/voltageScale` -- voltage scale
- `/minVoltageScale` / `/maxVoltageScale` -- scale limits

### Trigger Configuration
- `/trigger` -- trigger settings object
- `/triggerType` -- trigger type
- `/triggerEnabled` -- trigger enable
- `/triggerVoltage` -- trigger voltage
- `/triggerHysteresis` -- trigger hysteresis
- `/threshold` -- trigger threshold
- `/thresholdVoltage` -- threshold voltage
- `/durationRange` -- pulse trigger duration range
- `/edgeChannel` -- edge trigger channel
- `/pulseChannel` -- pulse trigger channel
- `/timeoutSeconds` -- trigger timeout
- `/preTriggerIndex` / `/postTriggerIndex` -- trigger buffer indices

### Device Info
- `/currentVersion` -- firmware version
- `/firmwarePlugCount` -- firmware plug count
- `/samplePoints` -- available sample points

## Pipe Data Types (Streaming, NOT JSON)

These data types flow through internal pipes between processing nodes.
They are NOT JSON-serialized -- they use custom binary formats in shared
memory:

| Type | Purpose | Format |
|------|---------|--------|
| `AnalogData` | Analog samples | Binary (packed data collection) |
| `DigitalData` | Digital samples | Binary (compressed transitions) |
| `VectorData<int16_t>` | Raw ADC vectors | Binary (int16 array) |
| `OwnedSpan<uint8_t>` | Raw USB data | Binary (byte array) |
| `SamplingConfigurationData` | Sample rate info | JSON-like |
| `CalibrationData` | Calibration coefficients | JSON-like |
| `PipeDone` | End-of-data marker | Empty |

## Implications for Our Driver

### What This Means

1. **The IPC protocol is irrelevant to USB device control.** All USB operations
   happen INSIDE `LogicDeviceNode`. The JSON IPC is only between Electron and
   the native graph server. Our driver bypasses this entirely.

2. **The JSON field names confirm our understanding** of device settings:
   - `/sampleRate`, `/enabledChannels`, `/digitalThreshold` match our
     existing SetSampleRate, EnableChannels, SetDigitalVoltageThreshold EP1 commands.
   - `/fullScaleGain` maps to our SetAdcFullScaleGain.
   - `/calibration` confirms calibration is passed as a data blob.

3. **No new USB protocol info here.** The serialization protocol is for
   the frontend-backend communication, not the USB wire protocol. The
   actual USB commands (EP1 register writes, FPGA bitstream, etc.) are
   encapsulated inside the native library.

4. **The channel capabilities structure** (from MSO device path) gives useful
   parameter bounds (max downsample ratio, trigger buffer sizes, glitch filter
   limits) that could be used to validate our driver's configuration.

### Action Items

None. This is documentation-only. The findings confirm our existing
understanding and do not affect the driver implementation.

## Files Generated

- `re/ghidra/decompiled-serialize-converter.txt` -- thin wrapper decompilations
- `re/ghidra/scripts/DecompileSerializeConverter.java` -- string xref script
- `re/ghidra/scripts/DecompileSerializeConverter2.java` -- inner function script
- `re/ghidra/scripts/DecompileSerializeConverter3.java` -- vtable reader script
- `re/ghidra/scripts/DecompileSerializeConverter4.java` -- deserialize method script
- `re/ghidra/scripts/DecompileSerializeConverter5.java` -- JSON field name xref script

## Confidence Levels

- **High**: Serialization format is JSON via RapidJSON (confirmed by symbol strings)
- **High**: JSON field names are RapidJSON Pointer paths (confirmed by string analysis and decompilation)
- **High**: LogicDevice settings structure layout (confirmed by decompiled FUN_01cf4190)
- **High**: SerializeConverter vtable layout (confirmed by memory reads)
- **Medium**: Channel capabilities field types (inferred from deserializer function signatures)
- **Medium**: Export configuration fields (confirmed by decompilation)
- **Low**: Exact nested object layouts for SettingResponse, AcquireLogicDeviceResult
  (constructor analysis shows composition but not all field names)
