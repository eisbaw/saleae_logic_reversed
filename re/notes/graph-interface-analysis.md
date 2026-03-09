# graph-interface.node Native Addon Analysis

## Summary

The `graph-interface.node` file is a 108KB **non-stripped** N-API native addon
that acts as the bridge between the Electron renderer (via a Web Worker) and
`libgraph_server_shared.so`. It is written in C++ with `node-addon-api`
(N-API C++ wrapper), compiled with GCC 7.5.0 on Ubuntu 18.04. The source
consists of three files: `instance.cpp`, `fs.cpp`, and `util.cpp`, combined
in `module.cpp`.

This is a **thin shim** -- it contains no business logic beyond:
1. Loading `libgraph_server_shared.so` via `dlopen()`
2. Resolving 8 C API symbols via `dlsym()`
3. Forwarding JSON string requests/responses between N-API and the C API
4. Providing a file-locking utility (`FsTools`)
5. Providing platform utilities (`Utils`)

## Binary Details

| Property | Value |
|----------|-------|
| Path | `resources/app.asar.unpacked/node_modules/@saleae/graph-interface/bin/linux-x64-106/graph-interface.node` |
| Size | ~108 KB |
| Type | ELF 64-bit LSB shared object, x86-64, not stripped |
| SONAME | `saleae_graph_interface.node` |
| Compiler | GCC 7.5.0 (Ubuntu 7.5.0-3ubuntu1~18.04) |
| N-API | `node-addon-api` v3.0.2 |
| Module ABI | 106 (Electron 19.x / Node 16.x) |
| Dependencies | libstdc++.so.6, libgcc_s.so.1, libpthread.so.0, libc.so.6 |
| Source files | `module.cpp`, `instance.cpp`, `fs.cpp`, `util.cpp` |

**Key observation**: The addon does NOT link to `libgraph_server_shared.so`
at build time. It uses `dlopen()` + `dlsym()` at runtime, meaning the .so
path is provided by the JS caller.

## Module Registration

The addon registers as a Node.js native module via:

```
_register_saleae_graph_interface  (INIT_ARRAY entry)
  -> napi_module_register(&_module)
  -> __napi_Init(env, exports)
     -> Init(Napi::Env, Napi::Object)
        -> Instance::Init(env, exports)  -- registers "Instance" class
        -> FsTools::Init(env, exports)   -- registers "FsTools" namespace
        -> Utils::Init(env, exports)     -- registers "Utils" namespace
```

The module is loaded by the `bindings` npm package, which searches:
```
bin/${platform}-${arch}-${process.versions.modules}/graph-interface.node
```
On Linux x64 with Electron 19 (modules=106): `bin/linux-x64-106/graph-interface.node`

## Exported JS API

### Instance Class (ObjectWrap)

The `Instance` class is a `Napi::ObjectWrap<Instance>` that wraps the C API.
It is constructed from JS with `new Instance(graphServerSharedPath, pythonHomePath, logPath)`.

**Constructor**: `Instance(graphServerSharedPath: string, pythonHomePath: string, logPath: string)`

The constructor performs these steps (confirmed from disassembly at 0xa630):

1. Validates exactly 3 string arguments (throws TypeError "invalid arguments" otherwise)
2. Extracts `graphServerSharedPath` (arg 0) as a C string
3. Extracts `pythonHomePath` (arg 1) as a C string
4. Sets environment variable `SALEAE_PYTHONHOME` to `pythonHomePath` via `setenv()`
5. Extracts `logPath` (arg 2) as a C string
6. Calls `dlopen(graphServerSharedPath, RTLD_LAZY | RTLD_GLOBAL)` (flags = 0x101)
7. Resolves 8 symbols via `dlsym()` (see C API section below)
8. Checks all 8 symbols are non-NULL (throws "missing symbols in graph_server_shared" if any NULL)
9. Calls `SetLogFileName(logPath)` via resolved function pointer
10. Calls `CreateGraphServer(NULL, 100, false)` -- in-process mode, no port needed
11. Stores the returned server handle
12. Sets up ThreadSafeFunction callbacks for `_onNotifyMessage` and `_logCallback`
13. Registers `SetLogCallback()` with the native library, pointing to `LogCallback`

