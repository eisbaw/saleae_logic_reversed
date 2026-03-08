# IPC Analysis: Electron <-> libgraph_server_shared.so

## Summary

Saleae Logic 2 v2.4.40 uses a **JSON message-passing protocol** between the
Electron renderer and `libgraph_server_shared.so`. There are two transport
modes and a separate gRPC automation API. The internal protocol uses
`Saleae::Graph::*` namespaced message types over JSON.

## Architecture

```
Renderer (bundle.js)
  |  postMessage (Worker API)
  v
Web Worker (6a4322176d988210675b.worker.js)
  |  GraphSocket class
  |  Two transport modes:
  |
  +--[direct mode]---> graph-interface.node (N-API addon)
  |                       |  dlopen() / dlsym()
  |                       |  Instance._send(JSON.stringify(msg))
  |                       |  Instance._tryRecv() -> JSON.parse()
  |                       v
  |                    libgraph_server_shared.so
  |                       C API: CreateGraphServer / DestroyGraphServer
  |                       C API: DriveRequest / PopResponse / FreeResponseBuffer
  |                       C API: SetLogFileName / SetLogCallback / FlushLog
  |
  +--[websocket mode]--> WebSocket ws://localhost:{port}/saleae
  |                       |  JSON.stringify({type:"request", contents: msg})
  |                       v
  |                    libgraph_server_shared.so (out-of-process)
  |                       Started via FFI from main process:
  |                       CreateGraphServer(port_ptr, 100, isDevMode)
  |                       Returns a WebSocket server on {port}
  |
  +--[mock mode]-------> No backend (development/testing)
```

### Additionally: gRPC Automation API (separate)

```
External Client (Python SDK, etc.)
  |  gRPC on port 10430 (default)
  |  Package: saleae.automation
  v
libgraph_server_shared.so
```

## Transport Mode Selection

### Direct Mode (default on Linux)

The renderer passes `logic_shared_path`, `python_home_path`, and
`log_file_name` as URL query parameters. The worker's `GraphSocket.loadDirect()`
creates a `@saleae/graph-interface` `Instance` object, which:

1. Takes `(graphServerSharedPath, pythonHomePath, logPath)` as constructor args
2. The native addon `graph-interface.node` calls `dlopen()` on the .so path
3. Resolves symbols: `CreateGraphServer`, `DestroyGraphServer`,
   `DriveRequest`, `PopResponse`, `FreeResponseBuffer`,
   `SetLogFileName`, `SetLogCallback`, `FlushLog`
4. Communication via `Instance.send(jsonObj)` and `Instance.recv()` (async)
5. Under the hood: `_send()` serializes to JSON string, calls `DriveRequest`
   in the native lib. `_tryRecv()` calls `PopResponse` to get response JSON.
   `_onNotifyMessage()` is called from a threadsafe callback when the native
   lib has a response ready.

The Instance.send() calls serialize the previous promise, so requests are
sequential (one at a time).

### WebSocket Mode (macOS, or when `needsOutOfProcessBackend()` returns true)

The main process uses `ffi-napi` to load `libgraph_server_shared.so` and call:
```js
CreateGraphServer(portPtr, 100, isDevMode)
// portPtr: int* that receives the assigned port number
// 100: some configuration parameter
// isDevMode: bool
```

This starts a WebSocket server. The renderer connects to
`ws://localhost:{port}/saleae`.

Note: The FFI path explicitly throws "not supported" for both "win32" and
"linux", meaning on Linux, direct mode is always used. The FFI/WebSocket
path is for macOS only (or when `--useExistingGraph` is passed).

### Mock Mode

When `--useMock` is passed, no backend is used. For development/testing only.

## Message Protocol

### Envelope Format

All messages are wrapped in an envelope:

**Outgoing (JS -> native):**
```json
{
  "type": "request",
  "contents": { ... message ... }
}
```

**Incoming (native -> JS):**
```json
{
  "type": "response" | "error",
  "contents": {
    "id": <int>,
    "type": "<Saleae::* type string>",
    "meta": {
      "sessionId": "<uuid>",
      "destination": "GraphServer" | "Graph",
      "portName": "<string>",
      "portIndex": <int>
    },
    ...payload fields...
  }
}
```

### Request/Response Pattern

- Each request has a unique `id` (monotonic counter from `getNewRequestId()`)
- Requests specify a `meta.destination` ("GraphServer" or "Graph")
- Responses are routed back by matching `id` and `meta` fields
- Pipe subscriptions match on `sessionId`, `portName`, `portIndex`, `destination`
- Timeouts are enforced client-side (default 60s, some operations 10s)

