---
id: TASK-0078
title: RE graph_server -- Electron-to-native IPC protocol
status: Done
assignee: []
created_date: '2026-03-10 12:31'
updated_date: '2026-03-10 18:25'
labels:
  - re
  - ghidra
  - ipc
dependencies: []
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Source paths leaked:
- `/workspace/monorepo/graph-io/server/graph_server/src/graph_server.cpp`
- `/workspace/monorepo/graph-io/server/graph_server/src/request_watcher.cpp`

The graph server is the IPC bridge between the Electron UI and the native library. Uses Boost.Asio (epoll reactor) as an async socket server. Understanding this would reveal:
- The full command/response protocol between UI and native code
- How capture settings are passed from UI to device layer
- How streaming data flows back to the UI
- The automation API surface (if any)

Investigation steps:
1. Search for strings like "graph_server", "request", "socket", "pipe" in binary
2. Identify the server's listen/accept path (Boost.Asio acceptor)
3. Decompile request dispatch logic
4. Map command names to handler functions
5. Check if this is the same protocol used by the Saleae automation API

Related: TASK-0022 (graph-interface.node) covered the Electron side. This is the native side.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Reverse engineered the GraphServer native-side IPC architecture inside libgraph_server_shared.so.

Key findings:
1. **Two IPC paths**: Direct mode (Linux default) uses dlopen/dlsym with JSON string passing through DriveRequest/PopResponse. WebSocket mode (macOS) uses Simple-WebSocket-Server on ws://localhost:{port}/saleae.
2. **Automation API is separate**: gRPC server (saleae.automation.Manager) on port 10430, running in Electron renderer, translating gRPC to the same JSON protocol. 16 RPC methods identified.
3. **47 messageType values** documented (submitActions, sendLogicDeviceRequest, createGraph, etc.)
4. **60+ request handler registrations** extracted from mangled C++ symbols, mapping Node -> Request -> Response tuples.
5. **60+ pipe handler registrations** showing streaming data flow between processing nodes.
6. **~30 processing node types** forming a DAG: DeviceListNode, LogicDeviceNode, DisaggregatorNode, DigitalStore, AnalogStore, AnalogCalibrationFilter, AnalyzerNode, HlaNode, etc.
7. **Capture data path**: USB -> DisaggregatorNode -> Digital/AnalogStore -> Renderers/Triggers/Analyzers. Calibration applied by AnalogCalibrationFilter.
8. **Internal Redux-like state management** (Saleae::Redux::ReduxStore) with typed state objects (State2 versions).
9. **Serialization**: RapidJSON with SerializeConverter<T> template specializations.
10. **Source paths**: /workspace/monorepo/graph-io/ built with vcpkg + libc++.

Created: re/notes/graph-server.md (618 lines)
<!-- SECTION:FINAL_SUMMARY:END -->
