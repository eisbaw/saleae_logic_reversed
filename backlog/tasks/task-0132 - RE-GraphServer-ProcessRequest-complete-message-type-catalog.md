---
id: TASK-0132
title: 'RE GraphServer::ProcessRequest -- complete message type catalog'
status: To Do
assignee: []
created_date: '2026-03-11 11:21'
labels:
  - re
  - ghidra
  - ipc
  - graph-server
dependencies: []
references:
  - re/notes/graph-server.md
  - re/notes/ipc-analysis.md
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Saleae::GraphServer::ProcessRequest(...) is the main dispatcher handling 47+ messageTypes from the Electron frontend. TASK-0078 documented the message type list but didn't decompile the individual handlers.

Understanding the complete request/response protocol enables:
- Automation API compatibility (send the same messages as the Electron app)
- Understanding exactly what data the frontend sends for each operation
- Potentially driving the original binary from our code for validation

Investigation:
1. Decompile ProcessRequest and its dispatch table
2. For each message type, document: request fields, response fields, side effects
3. Focus on device-related messages: SetSampleRate, StartCapture, StopCapture, GetCalibration, SetChannel
4. Skip GUI-only messages (rendering, zoom, pan, etc.)

Low priority -- we're building a standalone driver, not a frontend replacement.
<!-- SECTION:DESCRIPTION:END -->