### Session Management

- `GraphInitRequestData` creates a new graph session with a UUID sessionId
- `DeleteGraphData` destroys a session
- All subsequent operations reference the sessionId in meta
- Multiple sessions can be active simultaneously (tabs)

## Complete Message Type Catalog

### Graph Server Control

| Type | Direction | Purpose |
|------|-----------|---------|
| `Saleae::Graph::GraphInitRequestData` | JS->native | Create new graph session |
| `Saleae::Graph::DeleteGraphData` | JS->native | Destroy graph session |
| `Saleae::Graph::CrashRequestData` | JS->native | Force crash (debugging) |
| `Saleae::StartTracingData` | JS->native | Start performance tracing |
| `Saleae::StopTracingData` | JS->native | Stop tracing, save to file |

### Graph Actions (batched via GraphActionData)

| Action Type | Purpose |
|-------------|---------|
| `Saleae::Graph::GraphActions::AddNode` | Add processing node to graph |
| `Saleae::Graph::GraphActions::RemoveNode` | Remove node |
| `Saleae::Graph::GraphActions::AddPipe` | Connect nodes with data pipe |
| `Saleae::Graph::GraphActions::RemovePipe` | Disconnect pipe |
| `Saleae::Graph::GraphActions::ConnectApi` | Connect API endpoint |
| `Saleae::Graph::GraphActions::DisconnectApi` | Disconnect API endpoint |
| `Saleae::Graph::GraphActions::SetPlugins` | Configure plugins |
| `Saleae::Graph::GraphActions::SetPorts` | Configure ports |
| `Saleae::Graph::GraphActions::Start` | Start graph processing |
| `Saleae::Graph::GraphActions::Stop` | Stop graph processing |
| `Saleae::Graph::GraphActions::RouteAction` | Send action to specific node |

### Device Operations (routed to LogicDeviceNode)

Operations on the device are routed through `GraphActions::RouteAction` to
`LogicDeviceNode` (node ID 4). The actual device request payloads are opaque
JSON objects routed to the native `Saleae::Graph::LogicDeviceNode`.

Device-related types:
- `Saleae::Graph::LogicDeviceNode` -- node type for device operations
- `Saleae::Graph::DeviceListNode` -- lists connected devices
- `Saleae::Graph::AddSimulationDevice` -- add virtual device
- `Saleae::Graph::RemoveSimulationDevice` -- remove virtual device

### Trigger Settings

| Type | Purpose |
|------|---------|
| `Saleae::Graph::DigitalTriggerSettingsData` | Configure digital trigger |
| `Saleae::Graph::DigitalTriggerResultData` | Trigger hit notification |
| `Saleae::Graph::AnalogTriggerSettingsData` | Configure analog trigger |
| `Saleae::Graph::AnalyzerTriggerSettingsData` | Configure analyzer trigger |
| `Saleae::Graph::AnalyzerTriggerResultData` | Analyzer trigger hit |

Digital trigger payload structure:
```json
{
  "type": "Saleae::Graph::DigitalTriggerSettingsData",
  "id": <int>,
  "enabled": true,
  "trigger": {
    "type": "EdgeTrigger" | "PulseTrigger",
    "edgeType": "<pattern>",         // EdgeTrigger
    "edgeChannel": <portIndex>,      // EdgeTrigger
    "pulseType": "<pattern>",        // PulseTrigger
    "pulseChannel": <portIndex>,     // PulseTrigger
    "durationRange": {"begin": <f>, "end": <f>},  // PulseTrigger
    "bitStatesRequiredDuringEdge": [...],          // EdgeTrigger
    "bitStatesRequiredDuringPulse": [...]          // PulseTrigger
  },
  "meta": { "sessionId": "...", "portName": "...", ... }
}
```

Analog trigger payload structure:
```json
{
  "type": "Saleae::Graph::AnalogTriggerSettingsData",
  "id": <int>,
  "triggerType": "AboveToBelow" | "BelowToAbove",
  "enabled": true,
  "autoMode": false,
  "threshold": <float>,
  "holdoffSeconds": 0.06,
  "timeoutSeconds": 1,
  "meta": { "sessionId": "...", "portName": "...", "portIndex": <channel>, ... }
}
```

### Rendering / Visualization

