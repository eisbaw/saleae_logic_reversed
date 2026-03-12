# GraphServer Message Type Catalog

Complete catalog of all message types in the Saleae Logic 2 IPC protocol,
derived from JavaScript bundle.js analysis and Ghidra decompilation of
`libgraph_server_shared.so`.

## Architecture Overview

Message dispatch happens at two levels:

1. **JavaScript Worker** (`6a4322176d988210675b.worker.js`): The Electron
   renderer sends JSON messages with a `messageType` field to a Web Worker.
   The worker's `messageHandlers` object maps each `messageType` to a handler
   that translates it into a C++ request with a `type` field.

2. **Native RemoteRequestHandler** (ELF VA `0x1a040c0`): The native library
   dispatches on the C++ `type` field using a chain of `bcmp()` string
   comparisons. There are only 6 native request types -- most `messageType`
   values are funneled through `GraphActionData` as graph actions.

### Native Dispatch Table

| C++ type | Handler (ELF VA) | Purpose |
|----------|------------------|---------|
| `Saleae::Graph::GraphActionData` | `0x1a05a00` | Main action dispatch (submitActions, sendGraphActions, and all routed device/node actions) |
| `Saleae::Graph::GraphInitRequestData` | `0x1a071b0` | Create/init graph session (createGraph). Response field: `/newSessionId` |
| `Saleae::Graph::DeleteGraphData` | `0x1a07af0` | Destroy graph session (destroyGraph) |
| `Saleae::Graph::CrashRequestData` | CauseCrash (inline) | Force crash for debugging (crashBackend). Field: `crashType` |
| `Saleae::StartTracingData` | Detached thread (`0x1a0a800`) | Start performance tracing (startTracing). Field: `/filepath` |
| `Saleae::StopTracingData` | Inline | Stop tracing (stopTracing). Response: `Saleae::SuccessResponseData` |

### Response Envelope

All responses are wrapped by `EnqueueResponse` (ELF VA `0x1a07d20`):
```json
{"type": "response", "contents": {...}}
{"type": "error", "contents": {..., "error": "..."}}
```

## Complete Message Type Catalog

### Session Management

| messageType | Category | JS -> Native Translation |
|-------------|----------|--------------------------|
| `connect` | session | Initializes backend connection. Subscribes to broadcasts (workerError, connected, disconnected, triggers). Field: `config` |
| `disconnect` | session | Tears down connection. Called on app close |
| `createGraph` | session | Creates new graph session. Sends `GraphInitRequestData`. Returns `newSessionId` |
| `destroyGraph` | session | Destroys graph session. Sends `DeleteGraphData`. Field: `sessionId` |
| `getState` | session | Sends empty `sendGraphActions` to retrieve current `graphState` |

### Device Operations (USB-relevant)

| messageType | Category | Request Fields | Response |
|-------------|----------|----------------|----------|
| `addLogicDeviceNode` | device | `sessionId` | Creates LogicDeviceNode + DisaggregatorNode, connects them with a pipe (out->in) |
| `sendLogicDeviceRequest` | device | `sessionId`, `request: {type, ...}` | Routes action to `LOGIC_DEVICE_NODE`. Response is `responses[0]` from graph |
| `addSimulationDevice` | device | `sessionId`, `request: {deviceId, deviceType}` | Routes `AddSimulationDevice` to `DEVICE_LIST_NODE` |
| `removeSimulationDevice` | device | `sessionId`, `request: {deviceId}` | Routes `RemoveSimulationDevice` to `DEVICE_LIST_NODE` |

### Device Request Sub-types (via sendLogicDeviceRequest)

These are the `request.type` values sent through `sendLogicDeviceRequest`:

| request.type | Fields | Response Type | Purpose |
|--------------|--------|---------------|---------|
| `Saleae::Graph::AcquireLogicDeviceRequest` | `deviceInstanceId` | `AcquireLogicDeviceResult` with `wasAcquired`, `state.deviceId`, `configuration` | Acquire exclusive device access |
| `Saleae::Graph::GetDefaultLogicDeviceConfigurationRequest` | `deviceInstanceId` | `GetDefaultLogicDeviceConfigurationResult` with `devicePresent`, `configuration` | Get default config without acquiring |
| `Saleae::Graph::ReleaseDeviceRequest` | (none) | (void) | Release device |
| `Saleae::Graph::LogicDevice::EnableChannels` | `channelStates: [{channel: {type, index}, enabled}]` | `SettingResponse` with `configuration: {settings, capabilities}` | Enable/disable channels |
| `Saleae::Graph::LogicDevice::SetSampleRate` | `analog: number`, `digital: number` | `SettingResponse` | Set sample rates |
| `Saleae::Graph::LogicDevice::SetDigitalVoltageThreshold` | `thresholdDescription: string` | `SettingResponse` | Set digital threshold voltage |
| `Saleae::Graph::LogicDevice::SetLed` | `ledOption: string` | `SettingResponse` | Set device LED color. Options map to `modeToColor[mode][ledType]` |
| `Saleae::Graph::LogicDevice::SetAdcFullScaleGain` | `fullScaleGain: number` | (void) | Set Hittite/HMCAD ADC gain register |
| `Saleae::Graph::LogicDevice::StartCapture` | `simulateCaptureError?: string` | `{error: string, captureStartTime: number}` | Start capture. Error values: `"NoError"`, `"ReadTimeout"` |
| `Saleae::Graph::LogicDevice::StopCapture` | (none) | (void) | Stop capture |

