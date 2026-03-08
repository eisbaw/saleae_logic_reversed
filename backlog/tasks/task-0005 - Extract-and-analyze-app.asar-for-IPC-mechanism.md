---
id: TASK-0005
title: Extract and analyze app.asar for IPC mechanism
status: Done
assignee: []
created_date: '2026-03-08 22:42'
updated_date: '2026-03-08 23:40'
labels: []
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Extract resources/app.asar using electron-asar tool. Identify:
1. How the Electron JS communicates with libgraph_server_shared.so
2. Is it gRPC, named pipes, direct N-API, or something else?
3. If gRPC: extract .proto definitions
4. Map the command vocabulary (what commands does JS send to native?)
5. Is graph-interface.node the bridge between JS and native code?

Hypothesis: the IPC mechanism reveals the high-level API surface of the native library, making it easier to identify which native functions implement which features.

Output findings to re/notes/ipc-analysis.md. If IPC protocol is confirmed, promote to doc/protocol/ipc.md.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Extracted app.asar and fully analyzed the IPC mechanism between Electron and libgraph_server_shared.so.

Key findings:
- Primary IPC is JSON message-passing through a N-API native addon (graph-interface.node)
- graph-interface.node dlopen()s libgraph_server_shared.so and uses a C API: CreateGraphServer, DestroyGraphServer, DriveRequest, PopResponse, FreeResponseBuffer, SetLogFileName, SetLogCallback, FlushLog
- Two transport modes: direct (in-process, Linux default) and WebSocket (macOS/out-of-process on ws://localhost:{port}/saleae)
- Message format: JSON with Saleae::Graph::* namespaced type strings, envelope with type/contents fields
- Backend is a DAG graph processor -- device operations are routed as actions to LogicDeviceNode (node ID 4)
- 80+ message types cataloged covering graph control, device operations, triggers, rendering, analyzers, frame search
- Separate gRPC automation API (saleae.automation) on port 10430 with protobuf serialization
- Protobuf is only for the external automation API; internal IPC is plain JSON

Deliverables:
- re/notes/ipc-analysis.md -- comprehensive analysis with full message catalog
- doc/protocol/ipc.md -- verified protocol specification
- re/scripts/ -- extraction scripts documenting methodology
<!-- SECTION:FINAL_SUMMARY:END -->
