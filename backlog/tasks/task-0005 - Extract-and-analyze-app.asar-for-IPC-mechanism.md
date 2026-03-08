---
id: TASK-0005
title: Extract and analyze app.asar for IPC mechanism
status: To Do
assignee: []
created_date: '2026-03-08 22:42'
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
