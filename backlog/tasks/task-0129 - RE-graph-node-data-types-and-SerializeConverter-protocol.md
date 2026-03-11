---
id: TASK-0129
title: RE graph node data types and SerializeConverter protocol
status: To Do
assignee: []
created_date: '2026-03-11 11:21'
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
