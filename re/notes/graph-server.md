# Graph Server: Native-Side Architecture (libgraph_server_shared.so)

## Summary

This document covers the **native C++ side** of the graph server inside
`libgraph_server_shared.so`. Prior work (ipc-analysis.md, graph-interface-analysis.md)
documented the JS side and the C API bridge. This document covers the C++ class
architecture, request dispatch, data flow internals, and the WebSocket automation
path, all derived from string analysis, symbol demangling, and cross-referencing
with the Electron bundle.js.

## Source Paths (from assertions)

```
/workspace/monorepo/graph-io/server/graph_server/src/graph_server.cpp
/workspace/monorepo/graph-io/server/graph_server/src/request_watcher.cpp
/workspace/monorepo/graph-io/core/named_pipe/src/named_pipe_unix.cpp
```

Build system paths (from embedded file paths):
```
/workspace/monorepo/build/graph-io/linux-x64/release/vcpkg_installed/
    x64-linux-libcxx-release/share/rapidjson/../../include/boost/asio/...
```

This tells us:
- The project is called `graph-io` in a monorepo
- Built with vcpkg for package management
- Uses libc++ (LLVM's C++ standard library), not libstdc++
- RapidJSON is installed as a vcpkg dependency alongside Boost.Asio

## Exported C API (ELF symbol table)

| Symbol | ELF VA | Purpose |
|--------|--------|---------|
| `CreateGraphServer` | 0x1a01780 | Create server instance, optionally start WebSocket |
| `DestroyGraphServer` | 0x1a01930 | Destroy server instance |

These are the only two exported symbols. The other 6 functions (`DriveRequest`,
`PopResponse`, `FreeResponseBuffer`, `SetLogFileName`, `SetLogCallback`, `FlushLog`)
are also exported but may not appear in `nm -D` output -- they are resolved via
`dlsym()` at runtime (confirmed in graph-interface-analysis.md).

## Key C++ Classes

### Saleae::GraphServer

The main server class. Assertion strings reveal:

```cpp
namespace Saleae {
class GraphServer {
public:
    // Constructor: optional port, queue_size (always 100), dev_mode
    GraphServer(std::optional<int> port, int queue_size,
                std::function<void(GraphServer&)> on_ready);

    // Entry point for all JSON requests
    void ProcessRequest(const uint8_t* data, size_t length);

    // Send JSON response back to client
    void EnqueueResponse(rapidjson::Document& doc,
                         SerializeData& serialize_data,
                         RemoteClient::WebsocketEndpoint endpoint,
                         bool broadcast);

    // Handle graph initialization
    void HandleGraphInitRequest(const GraphInitRequestData& data);

    ~GraphServer();
};
}
```

The constructor's lambda captures (from demangled symbols) show it takes:
1. `std::optional<int>` -- port number (nullopt for in-process mode)
2. `int` -- parameter (always 100)
3. `std::function<void(GraphServer&)>` -- callback when server is ready

### Saleae::Detail::RemoteClientInternals

The WebSocket server implementation. Uses the `Simple-WebSocket-Server` library
(`SimpleWeb::SocketServer<boost::asio::basic_stream_socket<tcp>>`).

Constructor takes callbacks for:
1. `void(const uint8_t*, size_t)` -- message received
2. `void()` -- connected
3. `void()` -- disconnected
4. `int` -- port number

The WebSocket endpoint path is `/saleae` (from bundle.js: `ws://localhost:{port}/saleae`).

### Saleae::Redux::ReduxStore

The server uses a Redux-like state management pattern internally:

- `Saleae::Redux::ReduxStore::Dispatcher` -- action dispatcher
- `Saleae::Redux::Detail::ReduxStoreInternals::ExternalDispatcher`
- `Saleae::Redux::Detail::ReduxStoreInternals::InternalDispatcher`
- `Saleae::Redux::Detail::ReduxStoreInternals::DispatchAsync`
- `Saleae::Redux::Action` -- base action type

This mirrors the Redux pattern on the JS side. Actions dispatched from JS
are routed through this system to the appropriate handlers.

## Message Protocol (JSON Wire Format)

### Outgoing Request (JS -> native)

The GraphClient in the worker sends structured JSON. The top-level envelope
has a `messageType` field:

```json
{
    "messageType": "submitActions",
    "sessionId": "<uuid>",
    "actions": [
        {"type": "Saleae::Graph::GraphActions::RouteAction", "nodeId": 4, "action": {
            "type": "Saleae::Graph::LogicDevice::StartCapture",
            ...
        }}
    ],
    "runInParallel": false
}
```

All 50 known messageType values (see `doc/message-type-catalog.md` for
complete details including request/response fields):

| messageType | Purpose |
|-------------|---------|
| `connect` | Initialize backend connection |
| `disconnect` | Tear down connection |
| `createGraph` | Create new graph session |
| `destroyGraph` | Destroy graph session |
| `getState` | Get current graph state |
| `submitActions` | Submit batch of graph actions |
| `sendGraphActions` | Alternative action submission |
| `subscribeToUpdates` | Subscribe to state changes |
| `unsubscribeFromUpdates` | Unsubscribe |
| `subscribeToPipeBroadcast` | Subscribe to pipe data |
| `sendToPipe` | Send data to a pipe |
| `addLogicDeviceNode` | Add device control node |
| `sendLogicDeviceRequest` | Device operation |
| `addSimulationDevice` | Add simulation device |
| `removeSimulationDevice` | Remove simulation device |
| `sendDigitalTriggerSettings` | Configure digital trigger |
| `sendAnalyzerTriggerSettings` | Configure analyzer trigger |
| `sendAnalogTriggerSettings` | Configure analog trigger |
| `disableAnalogTrigger` | Disable analog trigger |
| `setGlitchFilter` | Configure glitch filter |
| `loadAnalyzersList` | Load analyzer plugins |
| `createAnalyzer` | Create analyzer instance |
| `startAnalyzer` | Start analysis |
| `stopAnalyzer` | Stop analysis |
| `updateAnalyzerSettings` | Update analyzer config |
| `renderAnalyzer` | Request analyzer render |
| `createHighLevelAnalyzer` | Create HLA |
| `connectHighLevelAnalyzer` | Connect HLA to analyzer |
| `disconnectHighLevelAnalyzer` | Disconnect HLA |
| `renderHighLevelAnalyzer` | Request HLA render |
| `setHighLevelAnalyzerSettings` | Configure HLA |
| `createExportNode` | Create data export node |
| `deleteNode` | Delete a processing node |
| `createFrameDBQueryNode` | Create frame DB query |
| `createFrameDBQueryNodeForTest` | Create test query node |
| `createFrameDBWriterNode` | Create frame DB writer |
| `sendQueryNodeMessage` | Send frame query message |
| `findResultInQueryNode` | Find result in query |
| `writeToQueryNodeTestDb` | Write test DB entry |
| `finishQueryNodeTestDb` | Finish test DB |
| `createAnalyzerExportNode` | Create analyzer export |
| `connectAnalyzerExportNode` | Connect export node |
| `disconnectAnalyzerExportNode` | Disconnect export |
| `sendAnalyzerExportMessage` | Export control message |
| `connectAnalyzerTrigger` | Connect analyzer trigger |
| `disconnectAnalyzerTrigger` | Disconnect trigger |
| `flushLogs` | Flush log buffers |
| `startTracing` | Start performance tracing |
| `stopTracing` | Stop tracing |
| `crashBackend` | Force crash (debugging) |

### Response (native -> JS)

Responses are wrapped in an envelope:

```json
{
    "type": "response",
    "contents": {
        "id": 42,
        "type": "Saleae::Graph::LogicDevice::StartCaptureResponse",
        "meta": {
            "sessionId": "...",
            "destination": "Graph",
            "portName": "...",
            "portIndex": 0
        },
        ...payload fields...
    }
}
```

Error envelope:
```json
{
    "type": "error",
    "contents": {
        "id": 42,
        "type": "...",
        "meta": {...},
        "error": "..."
    }
}
```

Required fields in contents: `id`, `type`, `meta`.

### Action Types Used in Messages

Each action/request/response is identified by its fully-qualified C++ type name.
The `type` field uses the exact C++ namespace path.

## Serialization

The native side uses **RapidJSON** for JSON serialization/deserialization.

The `Saleae::SerializeData` type is used alongside `rapidjson::Document` for
response construction. There is a template converter system:

```cpp
template<typename T>
class Saleae::SerializeConverter;

// Specializations exist for:
Saleae::SerializeConverter<Saleae::Graph::RenderRequestData>
Saleae::SerializeConverter<Saleae::Graph::MeasurementRequestData>
Saleae::SerializeConverter<Saleae::Graph::AnalyzerRenderRequestData>
Saleae::SerializeConverter<Saleae::Graph::RangeMeasurementRequestData>
```

There is also a `Saleae::ConvertableObject` base class used in the request
handler registration (all responses are `shared_ptr<const ConvertableObject>`).

## Request Handler Registration

The native graph kernel uses a registration system:

```cpp
namespace Saleae::Graph {
class ConvenienceKernel {
    template<typename Node, typename Request>
    void RegisterRequestHandler(Node* node,
        void (Node::*handler)(const Request&));

    template<typename Node, typename Request, typename Response>
    void RegisterRequestHandler(Node* node,
        shared_ptr<const Response> (Node::*handler)(const Request&));

    template<typename DataType, typename Node>
    void RegisterPipeHandler(Node* node,
        void (Node::*handler)(const string&, uint64_t, shared_ptr<const DataType>),
        PipeHandlerOptions options);
};
}
```

### Request Handlers (Node | Request -> Response)

These are **request/response** style handlers -- the JS side sends a request
and waits for a response.

| Node | Request | Response |
|------|---------|----------|
| LogicDeviceNode | AcquireLogicDeviceRequest | AcquireLogicDeviceResult |
| LogicDeviceNode | GetDefaultLogicDeviceConfigurationRequest | GetDefaultLogicDeviceConfigurationResult |
| LogicDeviceNode | LogicDevice::EnableChannels | SettingResponse |
| LogicDeviceNode | LogicDevice::SetSampleRate | SettingResponse |
| LogicDeviceNode | LogicDevice::SetDigitalVoltageThreshold | SettingResponse |
| LogicDeviceNode | LogicDevice::SetLed | SettingResponse |
| LogicDeviceNode | LogicDevice::SetAdcFullScaleGain | (void) |
| LogicDeviceNode | LogicDevice::StartCapture | StartCaptureResponse |
| LogicDeviceNode | LogicDevice::StopCapture | StopCaptureResponse |
| LogicDeviceNode | ReleaseDeviceRequest | (void) |
| DeviceListNode | AddSimulationDevice | (void) |
| DeviceListNode | RemoveSimulationDevice | (void) |
| DeviceListNode | SetOverrideUsb2PowerLimit | (void) |
| AnalogCalibrationFilter | AnalogCalibrationFilterActions::SetCalibration | (void) |
| AnalogCalibrationFilter | AnalogCalibrationFilterActions::SetChannel | (void) |
| AnalogStore | NodeActions::StartSerialization | (void) |
| AnalogStore | NodeActions::StartDeserialization | (void) |
| AnalogStore | StoreActions::SetDiscardData | (void) |
| DigitalStore | NodeActions::StartSerialization | (void) |
| DigitalStore | NodeActions::StartDeserialization | (void) |
| DigitalStore | StoreActions::SetDiscardData | (void) |
| WaveformStoreNode | NodeActions::StartSerialization | (void) |
| WaveformStoreNode | NodeActions::StartDeserialization | (void) |
| WaveformStoreNode | StoreActions::SetDiscardData | (void) |
| WaveformStoreNode | StoreActions::ExportWaveformCSV | (void) |
| AnalyzerNode | LegacyAnalyzerActions::LoadAnalyzers | (void) |
| AnalyzerNode | LegacyAnalyzerActions::SelectAnalyzer | (void) |
| AnalyzerNode | LegacyAnalyzerActions::SetAnalyzerSettings | (void) |
| AnalyzerNode | LegacyAnalyzerActions::StartAnalysis | (void) |
| AnalyzerNode | LegacyAnalyzerActions::StopAnalysis | (void) |
| AnalyzerNode | LegacyAnalyzerActions::StartExport | (void) |
| AnalyzerNode | LegacyAnalyzerActions::StopExport | (void) |
| AnalyzerNode | LegacyAnalyzerActions::OpenDatabase | (void) |
| AnalyzerNode | StoreActions::SetDiscardData | (void) |
| AnalyzerTriggerNode | NewAnalyzerRunStarted | (void) |
| AnalyzerTriggerNode | WorkRequest | (void) |
| HlaNode | HlaNodeActions::LoadAnalyzer | (void) |
| HlaNode | HlaNodeActions::SetSettings | (void) |
| HlaNode | WorkRequest | (void) |
| DisaggregatorNode | DisaggregatorActions::ConfigureGlitchFilters | (void) |
| ExportNode | ExportActions::ExportData | (void) |
| FrameDBQueryNode | FrameDBQueryNodeActions::LoadTestDatabase | (void) |
| FrameDBQueryNode | FrameDBQueryNodeActions::WriteToTestDatabase | (void) |
| FrameDBQueryNode | FrameDBQueryNodeActions::FinishTestDatabase | (void) |
| FrameDBWriterNode | WorkRequest | (void) |
| MeasurementNode | ScanMeasurersRequest | ScanMeasurersResult |
| MeasurementNode | SetMeasurementConfigurationRequest | (void) |
| MsoDeviceNode | AcquireMsoDeviceRequest | AcquireMsoDeviceResult |
| MsoDeviceNode | AddSimulatedInstrumentRequest | AddSimulatedInstrumentResult |
| MsoDeviceNode | RemoveSimulatedInstrumentRequest | (void) |
| MsoDeviceNode | ReleaseDeviceRequest | (void) |
| MsoDeviceNode | TestErrorRequest | (void) |
| MsoLogicAnalyzerInstrumentNode | ConnectToLogicAnalyzerInstrument | LogicAnalyzerInstrumentResult |
| MsoLogicAnalyzerInstrumentNode | ConfigureLogicAnalyzer | LogicAnalyzerInstrumentResult |
| MsoLogicAnalyzerInstrumentNode | GetLogicAnalyzerConfiguration | LogicAnalyzerInstrumentResult |
| MsoOscilloscopeInstrumentNode | AcquireOscilloscopeInstrumentRequest | Result |
| MsoOscilloscopeInstrumentNode | SetConfigurationRequest | SetConfigurationResult |
| MsoOscilloscopeInstrumentNode | SetChannelSimulatorConfigurationRequest | SetConfigurationResult |
| MsoOscilloscopeInstrumentNode | StartAutoScaleRequest | (void) |

### Pipe Handlers (Node | DataType)

These are **streaming data** handlers -- data flows continuously through pipes
between nodes. This is how captured sample data moves through the processing graph.

| Node | Data Type |
|------|-----------|
| AnalogStore | AnalogData |
| AnalogStore | PipeDone |
| AnalogStore | SamplingConfigurationData |
| DigitalStore | DigitalData |
| DigitalStore | PipeDone |
| DigitalStore | SamplingConfigurationData |
| AnalogCalibrationFilter | SamplingConfigurationData |
| AnalogCalibrationFilter | VectorData |
| AnalogTriggerNode | AnalogData |
| AnalogTriggerNode | AnalogTriggerSettingsData |
| AnalogTriggerNode | SamplingConfigurationData |
| DigitalTriggerNode | DigitalData |
| DigitalTriggerNode | DigitalTriggerSettingsData |
| DigitalTriggerNode | SamplingConfigurationData |
| AnalogRenderer | RenderRequestData |
| DigitalRenderer | RenderRequestData |
| AnalyzerNode | AnalyzerRenderRequestData |
| AnalyzerTriggerNode | AnalyzerTriggerSettingsData |
| HlaNode | AnalyzerRenderRequestData |
| DisaggregatorNode | CaptureCompleteData |
| DisaggregatorNode | DisaggregatorSettingsData |
| DisaggregatorNode | OwnedSpan<uint8_t> |
| MeasurementNode | MeasureInstantenousRequest |
| MeasurementNode | MeasureRangeRequest |
| MeasurementNode | MeasureRangeAllRequest |
| RangeMeasurer | RangeMeasurementRequestData |
| RenderingManagerNode | AnalogRenderData |
| RenderingManagerNode | DigitalRenderData |
| RenderingManagerNode | AnalyzerRenderResultData |
| RenderingManagerNode | AnalyzerTriggerResultData |
| RenderingManagerNode | MeasurementsResultData |
| RenderingManagerNode | ConfigureRenders |
| RenderingManagerNode | SetRenderViewState |
| RenderingManagerNode | ResizeViewState |
| RenderingManagerNode | MoveViewState |
| RenderingManagerNode | MoveViewStateToRange |
| RenderingManagerNode | PanTimeViewState |
| RenderingManagerNode | PanPixelsViewState |
| RenderingManagerNode | ZoomFactorViewState |
| RenderingManagerNode | AnimateViewStateZoom |
| RenderingManagerNode | AnimateViewStateZoomAbsolute |
| RenderingManagerNode | AnimateViewStatePanPixels |
| RenderingManagerNode | AnimateViewStatePanDecay |
| RenderingManagerNode | AnimateViewStateToTime |
| RenderingManagerNode | AnimateViewStateToRange |
| RenderingManagerNode | AnimateViewStatePanIndefinite |
| RenderingManagerNode | AnimateViewStateCancel |
| RenderingManagerNode | SetFollowViewMode |
| RenderingManagerNode | SetNavigationViewMode |
| RenderingManagerNode | SetCurrentTrigger |
| RenderingManagerNode | NavigateTrigger |
| RenderingManagerNode | FrameStartEvent |
| RenderingManagerNode | WaveformRenderResult |
| RenderingManagerNode | WaveformSummaryResult |
| WaveformRendererNode | WaveformRenderRequest |
| WaveformSummaryNode | WaveformRenderRequest |
| WaveformStoreNode | VectorRawWaveformData |
| (function) | MeasurementRequestData |
| (function) | PipeDone |
| (function) | SamplingConfigurationData |
| (function) | VectorData |

## Processing Graph Architecture

The native library implements a directed acyclic graph (DAG) of processing nodes.
Data flows through pipes between nodes. The JS side constructs the graph topology
using `GraphActions::AddNode`, `AddPipe`, `ConnectApi`, etc.

### Node Types (all in Saleae::Graph namespace)

| Node Class | Purpose |
|------------|---------|
| DeviceListNode | Lists connected/simulated devices |
| LogicDeviceNode | Controls a Logic device (capture, config) |
| AnalogStore | Stores captured analog data |
| DigitalStore | Stores captured digital data |
| WaveformStoreNode | Stores waveform data |
| AnalogCalibrationFilter | Applies calibration to analog data |
| AnalogDownsamplerNode | Downsamples analog data |
| AnalogCreatorNode | Creates analog data (simulation?) |
| DigitalCreatorNode | Creates digital data (simulation?) |
| DisaggregatorNode | Splits interleaved USB data into channels |
| AnalogRenderer | Renders analog waveforms for display |
| DigitalRenderer | Renders digital waveforms for display |
| RenderingManagerNode | Manages rendering viewport state |
| WaveformRendererNode | Renders waveforms |
| WaveformSummaryNode | Generates waveform summaries |
| AnalogTriggerNode | Analog trigger detection |
| DigitalTriggerNode | Digital trigger detection |
| AnalyzerTriggerNode | Analyzer-based trigger |
| AnalyzerNode | Protocol analyzer (SPI, I2C, etc.) |
| HlaNode | High-level analyzer (Python-based) |
| ExportNode | Data export |
| AnalyzerExportNode | Analyzer export |
| FrameDBQueryNode | Frame database query |
| FrameDBWriterNode | Frame database writer |
| MeasurementNode | Signal measurements |
| RangeMeasurer | Range-based measurements |
| MsoDeviceNode | MSO device control (out of scope) |
| MsoLogicAnalyzerInstrumentNode | MSO LA instrument (out of scope) |
| MsoOscilloscopeInstrumentNode | MSO scope instrument (out of scope) |

### Data Flow During Capture

Based on pipe handler registrations, the data flow during a capture is:

```
LogicDeviceNode (USB data)
  |
  v
DisaggregatorNode (splits interleaved data into per-channel streams)
  |
  +---> DigitalStore (stores digital samples)
  |       |
  |       +---> DigitalRenderer (for display)
  |       +---> DigitalTriggerNode (trigger detection)
  |       +---> AnalyzerNode (protocol analysis)
  |               +---> HlaNode (high-level analysis)
  |               +---> FrameDBWriterNode (index frames)
  |
  +---> AnalogCalibrationFilter (apply calibration)
          |
          v
        AnalogStore (stores calibrated analog samples)
          |
          +---> AnalogRenderer (for display)
          +---> AnalogTriggerNode (trigger detection)
          +---> WaveformStoreNode (waveform data)
                  +---> WaveformRendererNode (for display)
```

## WebSocket Automation (Internal)

Inside `libgraph_server_shared.so`, there is a WebSocket server for remote/
out-of-process operation. This uses the `Simple-WebSocket-Server` header-only
library built on Boost.Asio.

Key details:
- Library: `SimpleWeb::SocketServer<boost::asio::basic_stream_socket<tcp>>`
- Async I/O: Boost.Asio with epoll reactor
- WebSocket path: `/saleae`
- Port: assigned dynamically, returned via `CreateGraphServer(port_ptr, ...)`
- Messages: Same JSON format as direct mode
- Error: "Boost ASIO asio.misc:eof error caught, caused by suspend, ignoring"

This WebSocket server is the **internal** transport (for macOS out-of-process mode
and for `--useExistingGraph` debugging). It is NOT the automation API.

## gRPC Automation API (External)

The automation API is a **separate gRPC server** running in the Electron renderer
process (not in `libgraph_server_shared.so`). It is implemented in JavaScript
using `@grpc/grpc-js`.

- Package: `saleae.automation`
- Service: `saleae.automation.Manager`
- Default host: `127.0.0.1`
- Default port: `10430`
- Credentials: Insecure (no TLS)
- Flag: `--automation` (disabled by default)

### gRPC Methods

| Method | Path |
|--------|------|
| GetAppInfo | /saleae.automation.Manager/GetAppInfo |
| GetDevices | /saleae.automation.Manager/GetDevices |
| StartCapture | /saleae.automation.Manager/StartCapture |
| StopCapture | /saleae.automation.Manager/StopCapture |
| WaitCapture | /saleae.automation.Manager/WaitCapture |
| LoadCapture | /saleae.automation.Manager/LoadCapture |
| SaveCapture | /saleae.automation.Manager/SaveCapture |
| CloseCapture | /saleae.automation.Manager/CloseCapture |
| AddAnalyzer | /saleae.automation.Manager/AddAnalyzer |
| RemoveAnalyzer | /saleae.automation.Manager/RemoveAnalyzer |
| AddHighLevelAnalyzer | /saleae.automation.Manager/AddHighLevelAnalyzer |
| RemoveHighLevelAnalyzer | /saleae.automation.Manager/RemoveHighLevelAnalyzer |
| ExportRawDataCsv | /saleae.automation.Manager/ExportRawDataCsv |
| ExportRawDataBinary | /saleae.automation.Manager/ExportRawDataBinary |
| ExportDataTableCsv | /saleae.automation.Manager/ExportDataTableCsv |
| LegacyExportAnalyzer | /saleae.automation.Manager/LegacyExportAnalyzer |

The gRPC handler wrappers in the renderer translate these into the same
`graphClient.postMessage(...)` calls that the UI uses. So the automation API
is a thin gRPC-to-JSON bridge layered on top of the same internal protocol.

## Named Pipes

The binary contains a named pipe subsystem for streaming data:

```
Source: /workspace/monorepo/graph-io/core/named_pipe/src/named_pipe_unix.cpp
```

Strings:
- "failed to create named pipe"
- "Failed to create named pipe. Terminal mode will be disabled for this capture."
- "named pipe connected"
- "named pipe not connected"
- "A pipe can only have one reader"
- "A pipe in mConnectedOutputs should always exist in mExternalPipeDock"
- "unable to close named pipe handle, error: "
- "unable to delete named pipe, error: "
- "unknown error occured while trying to open named pipe for writing: "

Named pipes are used for streaming capture data (analog/digital samples, analyzer
frames) between processing nodes, and for terminal output in real-time capture mode.

## State Types (State2)

The Redux-like state system uses `State2` versioned types:

| State | Node |
|-------|------|
| AnalogStoreState2 | AnalogStore |
| DigitalStoreState2 | DigitalStore |
| DigitalTriggerState2 | DigitalTriggerNode |
| AnalogTriggerNodeState2 | AnalogTriggerNode |
| AnalyzerTriggerNodeState2 | AnalyzerTriggerNode |
| DisaggregatorNodeState2 | DisaggregatorNode |
| FrameDBWriterNodeState2 | FrameDBWriterNode |
| DeviceListState | DeviceListNode |
| GraphState | Graph |
| NodeState | Node (base) |
| ExportNodeState | ExportNode |
| LegacyAnalyzerState | AnalyzerNode |
| HlaNodeState | HlaNode |
| WaveformStoreState | WaveformStoreNode |
| MsoOscilloscopeState | MsoOscilloscopeInstrumentNode |

The `State2` suffix suggests a versioned state format (v2).

## Implications for RE

### What we now know

1. **The IPC protocol is fully mapped** -- 50 messageType values, 60+ request
   handler registrations, 60+ pipe handler registrations. The protocol surface
   is complete. See `doc/message-type-catalog.md` for the full catalog with
   request/response field documentation.

2. **The gRPC automation API is separate from the internal IPC** -- The automation
   API runs as a gRPC server in the Electron renderer process, not in the native
   library. It translates gRPC calls to the same JSON messages.

3. **The native library is a graph processor** -- a DAG of ~30 node types with
   typed pipe connections. Data flows from USB device through disaggregator,
   through stores, through renderers/analyzers.

4. **The capture data path is clear** -- USB -> DisaggregatorNode -> Digital/Analog
   Store -> Renderers/Triggers/Analyzers. Calibration is applied by
   AnalogCalibrationFilter between disaggregator and analog store.

5. **No direct USB access from the protocol layer** -- USB operations are entirely
   encapsulated inside LogicDeviceNode. The IPC protocol only exposes
   high-level operations (StartCapture, StopCapture, EnableChannels, etc.).

### What remains unknown

1. **Exact JSON field names** for each message type (would require Ghidra
   decompilation of SerializeConverter specializations)
2. **Binary data encoding** for pipe data (AnalogData, DigitalData, OwnedSpan)
3. **Session management internals** -- how multiple sessions are isolated
4. **Graph topology construction** -- exact sequence of AddNode/AddPipe calls
   for a typical capture session

### Relevance to USB RE

This protocol analysis is **out of scope** for the core USB RE work (Slices 2-6
in the PRD). The IPC protocol is the Electron-to-native bridge. What matters
for USB RE is what happens *inside* LogicDeviceNode, which is already being
investigated through libusb call site analysis.

However, understanding the IPC protocol is valuable for:
- Verifying our USB RE against what the UI actually sends
- Understanding how calibration data flows (AnalogCalibrationFilter)
- Understanding the automation API for scripting device interactions

## Files Analyzed

- Binary: `libgraph_server_shared.so` (string analysis, symbol demangling)
- Electron: `app.asar` -> `dist/main.js`, `dist/logic/bundle.js`
- Addon: `graph-interface.node` (from graph-interface-analysis.md)
- Cross-ref: `re/notes/ipc-analysis.md`, `re/notes/graph-interface-analysis.md`
