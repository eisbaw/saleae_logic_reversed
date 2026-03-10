---
id: TASK-0080
title: RE named_pipe IPC mechanism
status: Done
assignee: []
created_date: '2026-03-10 12:32'
updated_date: '2026-03-10 18:43'
labels:
  - re
  - ghidra
  - ipc
dependencies: []
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Source path leaked: `/workspace/monorepo/graph-io/core/named_pipe/src/named_pipe_unix.cpp`

Named pipes are used for IPC, likely between the Electron process and the native graph server. Understanding this reveals the transport layer for the UI↔native protocol.

Investigation steps:
1. Search binary for strings like "named_pipe", pipe path patterns (/tmp/*, /var/*)
2. Decompile the named pipe wrapper class
3. Determine pipe naming convention and lifecycle
4. Check if graph_server uses this or Boost.Asio TCP sockets (or both)
5. Document the framing protocol (length-prefixed? newline-delimited? protobuf?)

Lower priority -- transport plumbing, not core device protocol.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Reverse engineered the named pipe IPC mechanism from libgraph_server_shared.so.

Key findings:
- Named pipes are POSIX FIFOs in /tmp/{uuid}, NOT part of the main IPC protocol
- Used exclusively for analyzer terminal output (text stream to xterm.js widget)
- Class hierarchy: Saleae::NamedPipe (abstract) -> Saleae::NamedPipeUnix (concrete, extends std::iostream)
- Lifecycle: mkfifo("/tmp/{id}", 0666) in ctor, open(O_WRONLY|O_NONBLOCK) in TryConnect, poll+write in Write, unlink in dtor
- Uses anonymous pipe pair for interrupt signaling (to cancel blocking writes)
- No framing protocol -- raw UTF-8 text byte stream, no length prefixes or delimiters
- Backpressure: writes limited to 4096 bytes, pauses at buffer >4096 with sched_yield loop
- Reader side (JS): fs.open(O_RDONLY|O_NONBLOCK) wrapped in net.Socket with event-driven data callback
- /namedPipeIdentifier published in analyzer/HLA node state for renderer to discover
- Decompiled 6 core functions: constructor (0x20f6100), destructor (0x20f6400), TryConnect (0x20f6cd0), Write (0x20f6f00), StreamTerminal (0x1c6a0f0), state serializers

Low relevance to USB RE -- purely analyzer terminal plumbing.
<!-- SECTION:FINAL_SUMMARY:END -->