**Instance methods** (registered via `napi_define_class`):

| JS Method | C++ Method | Type | Purpose |
|-----------|-----------|------|---------|
| `_send(message: string)` | `Instance::Send` | InstanceMethod (returns Promise) | Send JSON request to native |
| `_tryRecv()` | `Instance::TryRecv` | InstanceMethod (returns string\|null) | Poll for response |
| `flushLogs()` | `Instance::FlushLogs` | InstanceMethod (returns Promise) | Flush log buffers |
| `_onNotifyMessage` | -- | Callback (set via constructor lambda) | Called from native thread |
| `_logCallback` | -- | Callback (set via constructor lambda) | Called from native thread |

**Destructor**: `Instance::~Instance()` at 0x80b0

The destructor (confirmed from disassembly):
1. Calls `DestroyGraphServer(server_handle)` via function pointer at `this+0x48`
2. Calls `dlclose(dl_handle)` on the stored dlopen handle at `this+0x20`

### FsTools Namespace

Static methods, no class instantiation:

| JS Method | C++ Method | Purpose |
|-----------|-----------|---------|
| `tryLock(path: string)` | `FsTools::TryLock` | Acquire file lock (POSIX flock) |

Implementation uses `open64()` + `fcntl()` with `F_SETLK` for non-blocking
advisory file locking. Returns boolean. This is used by the Electron app to
prevent multiple instances from running simultaneously.

### Utils Namespace

Static methods, no class instantiation:

| JS Method | C++ Method | Purpose |
|-----------|-----------|---------|
| `needsOutOfProcessBackend()` | `Utils::NeedsOutOfProcessBackend` | Check if WebSocket mode needed |
| `waitForDebugger()` | `Utils::WaitForDebugger` | Block until debugger attaches |
| `isMacOsCtrlKeyDown()` | `Utils::IsMacOsCtrlKeyDown` | Check macOS Ctrl key state |

`NeedsOutOfProcessBackend()` always returns `false` on Linux (in-process/
direct mode is always used). The WebSocket/out-of-process mode is macOS-only.

`IsMacOsCtrlKeyDown()` throws "macOS only" on non-macOS platforms.

## C API Resolution (dlsym Order)

The constructor resolves 8 symbols from `libgraph_server_shared.so` in this
exact order (confirmed from disassembly at 0xacc2-0xad5d):

| # | Symbol | Stored At | Purpose |
|---|--------|-----------|---------|
| 1 | `CreateGraphServer` | `this+0x28` | Create server instance |
| 2 | `DriveRequest` | `this+0x30` | Send JSON request |
| 3 | `PopResponse` | `this+0x38` | Receive JSON response |
| 4 | `FreeResponseBuffer` | `this+0x40` | Free response memory |
| 5 | `DestroyGraphServer` | `this+0x48` | Destroy server |
| 6 | `SetLogFileName` | `this+0x50` | Set log file path |
| 7 | `FlushLog` | `this+0x58` | Flush log buffers |
| 8 | `SetLogCallback` | `this+0x60` | Set log callback |

After resolution, the constructor validates ALL 8 pointers are non-NULL
(checks at 0xad5d-0xada3). If any is NULL, it throws "missing symbols in
graph_server_shared".

### dlopen Flags

```c
dlopen(path, RTLD_LAZY | RTLD_GLOBAL)  // 0x101
```

`RTLD_GLOBAL` is significant -- it makes symbols from `libgraph_server_shared.so`
available to subsequently loaded shared objects. This may be needed because the
.so itself loads Python/Tcl plugins that need to resolve back into the main library.

## Instance Object Memory Layout

Based on disassembly analysis of the constructor and method access patterns:

