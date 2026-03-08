---
id: TASK-0022
title: RE graph-interface.node native addon -- Electron bridge
status: To Do
assignee: []
created_date: '2026-03-08 22:53'
labels: []
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The @saleae/graph-interface native Node addon (108KB, not stripped) at resources/app.asar.unpacked/node_modules/@saleae/graph-interface/bin/linux-x64-106/graph-interface.node is likely the bridge between Electron JS and libgraph_server_shared.so.

At 108KB and NOT stripped, this is probably the easiest RE entry point in the entire project. It should have exported symbols revealing:
1. What N-API functions are exposed to JS
2. How it loads/calls into libgraph_server_shared.so
3. The IPC mechanism (direct dlopen? spawn subprocess? socket?)
4. The command/response API surface visible to JS

Also examine the TypeScript definitions in dist/ (index.d.ts, instance.d.ts, fs.d.ts) -- these are NOT stripped and describe the JS-visible API in plain text.

This task complements TASK-0005 (asar analysis) -- together they reveal the full Electron<->native interface.
<!-- SECTION:DESCRIPTION:END -->
