---
id: TASK-0129
title: RE graph node data types and SerializeConverter protocol
status: Done
assignee: []
created_date: '2026-03-11 11:21'
updated_date: '2026-03-12 04:59'
labels:
  - re
  - ghidra
  - ipc
  - data-format
dependencies: []
references:
  - re/notes/graph-server.md
  - re/notes/graph-architecture.md
  - re/notes/ipc-analysis.md
  - re/notes/serialize-converter-protocol.md
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The graph processing pipeline uses 17+ data types flowing between nodes via Saleae::SerializeConverter&lt;T&gt; and Saleae::ConvertableObject. These define the internal IPC between the Electron frontend and native graph server.

Known data types:
- AnalogData, DigitalData, VectorData&lt;short&gt;, SamplingConfigurationData
- AnalogTriggerSettingsData, DigitalTriggerSettingsData, AnalyzerTriggerSettingsData
- AnalogTriggerResultData, DigitalTriggerResultData, AnalyzerTriggerResultData
- CalibrationData, NavigateTrigger, SetCurrentTrigger, MemoryTrigger
- LogicDevice::SetSampleRate, AnalogDownsamplerActions::SetAnalogDownsampleRate
- AnalogCalibrationFilterActions::{SetChannel, SetCalibration}
- WorkRequest, NewAnalyzerRunStarted, KernelRequest

Investigation:
1. Decompile SerializeConverter specializations for key types
2. Document the serialization format (JSON? binary? protobuf?)
3. Map the data type names to their wire formats
4. Focus on CalibrationData and SamplingConfigurationData as most relevant to driver

Low priority -- only needed if we want to interop with the graph server directly.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
## Findings

### Serialization Format
The IPC between Electron and libgraph_server_shared.so uses **JSON via RapidJSON**. The native side uses `Saleae::SerializeConverter<T>` template specializations (144 total) that read/write JSON fields using **RapidJSON Pointer** paths (slash-prefixed, e.g., `/sampleRate`).

### Key Architecture
- `Saleae::ConvertableObject` -- base class for all serializable types
- `Saleae::SerializeData` -- wrapper around `rapidjson::Document`
- `Saleae::SerializeConverter<T>` -- template with vtable (3 entries: destructor, scalar deleting destructor, base method)
- Actual serialize/deserialize logic is **compiler-inlined** into static functions, not in virtual methods
- Source: `/workspace/monorepo/graph-io/core/saleae_base/include/serialize_data.h`

### Driver-Relevant JSON Fields (confirmed by decompilation)
LogicDevice settings deserialized at ELF 0x1bf4190:
- `/enabledChannels` (offset 0x00) -- active channels
- `/sampleRate` (offset 0x18) -- sample rate Hz
- `/digitalThreshold` (offset 0x38) -- voltage threshold
- `/led` (offset 0x58) -- LED config

Response types (SettingResponse, StartCaptureResponse, StopCaptureResponse) all embed a nested `LogicDeviceConfiguration` sub-object. AcquireLogicDeviceResult contains both LogicDeviceConfiguration and LogicDeviceState.

### 100+ JSON field names cataloged
Including `/analogSampleRate`, `/digitalChannels`, `/analogChannels`, `/fullScaleGain`, `/calibration`, `/analogSamplingConfiguration`, trigger fields, calibration fields, and channel capability limits.

### Impact on Driver
**None.** The IPC protocol is between Electron and the native graph server. Our driver bypasses this entirely by talking directly to the USB device. The findings confirm our existing understanding of device settings but reveal no new USB protocol information.

### Files Created
- `re/notes/serialize-converter-protocol.md` -- comprehensive documentation
- `re/ghidra/scripts/DecompileSerializeConverter[1-5].java` -- Ghidra analysis scripts
- `re/ghidra/decompiled-serialize-converter.txt` -- decompiled wrapper functions
<!-- SECTION:FINAL_SUMMARY:END -->