| Offset | Size | Field | Description |
|--------|------|-------|-------------|
| 0x00 | 8 | vtable ptr | Instance vtable |
| 0x08 | 8 | env | napi_env (N-API environment) |
| 0x10 | 8 | ref | napi_ref (prevent GC) |
| 0x18 | 1 | flags | Boolean flags |
| 0x19 | 1 | initialized | Init flag |
| 0x20 | 8 | dl_handle | dlopen() handle |
| 0x28 | 8 | fn_CreateGraphServer | Function pointer |
| 0x30 | 8 | fn_DriveRequest | Function pointer |
| 0x38 | 8 | fn_PopResponse | Function pointer |
| 0x40 | 8 | fn_FreeResponseBuffer | Function pointer |
| 0x48 | 8 | fn_DestroyGraphServer | Function pointer |
| 0x50 | 8 | fn_SetLogFileName | Function pointer |
| 0x58 | 8 | fn_FlushLog | Function pointer |
| 0x60 | 8 | fn_SetLogCallback | Function pointer |
| 0x68 | 8 | server_handle | Return value of CreateGraphServer |
| 0x70 | 8 | tsf_notify | ThreadSafeFunction for _onNotifyMessage |
| 0x78-0xA8 | | ... | ThreadSafeFunction state, misc |
| 0xB0 | 8 | shared_ptr | Reference-counted shared state |

## Data Flow: Send Path

The `Instance._send(jsonString)` call follows this path (from disassembly):

```
JS: instance._send("{ ... JSON ... }")
  |
  v
Instance::Send(CallbackInfo)          [0x8d90]
  |-- Validates 1 string argument
  |-- Creates a Promise (napi_create_promise)
  |-- Extracts string argument to std::string
  |-- Allocates SendAsyncWorker (0xC8 bytes, operator new)
  |-- Stores: env, deferred, self-reference, request string
  |-- Queues async work (napi_queue_async_work)
  |-- Returns the Promise to JS
  |
  v (on libuv worker thread)
SendAsyncWorker::Execute()            [0xe950]
  |-- Acquires mutex (pthread_mutex_lock on this+0x70 of Instance)
  |-- Calls: fn_DriveRequest(server_handle, request_str, request_len)
  |     i.e.: call *0x30(%rax) with args from worker
  |-- Releases mutex
  |
  v (back on main thread)
SendAsyncWorker::OnOK()               [0xc880]
  |-- Resolves the Promise (napi_resolve_deferred)
  |
SendAsyncWorker::OnError()            [0xc660]
  |-- Rejects the Promise (napi_reject_deferred)
  |-- Logs: "error while resolving JS send promise: " + error.what()
```

**Key observations**:
- Send is **async** (Promise-based), runs on a libuv worker thread
- A **mutex** protects the DriveRequest call, serializing concurrent sends
- The JS wrapper (`instance.js`) further serializes sends by chaining promises
- This means requests to the native library are strictly sequential

## Data Flow: Receive Path

The `Instance._tryRecv()` call follows this path (from disassembly):

```
JS: const response = instance._tryRecv()
  |
  v
Instance::TryRecv(CallbackInfo)       [0x7cf0]
  |-- Atomically increments reference count on shared state (lock addq at +0xb0)
  |-- Calls: fn_PopResponse(server_handle, &data_ptr, &data_len)
  |     i.e.: call *0x38(this) with server_handle from this+0x68
  |-- If response available (non-zero return):
  |     Creates napi string from (data_ptr, data_len)
  |     Calls: fn_FreeResponseBuffer(data_ptr)
  |       i.e.: call *0x40(this)
  |     Returns the string to JS
  |-- If no response:
  |     Returns null to JS
  |-- Atomically decrements reference count
```

**Key observations**:
- TryRecv is **synchronous** (no Promise, no async work)
- It is a **polling** operation -- returns null if no response is ready
- The response buffer is freed after copying to a JS string
- PopResponse appears to return: (success/failure, data_ptr, data_len)