### SettingResponse Structure

The `SettingResponse` is returned by channel, sample rate, threshold, and LED operations:
```json
{
  "type": "Saleae::Graph::LogicDevice::SettingResponse",
  "configuration": {
    "settings": {
      "enabledChannels": [...],
      "sampleRate": {"analog": number, "digital": number}
    },
    "capabilities": {
      "channelCapabilities": [
        {"type": "Digital"|"Analog", "index": number, "capability": "Toggleable"|"Fixed"}
      ],
      "sampleRateOptions": [
        {"analog": number, "digital": number}
      ]
    }
  }
}
```

### AcquireLogicDeviceResult Structure

```json
{
  "type": "Saleae::Graph::AcquireLogicDeviceResult",
  "wasAcquired": boolean,
  "state": {
    "deviceId": string,
    "type": "DSInitializing" | "DSReady" | ...
  },
  "configuration": { ... same as SettingResponse.configuration ... }
}
```

### Device Types

```
Logic, Logic16, Logic4, Logic8, LogicPro8, LogicPro16, Mso
```

All except Mso are "legacy" device types. Calibration required for: `Logic8`, `LogicPro8`, `LogicPro16`.

### Device State Types

| State Type | Meaning |
|------------|---------|
| `DSInitializing` | Device initializing (firmware upload, FPGA programming). Has `progress: number` |
| `DSReady` | Device ready for capture |
| `MsoWaitingForWallPower` | MSO needs external power (states: `InsufficientPower`, `NotPresent`) |
| `MsoFirmwareUpdateRequired` | MSO firmware too old |
| `MsoBringup` | MSO initializing |

### Capture Configuration

| messageType | Category | Fields | Purpose |
|-------------|----------|--------|---------|
| `setGlitchFilter` | capture | `sessionId`, action routed to DisaggregatorNode | Configure glitch filter width. Action type: `DisaggregatorActions::ConfigureGlitchFilters` |
| `sendDigitalTriggerSettings` | capture | `sessionId`, action routed to DigitalTriggerNode | Configure digital trigger. Action type: `DigitalTriggerSettingsData` |
| `sendAnalyzerTriggerSettings` | capture | `sessionId`, action to AnalyzerTriggerNode | Configure analyzer trigger |
| `sendAnalogTriggerSettings` | capture | (worker-only, not in main bundle) | Configure analog trigger |
| `disableAnalogTrigger` | capture | (worker-only) | Disable analog trigger |

### Graph Actions

| messageType | Category | Fields |
|-------------|----------|--------|
| `submitActions` | graph | `sessionId`, `actions: [...]`, `runInParallel: boolean` |
| `sendGraphActions` | graph | `sessionId`, `actions: [...]` |
| `subscribeToUpdates` | graph | `sessionId`, `subscriptionRequest` |
| `unsubscribeFromUpdates` | graph | `sessionId`, `subscriptionIds: []` |
| `subscribeToPipeBroadcast` | graph | `sessionId` |
| `sendToPipe` | graph | Sends data to a named pipe |

### Graph Action Types (used within submitActions/sendGraphActions)

| Action Type | Purpose |
|-------------|---------|
| `GraphActions::AddNode` | Add processing node to graph |
| `GraphActions::RemoveNode` | Remove node |
| `GraphActions::AddPipe` | Connect two nodes with a pipe |
| `GraphActions::RemovePipe` | Disconnect pipe |
| `GraphActions::ConnectApi` | Connect API endpoint |
| `GraphActions::DisconnectApi` | Disconnect API endpoint |
| `GraphActions::RouteAction` | Route action to specific node (by nodeId) |
| `GraphActions::SetPlugins` | Set analyzer plugins |
| `GraphActions::SetPorts` | Set node ports |
| `GraphActions::Start` | Start graph execution |
| `GraphActions::Stop` | Stop graph execution |

### Analyzer Operations

| messageType | Category | Purpose |
|-------------|----------|---------|
| `loadAnalyzersList` | analyzer | Load available analyzer plugins |
| `createAnalyzer` | analyzer | Create analyzer instance |
| `startAnalyzer` | analyzer | Start analysis on captured data |
| `stopAnalyzer` | analyzer | Stop analysis |
| `updateAnalyzerSettings` | analyzer | Update analyzer configuration |
| `renderAnalyzer` | analyzer | Request analyzer render data |
| `deleteNode` | analyzer | Delete analyzer or other node |

### High-Level Analyzer (HLA) Operations

| messageType | Category | Purpose |
|-------------|----------|---------|
| `createHighLevelAnalyzer` | hla | Create Python HLA instance |
| `connectHighLevelAnalyzer` | hla | Connect HLA to analyzer |
| `disconnectHighLevelAnalyzer` | hla | Disconnect HLA |
| `renderHighLevelAnalyzer` | hla | Request HLA render data |
| `setHighLevelAnalyzerSettings` | hla | Configure HLA settings |

