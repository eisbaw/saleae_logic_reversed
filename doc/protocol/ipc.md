# IPC Protocol: Electron <-> libgraph_server_shared.so

**Confidence: Confirmed** (source code analysis of main.js, worker.js,
graph-interface.node binary symbols, TypeScript type declarations)

## Overview

The Saleae Logic 2 application uses a JSON message-passing protocol between
its Electron frontend and the native `libgraph_server_shared.so` backend.
The backend implements a directed acyclic graph (DAG) of processing nodes.

## Transport

### Primary: In-Process (Direct Mode, Linux)

The `graph-interface.node` N-API addon `dlopen()`s `libgraph_server_shared.so`
and communicates via a C API:

```
graph-interface.node  --(dlopen)-->  libgraph_server_shared.so
     |                                    |
     +-- DriveRequest(json_string)  ----> |
     |                                    |
     +-- PopResponse() <---------------- |
     |   FreeResponseBuffer()             |
     +-- SetLogFileName(path)             |
     +-- SetLogCallback(fn)               |
     +-- FlushLog()                       |
```

### Secondary: WebSocket (macOS, or --useExistingGraph)

```
Renderer --WebSocket ws://localhost:{port}/saleae--> libgraph_server_shared.so
```

Started via FFI: `CreateGraphServer(port_ptr, 100, dev_mode)`.

## Message Format

### Request (JS -> Native)

Wrapped in envelope for WebSocket mode:
```json
{"type": "request", "contents": <message>}
```

In direct mode, the message object is sent directly via `Instance.send()`.

### Response (Native -> JS)

```json
{
  "type": "response" | "error",
  "contents": {
    "id": <int>,
    "type": "<Saleae::* type>",
    "meta": {
      "sessionId": "<uuid>",
      "destination": "GraphServer" | "Graph",
      "portName": "<string>",
      "portIndex": <int>
    },
    ...payload...
  }
}
```

## C API (from dlsym resolution in graph-interface.node)

| Function | Signature (inferred) | Purpose |
|----------|---------------------|---------|
| `CreateGraphServer` | `void*(int*, int, bool)` | Create server, returns handle |
| `DestroyGraphServer` | `void(void*)` | Destroy server |
| `DriveRequest` | drives a JSON request string | Send request |
| `PopResponse` | returns JSON string or null | Get response |
| `FreeResponseBuffer` | frees response string | Free response memory |
| `SetLogFileName` | `void(char*)` | Set log file path |
| `SetLogCallback` | sets callback function | Log forwarding |
| `FlushLog` | `void()` | Flush logs |

## Graph Architecture

The backend processes data through a DAG of nodes:

| Node | ID | Purpose |
|------|----|---------|
| DisaggregatorNode | 1 | Signal disaggregation, glitch filter |
| DigitalTriggerNode | 2 | Digital trigger detection |
| AnalyzerTriggerNode | 3 | Analyzer-based trigger |
| LogicDeviceNode | 4 | USB device operations |
| DeviceListNode | 1* | Device enumeration |

*DeviceListNode uses ID 1 in a separate "device list" graph session.

Nodes are connected via pipes. Actions are dispatched to nodes via
`GraphActions::RouteAction`. The JS side constructs the graph topology and
then sends actions; the native side executes them.

## Core Message Types

Graph control: `Saleae::Graph::GraphInitRequestData`, `DeleteGraphData`,
`GraphActionData`

Graph actions: `AddNode`, `RemoveNode`, `AddPipe`, `RemovePipe`,
`ConnectApi`, `DisconnectApi`, `Start`, `Stop`, `RouteAction`

Device: Opaque actions routed to `LogicDeviceNode` via `RouteAction`

Triggers: `DigitalTriggerSettingsData`, `AnalogTriggerSettingsData`,
`AnalyzerTriggerSettingsData`

See `re/notes/ipc-analysis.md` for the complete 80+ message type catalog.

## Automation gRPC API (Separate)

A separate gRPC service (`saleae.automation`) runs on port 10430 when
`--automation` is enabled. This is the public API used by the Python SDK.
Uses protobuf serialization. Key RPCs: `StartCapture`, `StopCapture`,
`GetDevices`, `AddAnalyzer`, `ExportRawDataCsv`.