## Data Flow: Notification Path

When the native library has a response ready, it notifies the JS thread:

```
Native thread (inside libgraph_server_shared.so):
  |-- Calls NotifyMessage callback (registered during construction)
  |
  v
NotifyMessage()                       [0xa480]
  |-- Calls Instance::NotifyMessageThreadsafe()
  |
  v
Instance::NotifyMessageThreadsafe()   [0xa300]
  |-- Calls napi_call_threadsafe_function to wake JS thread
  |
  v (on JS main thread)
ThreadSafeFunction handler:
  |-- Calls instance._onNotifyMessage() on the JS object
  |
  v
JS: instance._onNotifyMessage()
  |-- Resolves waiting Promise callbacks
  |-- Caller then calls _tryRecv() to get the actual data
```

**Key observations**:
- The notification is **asynchronous** and thread-safe (uses N-API ThreadSafeFunction)
- Notifications do NOT carry data -- they just signal "a response is available"
- The JS side must call `_tryRecv()` after notification to get the actual response
- This is a classic **producer-consumer** pattern with a notification mechanism

## Data Flow: Log Callback Path

```
Native thread:
  |-- Calls LogCallback(data_ptr, data_len)
  |
  v
LogCallback()                         [0xa560]
  |-- Calls Instance::LogCallbackThreadsafe(data_ptr, data_len)
  |
  v
Instance::LogCallbackThreadsafe()     [0xa380]
  |-- Calls napi_call_threadsafe_function to relay to JS thread
  |
  v (on JS main thread)
ThreadSafeFunction handler:
  |-- Creates napi string from (data_ptr, data_len)
  |-- Calls instance._logCallback(message) on the JS object
```

There is also `LogCallbackNoop` at 0x7960 (a single `ret` instruction) -- this
is likely the default callback before a real one is registered.

## Data Flow: FlushLogs Path

```
JS: await instance.flushLogs()
  |
  v
Instance::FlushLogs(CallbackInfo)     [0x82c0]
  |-- Creates a Promise
  |-- Allocates FlushLogWorker
  |-- Queues async work
  |-- Returns Promise
  |
  v (on libuv worker thread)
FlushLogWorker::Execute()             [0xbee0]
  |-- Calls: fn_FlushLog()
  |     i.e.: jmp *0x58(%rax) -- tail call to FlushLog
  |
  v (back on main thread)
FlushLogWorker::OnOK()                [0xc570]
  |-- Resolves the Promise
```

## C API Function Signatures (Refined)

Based on the disassembly call patterns, the C API signatures are:

```c
// Create graph server instance
// port_ptr: NULL for in-process mode, int* for WebSocket mode
// param: always 100 (possibly max concurrent connections or queue size)
// dev_mode: false in production
// Returns: opaque server handle
void* CreateGraphServer(int* port_ptr, int param, bool dev_mode);

// Send a JSON request string to the server
// server: handle from CreateGraphServer
// request: JSON string (not null-terminated -- length is provided)
// length: length of request string
void DriveRequest(void* server, const char* request, size_t length);

// Pop a response from the response queue
// server: handle from CreateGraphServer
// out_data: receives pointer to response string
// out_len: receives length of response string
// Returns: non-zero if response available, 0 if queue empty
int PopResponse(void* server, char** out_data, size_t* out_len);

// Free a response buffer returned by PopResponse
void FreeResponseBuffer(char* buffer);

// Destroy the graph server instance
void DestroyGraphServer(void* server);

// Set the log output file path
void SetLogFileName(const char* path);

// Flush log buffers to disk
void FlushLog(void);

// Set a callback function for log messages
// callback: function pointer void(*)(unsigned char* data, size_t len)
void SetLogCallback(void (*callback)(unsigned char* data, size_t len));
```

**Refinements from this analysis vs. TASK-0005 findings**:

