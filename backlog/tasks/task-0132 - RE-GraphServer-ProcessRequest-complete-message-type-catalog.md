---
id: TASK-0132
title: 'RE GraphServer::ProcessRequest -- complete message type catalog'
status: Done
assignee: []
created_date: '2026-03-11 11:21'
updated_date: '2026-03-12 05:08'
labels:
  - re
  - ghidra
  - ipc
  - graph-server
dependencies: []
references:
  - re/notes/graph-server.md
  - re/notes/ipc-analysis.md
documentation:
  - doc/message-type-catalog.md
  - re/ghidra/decompiled-process-request.txt
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Saleae::GraphServer::ProcessRequest(...) is the main dispatcher handling 47+ messageTypes from the Electron frontend. TASK-0078 documented the message type list but didn't decompile the individual handlers.

Understanding the complete request/response protocol enables:
- Automation API compatibility (send the same messages as the Electron app)
- Understanding exactly what data the frontend sends for each operation
- Potentially driving the original binary from our code for validation

Investigation:
1. Decompile ProcessRequest and its dispatch table
2. For each message type, document: request fields, response fields, side effects
3. Focus on device-related messages: SetSampleRate, StartCapture, StopCapture, GetCalibration, SetChannel
4. Skip GUI-only messages (rendering, zoom, pan, etc.)

Low priority -- we're building a standalone driver, not a frontend replacement.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Completed full message type catalog for GraphServer::ProcessRequest.

Key findings:
1. **50 messageTypes** identified (up from previously documented 47). New ones: `getState`, `sendAnalogTriggerSettings`, `disableAnalogTrigger`, `addSimulationDevice`, `removeSimulationDevice`.
2. **Native dispatch is only 6 types**: The JS worker translates 50 messageTypes into just 6 native C++ request types (`GraphActionData`, `GraphInitRequestData`, `DeleteGraphData`, `CrashRequestData`, `StartTracingData`, `StopTracingData`). Most messages are funneled through `GraphActionData` as graph actions.
3. **10 device-related request sub-types** documented with full request/response field structures: AcquireLogicDeviceRequest, GetDefaultLogicDeviceConfigurationRequest, ReleaseDeviceRequest, EnableChannels, SetSampleRate, SetDigitalVoltageThreshold, SetLed, SetAdcFullScaleGain, StartCapture, StopCapture.
4. **SettingResponse structure** documented: returns updated `configuration.settings` and `configuration.capabilities` (including `sampleRateOptions` and `channelCapabilities`).
5. **Calibration data flow** documented: SetCalibration per analog filter node + SetAdcFullScaleGain to device.
6. **Analog scale transforms** per device type documented (ADC ranges, voltage ranges).
7. **8 USB-relevant message types** identified that directly affect device behavior.

Deliverables:
- `doc/message-type-catalog.md` - Complete catalog with categories, fields, and structures
- `re/ghidra/decompiled-process-request.txt` - Ghidra decompilation of RemoteRequestHandler and related functions
- Updated `re/notes/graph-server.md` with corrected count and cross-reference
<!-- SECTION:FINAL_SUMMARY:END -->
