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
(with `RTLD_LAZY | RTLD_GLOBAL`) and communicates via a C API. The constructor
also calls `setenv("SALEAE_PYTHONHOME", pythonHomePath)` before loading the .so.

```
graph-interface.node  --(dlopen RTLD_GLOBAL)-->  libgraph_server_shared.so
     |                                                |
     +-- DriveRequest(server, json, len)  ----------> |  (mutex-protected)
     |                                                |
     +-- PopResponse(server, &data, &len) <--------- |  (atomic refcount)
     |   FreeResponseBuffer(data)                     |
     +-- SetLogFileName(path)                         |
     +-- SetLogCallback(fn)                           |
     +-- FlushLog()                                   |
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

Resolved via `dlsym()` in this exact order (confirmed from binary disassembly
of the Instance constructor). The addon uses `dlopen(path, RTLD_LAZY | RTLD_GLOBAL)`.
`RTLD_GLOBAL` is needed because the .so embeds a Python interpreter whose
extension modules must resolve symbols back into the main library.

```c
// Create graph server instance.
// port_ptr: NULL for in-process mode, int* for WebSocket mode
// param: always 100 (hardcoded in addon)
// dev_mode: false in production
// Returns: opaque server handle
void* CreateGraphServer(int* port_ptr, int param, bool dev_mode);

// Send a JSON request string. Called under mutex (one at a time).
void DriveRequest(void* server, const char* request, size_t length);

// Pop a response from the queue. Thread-safe (uses atomic refcounting).
// Returns: non-zero if response available, 0 if queue empty
int PopResponse(void* server, char** out_data, size_t* out_len);

// Free response buffer returned by PopResponse.
void FreeResponseBuffer(char* buffer);

// Destroy the graph server instance.
void DestroyGraphServer(void* server);

// Set log output file path. Global (no server handle).
void SetLogFileName(const char* path);

// Flush log buffers. Global (no server handle).
void FlushLog(void);

// Set callback for log messages. Global (no server handle).
void SetLogCallback(void (*callback)(unsigned char* data, size_t len));
```

See `re/notes/graph-interface-analysis.md` for the full binary analysis
including object layout, data flow diagrams, and thread safety model.

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