1. `DriveRequest` takes `(server, string, length)` -- confirmed three arguments
2. `PopResponse` takes `(server, &data_ptr, &data_len)` -- returns data by out-parameters
3. `CreateGraphServer` takes `(port_ptr, 100, dev_mode)` -- the `100` is hardcoded
4. `SetLogCallback` takes a function pointer `void(*)(uint8_t*, size_t)`
5. `DriveRequest` is called under a mutex, confirming single-threaded access
6. `PopResponse` uses atomic reference counting, suggesting thread-safe polling
7. `FreeResponseBuffer` takes just the data pointer (not server handle)
8. `SetLogFileName` and `FlushLog` appear to be global (no server handle parameter)

## JS-Side Wrapper (from instance.js)

The TypeScript/JS wrapper in `instance.js` adds:

1. **JSON serialization**: `send(message)` calls `JSON.stringify(message)` before
   passing to `_send()`, and `recv()` calls `JSON.parse()` on `_tryRecv()` result
2. **Sequential sends**: `_previousSend` Promise chain ensures one send at a time
3. **Blocking receive**: `recv()` loops on `_tryRecv()`, sleeping via
   `_onNotifyMessage` callbacks between polls (no busy-wait)
4. **Notification fan-out**: `_onNotifyMessage()` resolves all waiting Promise
   callbacks, which then individually call `_tryRecv()`

The complete send/receive cycle:

```
JS: await instance.send({ type: "request", contents: { ... } })
  -> JSON.stringify(message)
  -> await _send(jsonString)
  -> DriveRequest(server, jsonString, len)   // on worker thread

JS: const response = await instance.recv()
  -> loop:
       result = _tryRecv()                   // synchronous poll
       if result: return JSON.parse(result)
       else: await new Promise(/* wait for _onNotifyMessage */)
```

## Architectural Observations

### The addon is a pure passthrough

There is zero business logic in `graph-interface.node`. It does not:
- Parse or validate JSON messages
- Transform data
- Buffer or queue messages (beyond what N-API async workers do)
- Maintain state beyond the C API function pointers and server handle

The entire intelligence is in `libgraph_server_shared.so`.

### Thread safety model

- **Send**: Mutex-protected, one request at a time, on a libuv worker thread
- **Receive**: Lock-free polling with atomic reference counting
- **Notifications**: N-API ThreadSafeFunction (thread-safe by design)
- **Log callbacks**: N-API ThreadSafeFunction

The native library (`libgraph_server_shared.so`) can call `NotifyMessage` and
`LogCallback` from any thread. The addon safely marshals these to the JS
main thread via ThreadSafeFunction.

### dlopen RTLD_GLOBAL

The use of `RTLD_GLOBAL` (flag 0x101) means all symbols from the .so are
globally visible. This is unusual for a typical addon but makes sense because
`libgraph_server_shared.so` embeds a Python interpreter that may need to
`dlopen()` Python extension modules, which in turn need to find Python
symbols in the already-loaded library.

### SALEAE_PYTHONHOME environment variable

The constructor sets `SALEAE_PYTHONHOME` via `setenv()` before any calls into
the .so. This configures Python's home directory, confirming that the embedded
Python interpreter is used (likely for High-Level Analyzers).

### CreateGraphServer(NULL, 100, false)

In direct/in-process mode, the first argument (port_ptr) is NULL, meaning no
WebSocket server is started. The `100` parameter is always hardcoded.
`false` for dev_mode in production.

In WebSocket mode (macOS), the same function is called via FFI from the
Electron main process with a non-NULL port_ptr to receive the assigned port.

## Cross-Reference with IPC Analysis (re/notes/ipc-analysis.md)

The IPC analysis from TASK-0005 is **confirmed and consistent** with these
binary-level findings:

1. The 8 C API functions match exactly
2. The dlopen/dlsym pattern matches
3. The JSON string-passing protocol matches (no protobuf at this layer)
4. The async send + polling receive pattern matches
5. The notification mechanism matches (_onNotifyMessage -> ThreadSafeFunction)
6. The SALEAE_PYTHONHOME setup matches
7. The RTLD_GLOBAL flag is a new detail not in the previous analysis