| Type | Purpose |
|------|---------|
| `Saleae::Graph::RenderRequestData` | Request waveform render data |
| `Saleae::Graph::AnalyzerRenderRequestData` | Request analyzer render |
| `Saleae::Graph::AnalyzerRenderResultData` | Analyzer render result |
| `Saleae::Graph::AnalogRenderData` | Analog render data |
| `Saleae::Graph::DigitalRenderData` | Digital render data |
| `Saleae::Graph::MeasurementRequestData` | Request measurement |
| `Saleae::Graph::MeasurementData` | Measurement result |
| `Saleae::Graph::RangeMeasurementRequestData` | Range measurement request |
| `Saleae::Graph::RangeMeasurementData` | Range measurement result |
| `Saleae::Graph::VectorData` | Vector/waveform data |

### Analyzer Operations

| Type | Purpose |
|------|---------|
| `Saleae::Graph::AnalyzerNode` | Node type for protocol analyzers |
| `Saleae::Graph::AnalyzerExportNode` | Export analyzer data |
| `Saleae::Graph::AnalyzerExportCreateJobData` | Start export job |
| `Saleae::Graph::AnalyzerExportCancelJobData` | Cancel export job |
| `Saleae::Graph::AnalyzerExportProgressData` | Export progress |
| `Saleae::Graph::AnalyzerExportErrorData` | Export error |
| `Saleae::Graph::HlaNode` | High-level analyzer node |
| `Saleae::Graph::HlaActions::LoadAnalyzer` | Load HLA |
| `Saleae::Graph::HlaActions::SetSettings` | Configure HLA |

### Legacy Analyzer Actions

| Type | Purpose |
|------|---------|
| `Saleae::Graph::LegacyAnalyzerActions::LoadAnalyzers` | Load analyzer plugins |
| `Saleae::Graph::LegacyAnalyzerActions::SelectAnalyzer` | Select analyzer |
| `Saleae::Graph::LegacyAnalyzerActions::SetAnalyzerSettings` | Configure |
| `Saleae::Graph::LegacyAnalyzerActions::StartAnalysis` | Start analysis |
| `Saleae::Graph::LegacyAnalyzerActions::StopAnalysis` | Stop analysis |
| `Saleae::Graph::LegacyAnalyzerActions::StartExport` | Start data export |
| `Saleae::Graph::LegacyAnalyzerActions::StopExport` | Stop export |
| `Saleae::Graph::LegacyAnalyzerActions::StartSimulation` | Start sim |
| `Saleae::Graph::LegacyAnalyzerActions::StopSimulation` | Stop sim |
| `Saleae::Graph::LegacyAnalyzerActions::OpenDatabase` | Open analyzer DB |

### Data Processing

| Type | Purpose |
|------|---------|
| `Saleae::Graph::DisaggregatorNode` | Signal disaggregation node |
| `Saleae::Graph::DisaggregatorSettingsData` | Disaggregator config |
| `Saleae::Graph::DisaggregatorActions::ConfigureGlitchFilters` | Glitch filter |
| `Saleae::Graph::AnalogDownsamplerActions::SetAnalogDownsampleRate` | Downsample |
| `Saleae::Graph::ExportNode` | Data export node |
| `Saleae::Graph::ExportActions::ExportData` | Export data |
| `Saleae::Graph::StoreActions::SetDiscardData` | Control data retention |
| `Saleae::Graph::NodeActions::StartDeserialization` | Load saved data |
| `Saleae::Graph::NodeActions::StartSerialization` | Save data |
| `Saleae::Graph::NodeActions::StartExecution` | Start node execution |

### Frame Database / Search

| Type | Purpose |
|------|---------|
| `Saleae::Graph::FrameDBQueryNode` | Query node for frame DB |
| `Saleae::Graph::FrameDBWriterNode` | Write to frame DB |
| `Saleae::Graph::FrameDBWriterNodeActions::OpenDatabase` | Open DB |
| `Saleae::Graph::FrameDBQueryNodeActions::LoadTestDatabase` | Load test DB |
| `Saleae::Graph::FrameDBQueryNodeActions::WriteToTestDatabase` | Write test |
| `Saleae::Graph::FrameDBQueryNodeActions::FinishTestDatabase` | Finish test |
| `Saleae::Graph::FrameSearchQueryData` | Search query |
| `Saleae::Graph::FrameSearchResultData` | Search result |
| `Saleae::Graph::FrameSearchRequestFramesData` | Request frames |
| `Saleae::Graph::FrameSearchRequestLengthData` | Request length |
| `Saleae::Graph::FrameSearchLengthResultData` | Length result |
| `Saleae::Graph::FrameSearchSeekData` | Seek in results |
| `Saleae::Graph::FrameSearchDestroyQueryData` | Destroy query |
| `Saleae::Graph::FrameSearchEndReachedData` | End of results |
| `Saleae::Graph::FrameSearchSingleFrameRequest` | Single frame query |
| `Saleae::Graph::FrameSearchSingleFrameResponse` | Single frame result |

