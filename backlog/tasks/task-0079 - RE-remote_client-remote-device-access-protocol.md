---
id: TASK-0079
title: RE remote_client -- remote device access protocol
status: Done
assignee: []
created_date: '2026-03-10 12:32'
updated_date: '2026-03-10 18:35'
labels:
  - re
  - ghidra
  - ipc
dependencies: []
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Source path leaked: `/workspace/monorepo/graph-io/server/remote_client/src/remote_client.cpp`

A remote client module exists in the binary. This could be:
- Network-based device sharing (use a Logic device on another machine)
- The Saleae automation/scripting API client
- Cloud connectivity for calibration or telemetry

Investigation steps:
1. Search binary for strings like "remote_client", "remote", "connect" near this TU
2. Decompile the client class
3. Identify the transport (TCP? WebSocket? gRPC?)
4. Determine if this is user-facing or internal
5. Check relationship to named_pipe IPC

Lower priority -- mainly interesting for understanding the full architecture.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Reverse engineered the remote_client module from libgraph_server_shared.so.

Key findings:
- Despite the "client" name, remote_client is a WebSocket SERVER inside the native library
- Transport: SimpleWeb::SocketServer (header-only C++ lib) on Boost.Asio, plain TCP
- Binds to 127.0.0.1 on a dynamically-assigned port
- Two WebSocket endpoints: ^/saleae/?$ (main IPC) and ^/analyzer/?$ (analyzer-specific)
- Carries the exact same JSON protocol as the in-process named pipe path
- Used for out-of-process mode (macOS) and --useExistingGraph debugging
- NOT network device sharing, NOT cloud connectivity, NOT the automation API

Decompiled 8 functions covering: constructor, request handler, response serializer, WebSocket event handlers (open/close/error/message), port retrieval, and send.

Created re/notes/remote-client.md with full analysis including ELF VAs, class signatures, protocol flow, and architecture diagram.
<!-- SECTION:FINAL_SUMMARY:END -->