### New details from this analysis

- **Exact dlsym order** confirmed from binary
- **Instance object memory layout** mapped (function pointers at offsets 0x28-0x60)
- **DriveRequest signature** refined: takes (server, string, length) -- not just string
- **PopResponse signature** refined: returns data via out-parameters (data_ptr, data_len)
- **Mutex** on DriveRequest confirmed (not just JS-level serialization)
- **Atomic refcounting** on PopResponse path
- **RTLD_GLOBAL** flag (0x101) -- not previously documented
- **dlopen flags** = RTLD_LAZY | RTLD_GLOBAL
- **CreateGraphServer hardcoded 100** parameter
- **FreeResponseBuffer** takes only the data pointer, not the server handle
- **SetLogCallback** signature: `void(*)(uint8_t*, size_t)`
- **FlushLogWorker** uses tail-call to FlushLog (direct jmp, no wrapper)

## Error Messages in Binary

| String | Context |
|--------|---------|
| "failed to load graph_server_shared: " | dlopen() failed, followed by dlerror() |
| "missing symbols in graph_server_shared" | Any of the 8 dlsym() calls returned NULL |
| "error while notifying JS thread: " | ThreadSafeFunction callback error |
| "error while resolving JS send promise: " | SendAsyncWorker::OnError |
| "invalid arguments" | Wrong argument count/type to _send, _tryRecv, or constructor |
| "Class constructors cannot be invoked without 'new'" | Missing `new` keyword |
| "encountered unexpected filesystem error opening file" | FsTools::TryLock open failed |
| "encountered unexpected filesystem error acquiring lock" | FsTools::TryLock fcntl failed |
| "macOS only" | IsMacOsCtrlKeyDown called on non-macOS |

## Files Analyzed

- Binary: `/nix/store/brcaw41j62pjki70ahf7ffd2f3bhaa3d-saleae-logic-2-extracted/resources/app.asar.unpacked/node_modules/@saleae/graph-interface/bin/linux-x64-106/graph-interface.node`
- TypeScript: `re/extracted-asar/node_modules/@saleae/graph-interface/dist/instance.d.ts`
- TypeScript: `re/extracted-asar/node_modules/@saleae/graph-interface/dist/fs.d.ts`
- TypeScript: `re/extracted-asar/node_modules/@saleae/graph-interface/dist/util.d.ts`
- JavaScript: `re/extracted-asar/node_modules/@saleae/graph-interface/dist/instance.js`
- JavaScript: `re/extracted-asar/node_modules/@saleae/graph-interface/dist/fs.js`
- JavaScript: `re/extracted-asar/node_modules/@saleae/graph-interface/dist/util.js`
- Package: `re/extracted-asar/node_modules/@saleae/graph-interface/package.json`
- Cross-ref: `re/notes/ipc-analysis.md`
- Cross-ref: `doc/protocol/ipc.md`

## Implications for RE

1. **No need to reverse this addon further** -- it is fully understood. The 8 C
   API functions are the complete interface between Electron and the native library.

2. **Focus shifts to the C API consumers in libgraph_server_shared.so** -- specifically
   the implementations of `CreateGraphServer`, `DriveRequest`, and `PopResponse`.
   These are the entry points into the 48MB stripped binary.

3. **For a Rust replacement**, the equivalent of this addon is trivially simple:
   just dlopen + dlsym + string passing. The complexity is entirely inside the .so.

4. **The JSON messages** flowing through `DriveRequest`/`PopResponse` are the same
   ones documented in `re/notes/ipc-analysis.md` (Saleae::Graph::* types).

5. **The mutex on DriveRequest** means the native library may not be thread-safe
   for request submission. However, PopResponse uses atomic refcounting, suggesting
   the response queue IS thread-safe for reading.