### Graph State

| Type | Purpose |
|------|---------|
| `Saleae::GraphState` | Full graph state snapshot |
| `Saleae::Graph::GraphActionData` | Batched graph actions |

## Node ID Constants

The graph uses fixed node IDs for well-known nodes:

| Constant | Value | Purpose |
|----------|-------|---------|
| DISAGGREGATOR_NODE_ID | 1 | Signal disaggregation |
| DIGITAL_TRIGGER_NODE_ID | 2 | Digital trigger |
| ANALYZER_TRIGGER_NODE_ID | 3 | Analyzer trigger |
| LOGIC_DEVICE_NODE | 4 | Device control |
| ANALYZER_MANAGER_NODE | 1 | Analyzer management |
| DEVICE_LIST_NODE | 1 | Device list |

## Broadcast Events (native -> JS, unsolicited)

| Event | Purpose |
|-------|---------|
| `connected` | Backend connection established |
| `disconnected` | Backend disconnected |
| `workerError` | Error in worker/backend |
| `digitalTriggerEvent` | Digital trigger fired |
| `analogTriggerEvent` | Analog trigger fired |
| `analyzerTriggerEvent` | Analyzer trigger fired |
| `analyticsEvent` | Telemetry from backend |
| `subscriptionUpdate` | State change notification |
| `pipeBroadcast` | Data from pipe subscription |
| `framedbQueryResponse` | Frame DB query result |
| `analyzerExportResponse` | Analyzer export progress |

## Automation gRPC API (port 10430)

A separate gRPC service (`saleae.automation`) is available when the
`--automation` flag is passed. Default host: `127.0.0.1`, port: `10430`.

This is the public Saleae automation API (used by the Python SDK). It uses
protobuf-serialized messages (compiled into the renderer bundle as google-
protobuf JS classes).

### Automation Message Types (proto.saleae.automation.*)

**Device/App:**
- `GetAppInfoRequest` / `GetAppInfoReply`
- `GetDevicesRequest` / `GetDevicesReply`
- `Device` (deviceId, deviceType, isSimulation)
- `DeviceType` (enum)
- `AppInfo` (apiVersion, applicationVersion, launchPid)
- `Version` (major, minor, patch)

**Capture Control:**
- `StartCaptureRequest` / `StartCaptureReply`
- `StopCaptureRequest` / `StopCaptureReply`
- `WaitCaptureRequest` / `WaitCaptureReply`
- `CloseCaptureRequest` / `CloseCaptureReply`
- `LoadCaptureRequest` / `LoadCaptureReply`
- `SaveCaptureRequest` / `SaveCaptureReply`
- `CaptureInfo` (captureId)
- `CaptureConfiguration`
  - `ManualCaptureMode` (trimDataSeconds)
  - `TimedCaptureMode` (durationSeconds, trimDataSeconds)
  - `DigitalTriggerCaptureMode` (triggerChannelIndex, triggerType, ...)
  - bufferSizeMegabytes

**Device Configuration:**
- `LogicDeviceConfiguration`
  - digitalSampleRate, analogSampleRate
  - digitalThresholdVolts
  - LogicChannels (digitalChannels[], analogChannels[])
  - GlitchFilterEntry[] (channelIndex, pulseWidthSeconds)

**Analyzers:**
- `AddAnalyzerRequest` / `AddAnalyzerReply`
- `RemoveAnalyzerRequest` / `RemoveAnalyzerReply`
- `AddHighLevelAnalyzerRequest` / `AddHighLevelAnalyzerReply`
- `RemoveHighLevelAnalyzerRequest` / `RemoveHighLevelAnalyzerReply`
- `LegacyExportAnalyzerRequest` / `LegacyExportAnalyzerReply`
- `AnalyzerSettingValue` (int, stringValue, doubleValue, boolValue)
- `HighLevelAnalyzerSettingValue` (numberValue, stringValue)

**Data Export:**
- `ExportRawDataCsvRequest` / `ExportRawDataCsvReply`
- `ExportRawDataBinaryRequest` / `ExportRawDataBinaryReply`
- `ExportDataTableCsvRequest` / `ExportDataTableCsvReply`
- `DataTableFilter` (query, columns[])
- `DataTableAnalyzerConfiguration` (analyzerId, radixType)
- `LogicChannels` (digitalChannels[], analogChannels[])
- `RadixType` (enum)

