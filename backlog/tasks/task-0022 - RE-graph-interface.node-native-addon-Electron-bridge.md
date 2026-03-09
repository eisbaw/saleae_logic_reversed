---
id: TASK-0022
title: RE graph-interface.node native addon -- Electron bridge
status: Done
assignee: []
created_date: '2026-03-08 22:53'
updated_date: '2026-03-09 01:20'
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

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
## Analysis Complete

Fully reverse engineered `graph-interface.node` (108KB, non-stripped N-API addon).

### Key Findings

**Architecture**: Pure passthrough shim with zero business logic. Written in C++ with `node-addon-api` v3.0.2, compiled with GCC 7.5.0 on Ubuntu 18.04. Source files: `module.cpp`, `instance.cpp`, `fs.cpp`, `util.cpp`.

**C API Resolution**: Constructor does `dlopen(path, RTLD_LAZY | RTLD_GLOBAL)` then resolves 8 symbols via `dlsym()` in this order:
1. `CreateGraphServer` -> `this+0x28`
2. `DriveRequest` -> `this+0x30`
3. `PopResponse` -> `this+0x38`
4. `FreeResponseBuffer` -> `this+0x40`
5. `DestroyGraphServer` -> `this+0x48`
6. `SetLogFileName` -> `this+0x50`
7. `FlushLog` -> `this+0x58`
8. `SetLogCallback` -> `this+0x60`

**Refined Signatures** (confirmed from disassembly):
- `DriveRequest(server, string, length)` -- three args, not one
- `PopResponse(server, &data_ptr, &data_len)` -- out-parameters
- `SetLogCallback(void(*)(uint8_t*, size_t))` -- typed callback
- `CreateGraphServer(NULL, 100, false)` for in-process mode

**Thread Safety**:
- Send path: mutex-protected, runs on libuv worker thread
- Receive path: lock-free polling with atomic reference counting
- Notifications: N-API ThreadSafeFunction (thread-safe by design)

**Additional Details**:
- Sets `SALEAE_PYTHONHOME` env var before loading .so
- `RTLD_GLOBAL` flag needed for embedded Python extension modules
- `FsTools.tryLock()` implements POSIX file locking via `open64()` + `fcntl()`
- `Utils.needsOutOfProcessBackend()` always returns false on Linux

### Files Created/Updated
- Created: `re/notes/graph-interface-analysis.md` (comprehensive analysis)
- Updated: `doc/protocol/ipc.md` (refined C API signatures with confirmed details)
<!-- SECTION:FINAL_SUMMARY:END -->