### Export Operations

| messageType | Category | Purpose |
|-------------|----------|---------|
| `createExportNode` | export | Create data export node |
| `createAnalyzerExportNode` | export | Create analyzer export |
| `connectAnalyzerExportNode` | export | Connect export to analyzer |
| `disconnectAnalyzerExportNode` | export | Disconnect export |
| `sendAnalyzerExportMessage` | export | Send export control message |

### Frame DB Operations

| messageType | Category | Purpose |
|-------------|----------|---------|
| `createFrameDBQueryNode` | framedb | Create frame database query |
| `createFrameDBQueryNodeForTest` | framedb | Create test query node |
| `createFrameDBWriterNode` | framedb | Create frame database writer |
| `sendQueryNodeMessage` | framedb | Send query message |
| `findResultInQueryNode` | framedb | Search for result |
| `writeToQueryNodeTestDb` | framedb | Write test entry |
| `finishQueryNodeTestDb` | framedb | Finish test database |

### Trigger Operations

| messageType | Category | Purpose |
|-------------|----------|---------|
| `connectAnalyzerTrigger` | trigger | Connect analyzer trigger |
| `disconnectAnalyzerTrigger` | trigger | Disconnect analyzer trigger |

### Diagnostics

| messageType | Category | Purpose |
|-------------|----------|---------|
| `flushLogs` | diag | Flush log buffers |
| `startTracing` | diag | Start performance tracing. Field: `filepath` |
| `stopTracing` | diag | Stop tracing |
| `crashBackend` | diag | Force crash for debugging. Field: `crashType` (0-7 jump table) |

## Calibration Data Flow

Calibration is separate from the main message protocol. It flows through graph actions:

1. `AnalogCalibrationFilterActions::SetCalibration` - Set calibration data per filter node
   - Field: `calibration` (calibration coefficients object)
2. `AnalogCalibrationFilterActions::SetChannel` - Set which channel the filter processes
   - Field: `channel` (channel index)
3. `LogicDevice::SetAdcFullScaleGain` - Set HMCAD ADC gain register
   - Field: `fullScaleGain` (register value `hittiteAdcGainRegisterValue`)

The JS side calls `sendCalibrationToGraph(calibrationData)` which:
1. Iterates all analog channels
2. For each channel's AnalogCalibrationFilter node, sends `SetCalibration` action
3. If `calibrationData.hittiteAdcGainRegisterValue` is set, sends `SetAdcFullScaleGain`

## Analog Scale Transforms

Device-specific ADC value to voltage transforms:
- **Logic**: default transform (unknown)
- **Logic16**: default transform
- **Logic4**: `compute1DTransform(0, 255, 0, 5)` -- 8-bit ADC, 0-5V range
- **Logic8**: `compute1DTransform(-2048, 4095, 0, 5)` -- 12-bit ADC, 0-5V range
- **LogicPro8**: `compute1DTransform(-2048, 4095, -10, 20)` -- 12-bit ADC, -10V to +10V range
- **LogicPro16**: `compute1DTransform(-2048, 4095, -10, 20)` -- same as Pro 8

ADC ranges (raw values):
- Logic4: 0 to 255
- All others: -2048 to 2047

Analog display ranges (volts):
- LogicPro8/16: -15V to +15V (max zoom out: -750 to +750)
- Others: -2.5V to +7.5V

## Message Count Summary

| Category | Count |
|----------|-------|
| Session management | 5 |
| Device operations | 4 |
| Device request sub-types | 10 |
| Capture configuration | 5 |
| Graph actions | 6 |
| Analyzer | 7 |
| HLA | 5 |
| Export | 5 |
| Frame DB | 7 |
| Trigger | 2 |
| Diagnostics | 4 |
| **Total messageTypes** | **50** |
| **Native dispatch types** | **6** |

## Source Files (from assertions/strings)

```
/workspace/monorepo/graph-io/server/graph_server/src/graph_server.cpp
/workspace/monorepo/graph-io/server/graph_server/src/request_watcher.cpp
/workspace/monorepo/graph-io/server/remote_client/src/remote_client.cpp
```

## Relevance to USB RE

The following message types directly affect USB device behavior:

1. **AcquireLogicDeviceRequest** -- Triggers firmware upload, FPGA programming, ADC init
2. **EnableChannels** -- Changes which channels are sampled (affects FPGA register config)
3. **SetSampleRate** -- Changes sample rate (affects FPGA clock dividers and HMCAD config)
4. **SetDigitalVoltageThreshold** -- Changes comparator threshold (FPGA register)
5. **SetAdcFullScaleGain** -- Sets HMCAD ADC gain register value
6. **StartCapture** -- Begins USB bulk transfer reads on EP2
7. **StopCapture** -- Stops capture, sends StopStream EP1 command
8. **SetCalibration** -- Calibration coefficients for analog channels (applied host-side by AnalogCalibrationFilter, but `hittiteAdcGainRegisterValue` is sent to device)

All other message types are GUI-only (rendering, analyzers, export, session management).