**Enums:**
- `ChannelType` -- analog/digital
- `DeviceType` -- device type enum
- `DigitalTriggerType` -- trigger type enum
- `DigitalTriggerLinkedChannelState` -- trigger state
- `ErrorCode` -- error codes
- `RadixType` -- number display format

## Native Addon Details

### graph-interface.node

- **Binary**: `bin/linux-x64-106/graph-interface.node` (108KB)
- **Build**: node-addon-api (N-API), node-gyp
- **SONAME**: `saleae_graph_interface.node`
- **Compiled with**: GCC 7.5.0 (Ubuntu 18.04)
- **Links**: libstdc++, libgcc_s, libpthread, libc
- **Key symbols**:
  - `_register_saleae_graph_interface` -- module registration
  - `Instance::Send` / `Instance::TryRecv` -- message passing
  - `SendAsyncWorker` -- async N-API worker for send operations
  - `NotifyMessage` / `NotifyMessageThreadsafe` -- callback from native thread
  - `LogCallback` / `LogCallbackNoop` -- log forwarding
- **dlopen pattern**: loads libgraph_server_shared.so at runtime, resolves
  `CreateGraphServer`, `DestroyGraphServer`, `DriveRequest`, `PopResponse`,
  `FreeResponseBuffer`, `SetLogFileName`, `SetLogCallback`, `FlushLog`
- **Error strings**: "failed to load graph_server_shared: ",
  "missing symbols in graph_server_shared"

### libgraph_server_shared.so C API

The native library exposes this minimal C API (resolved via dlsym):

```c
// Create graph server, optionally listening on a port
// Returns opaque pointer to server instance
// port_ptr receives the assigned port number (for WebSocket mode)
void* CreateGraphServer(int* port_ptr, int param, bool dev_mode);

// Destroy graph server instance
void DestroyGraphServer(void* server);

// Send a JSON request string to the server
// Used by the N-API addon's SendAsyncWorker
void DriveRequest(/* request string */);

// Pop a JSON response from the response queue
// Returns response string or null
char* PopResponse(/* ... */);

// Free response buffer returned by PopResponse
void FreeResponseBuffer(char* buffer);

// Set log output file
void SetLogFileName(const char* filename);

// Set log callback function
void SetLogCallback(/* callback ptr */);

// Flush log buffers
void FlushLog(void);
```

Note: Exact function signatures are inferred from the ffi-napi declarations
and N-API wrapper behavior. The DriveRequest/PopResponse signatures need
confirmation from Ghidra analysis of the .so.

## Key Implications for RE

1. **The IPC is JSON, not protobuf** -- The internal protocol between Electron
   and libgraph_server_shared.so uses JSON with `Saleae::*` namespaced type
   strings. The protobuf is only used for the external automation gRPC API.

2. **The native library is a graph processor** -- It implements a directed
   acyclic graph of processing nodes. The JS side constructs the graph
   (add nodes, pipes, connections) and sends actions to nodes.

3. **Device operations are graph actions** -- Starting a capture, configuring
   channels, etc. are all routed as actions to the `LogicDeviceNode` (node 4)
   through the graph action system.

4. **The C API is simple** -- Only 8 functions. The complexity is in the JSON
   message protocol, not the transport. `DriveRequest` + `PopResponse` is
   essentially a producer/consumer queue with JSON strings.

5. **USB traffic is hidden behind the graph** -- To reverse engineer the USB
   protocol, we need to understand what happens INSIDE `LogicDeviceNode` when
   it receives routed actions. The JS side only sees high-level abstractions.

6. **The automation gRPC API is a secondary path** -- It reuses the same
   backend but through a different entry point. The proto definitions give us
   the public API vocabulary, which maps to internal graph operations.

## Files Analyzed

- `re/extracted-asar/package.json` -- dependency list
- `re/extracted-asar/dist/main.js` -- Electron main process
- `re/extracted-asar/dist/logic/bundle.js` -- Renderer bundle (9.6MB minified)
- `re/extracted-asar/dist/logic/6a4322176d988210675b.worker.js` -- Web Worker
- `re/extracted-asar/node_modules/@saleae/graph-interface/` -- N-API addon
  - `dist/instance.js` -- Instance class (send/recv wrapper)
  - `dist/instance.d.ts` -- TypeScript declarations
  - `dist/util.d.ts` -- Utils interface (needsOutOfProcessBackend, etc.)
  - `dist/fs.d.ts` -- FsTools interface (file locking)
  - `bin/linux-x64-106/graph-interface.node` -- Native addon binary
