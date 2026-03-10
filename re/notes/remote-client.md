# RemoteClient Module: WebSocket IPC Transport

## Summary

The `remote_client` module implements a **WebSocket server** inside
`libgraph_server_shared.so` that provides an alternative transport for the
JSON-based IPC protocol (the same protocol used by the in-process named pipe
path). It is NOT a network device-sharing system, NOT a cloud client, and NOT
the automation API. It is the internal IPC mechanism used when the graph server
runs out-of-process from the Electron renderer.

## Source Path (from assertion)

```
/workspace/monorepo/graph-io/server/remote_client/src/remote_client.cpp
```

## Classes and Architecture

### Saleae::Detail::RemoteClientInternals

The core implementation class. Despite the "client" name, this is a **WebSocket
server**. The naming reflects the graph server's perspective: the Electron
renderer is the "remote client" connecting to it.

**Constructor signature** (from demangled RTTI):
```cpp
Saleae::Detail::RemoteClientInternals::RemoteClientInternals(
    std::function<void(const uint8_t*, size_t)>,  // on_message callback
    std::function<void()>,                          // on_connected callback
    std::function<void()>,                          // on_disconnected callback
    int                                             // port number
);
```

The constructor is called from `FUN_01b3d6f0` (ELF VA 0x1a3d6f0) inside the
`GraphServer` constructor when a port is specified.

### Saleae::Detail::(anonymous namespace)::StoppableSocketServer

A wrapper around the SimpleWeb socket server that adds clean shutdown support.
RTTI type name: `N6Saleae6Detail12_GLOBAL__N_121StoppableSocketServerE`

### RemoteClient::WebsocketEndpoint

An opaque endpoint identifier used to route responses back to the correct
WebSocket connection. Appears as a parameter in `EnqueueResponse()`.

### RemoteRequestHandler

A function inside `graph_server.cpp` (not `remote_client.cpp`) that dispatches
incoming JSON requests received over WebSocket. It parses the request type and
routes to the appropriate handler.

## Transport Details

### Library

Uses **Simple-WebSocket-Server** (a header-only C++ WebSocket library):
- `SimpleWeb::SocketServer<boost::asio::basic_stream_socket<boost::asio::ip::tcp, boost::asio::executor>>`
- Built on `boost::asio` with `boost::asio::ip::tcp`
- Plain TCP (no TLS), using WebSocket protocol upgrade

### Endpoint Configuration

- **Bind address**: `127.0.0.1` (localhost only, confirmed from decompilation at
  ELF VA 0x1a48fb4: `std::string::assign((string*)(param_1 + 0xe8), "127.0.0.1")`)
- **Port**: dynamically assigned (0 = OS-assigned), returned to caller
- **WebSocket paths**: Two endpoints registered:
  - `^/saleae/?$` -- main IPC endpoint (regex pattern, from string at file offset 0x3bdbeb)
  - `^/analyzer/?$` -- analyzer-specific endpoint (from string at file offset 0x3555c5)
- **URL**: `ws://localhost:{port}/saleae` (confirmed by bundle.js analysis)

### Startup Sequence

From decompilation of `FUN_01b48e60` (ELF VA 0x1a48e60), the
RemoteClientInternals constructor:

1. Stores the three callbacks (on_message, on_connected, on_disconnected)
2. Sets up the `StoppableSocketServer` via `FUN_01b497b0`
3. Creates a `std::promise<void>` for startup synchronization
4. Assigns bind address "127.0.0.1" to the server config
5. Sets the port number from the constructor parameter
6. Registers the `^/saleae/?$` path with the WebSocket server
7. Registers the `^/analyzer/?$` path with the WebSocket server
8. Registers four WebSocket event handlers:
   - `on_open` handler (calls the on_connected callback, logs "Server: Opened connection")
   - `on_close` handler (calls the on_disconnected callback, logs "Server: Closed connection (N) with error: ...")
   - `on_error` handler (logs "Server: Connection error (N) with error: ...")
   - `on_message` handler (calls the on_message callback with raw data)
9. Starts the server thread
10. Waits up to 1 second for the server to bind, using `std::future::wait_for(10000)`
    (10000 = 0x2710 in decompilation)
11. If startup times out, throws: "web socket server failed to start after 1 second."
12. On success, records whether the port was auto-assigned and stores the actual port

### Port Retrieval

`FUN_01b3d750` (ELF VA 0x1a3d750) returns the actual bound port:
```cpp
unsigned short GetPort(RemoteClientInternals* self) {
    return *(unsigned short*)(*(long*)self + 0x17a);
}
```

This port is returned to the caller of `CreateGraphServer` so the Electron
renderer knows where to connect.

## Integration with GraphServer

### CreateGraphServer

The exported `CreateGraphServer` function (ELF VA 0x1a01780) controls whether
WebSocket transport is enabled:

```
void* CreateGraphServer(uint* port_ptr, int queue_size, bool dev_mode)
```

- If `port_ptr` is NULL: no WebSocket server, in-process mode only
- If `port_ptr` is non-NULL:
  - If `*port_ptr == 0`: auto-assign port, start WebSocket server
  - If `*port_ptr != 0`: use specified port
  - After init, `*port_ptr` is set to the actual port number
- If WebSocket server is requested but fails to initialize, throws
  `std::runtime_error("websocket transport not enabled")`
- The GraphServer object is 0x380 bytes (allocated via `operator_new(0x380)`)
- The RemoteClientInternals pointer is stored at offset 0x2d8 within GraphServer

### GraphServer Constructor (FUN_01b02560, ELF VA 0x1a02560)

The constructor:
1. Initializes all GraphServer fields (0x380 bytes of state)
2. If a port was requested (`param_2 & 0xff00000000 != 0`):
   - Allocates 8 bytes for the RemoteClientInternals pointer
   - Calls `FUN_01b3d6f0` to create the RemoteClientInternals
   - Stores the pointer at `this + 0x2d8`
3. If no port requested: sets `this + 0x2d8 = 0` (no WebSocket)
4. Spawns two worker threads (FUN_01b0a2a0, FUN_01b0a340) for request processing
5. Sets up a signal handler and random number generator

### EnqueueResponse (FUN_01b07d20, ELF VA 0x1a07d20)

This function serializes a JSON response and delivers it:

```cpp
void EnqueueResponse(GraphServer* self,
                     rapidjson::Document& doc,
                     SerializeData& data,
                     RemoteClient::WebsocketEndpoint endpoint,
                     bool is_error);
```

1. Asserts `empty_root.IsNull()` (the document must start empty)
2. Adds `"type": "response"` or `"type": "error"` to the document
3. Serializes the SerializeData contents into the document
4. Serializes the complete JSON to a byte buffer
5. If WebSocket is active (`this + 0x2d8 != 0`):
   - Calls `FUN_01b3d740` which delegates to `FUN_01b3dca0` to send over WebSocket
6. If WebSocket is NOT active (in-process mode):
   - Copies the serialized data into a response queue
   - Locks a mutex at `this + 0x2e0`
   - Enqueues the response buffer into a deque at `this + 0x308`
   - Invokes any waiting callbacks

### RemoteRequestHandler (FUN_01b040c0, ELF VA 0x1a040c0)

This function handles incoming JSON messages received over WebSocket:

1. Parses the JSON document
2. Extracts `/meta` and `/type` fields from the request
3. Validates that either `node` or `sessionId` is present in the meta
4. Dispatches based on the request type by comparing against known type strings:
   - First type match: calls `FUN_01b05a00` (DriveRequest-equivalent, processes
     graph actions based on session ID lookup)
   - Second type match: calls `FUN_01b071b0` (PopResponse-equivalent)
   - Third type match: calls delete graph data handler (validates sessionId)
   - Fourth type match: calls forced crash handler (CauseCrash)
   - Fifth type match: starts/stops tracing (detached thread)
   - Sixth type match: another tracing variant
   - Default: converts type to string, logs "RemoteRequestHandler" with the
     unknown type, then calls `FUN_02eaee00` to clean up

The type dispatch uses a chain of `bcmp()` string comparisons, not a hash map.
This is consistent with the relatively small number of request types.

### DriveRequest Handler (FUN_01b05a00, ELF VA 0x1a05a00)

Processes graph actions for a specific session:

1. Validates `sessionId` is present (throws "Session Id mising from GraphActionData" [sic])
2. Locks the session mutex at `this + 0x88`
3. Looks up the session by ID in a red-black tree rooted at `this + 0xe8`
4. Uses `__dynamic_cast` to determine the specific action type (9 casts in chain):
   - Each cast corresponds to a different action subclass
   - Routes to the appropriate handler function
5. Builds a response document and calls `EnqueueResponse`

## WebSocket Event Handlers

### on_open (FUN_01b67590, ELF VA 0x1a67590)

Simple handler:
1. Logs "Server: Opened connection" via the logging system
2. Invokes the on_connected callback stored in the RemoteClientInternals

### on_close (FUN_01b67950, ELF VA 0x1a67950)

Connection close handler with error reporting:
1. Builds log message: `"Server: Closed connection (N) with error: E: S"`
   where N=connection ID, E=error code, S=error string
2. Logs via the logging system (referencing remote_client.cpp line 0x5d = 93)
3. Invokes the on_disconnected callback

### on_error (FUN_01b68970, ELF VA 0x1a68970)

Error handler with special case for suspend:
1. Builds log message: `"Server: Connection error (N) with error: S"`
2. Logs via the logging system (referencing remote_client.cpp line 0x68 = 104)
3. Checks if the error is `boost::asio::error::eof`:
   - If yes, logs "Boost ASIO asio.misc:eof error caught, caused by suspend, ignoring"
     (line 0x6d = 109)
   - This handles the case where a laptop suspend interrupts WebSocket connections
4. Otherwise, invokes the on_disconnected callback

### on_message

Invokes the on_message callback with the raw message data (uint8_t*, size_t).
This feeds directly into `GraphServer::ProcessRequest()`.

## Protocol

The WebSocket transport carries the **exact same JSON protocol** as the in-process
named pipe path. No additional framing or envelope is added by the WebSocket layer.
The JSON message format is documented in `re/notes/graph-server.md`.

Messages are WebSocket text frames containing JSON. The binary frames appear to
not be used (all serialization goes through RapidJSON text output).

## Relationship to Other Components

```
Electron Renderer (bundle.js)
  |
  | In-process mode:       Out-of-process mode:
  | graph-interface.node    WebSocket client
  | DriveRequest()          ws://localhost:{port}/saleae
  | PopResponse()           |
  |                         |
  v                         v
GraphServer (C++)           RemoteClientInternals (C++)
  |                           |
  |  <--- same JSON protocol -|
  |
  v
ProcessRequest() -> RemoteRequestHandler() -> action dispatch
  |
  v
EnqueueResponse() -> [response queue] or [WebSocket send]
```

The choice between in-process and WebSocket modes is made at `CreateGraphServer`
time:
- **In-process**: `graph-interface.node` calls `DriveRequest`/`PopResponse` directly
  via the C API (FFI). No WebSocket server is started. Responses go through a
  shared memory queue.
- **Out-of-process**: A WebSocket server is started. The Electron renderer connects
  as a WebSocket client to `ws://localhost:{port}/saleae`. Requests and responses
  flow over WebSocket as JSON text frames.

macOS appears to use the out-of-process mode (based on bundle.js references to
`useExistingGraph` and platform checks).

## Security Considerations

- Bound to `127.0.0.1` only -- not accessible from the network
- No authentication on the WebSocket endpoint
- No TLS (plain TCP)
- Port is dynamically assigned and communicated to the renderer via shared state

## Key ELF Virtual Addresses

| Function | ELF VA | Description |
|----------|--------|-------------|
| CreateGraphServer | 0x1a01780 | Exported: create server with optional WS |
| GraphServer::ctor | 0x1a02560 | Constructor body |
| RemoteRequestHandler | 0x1a040c0 | WS request dispatch |
| DriveRequest handler | 0x1a05a00 | Graph action dispatch |
| PopResponse handler | 0x1a071b0 | Response retrieval |
| HandleDeleteGraphRequest | 0x1a07af0 | Delete graph data |
| EnqueueResponse | 0x1a07d20 | JSON response serialization + delivery |
| GetWsPort | 0x1a3d750 | Returns bound WebSocket port |
| WsSend | 0x1a3d740 | Sends data over WebSocket |
| RemoteClientInternals::ctor | 0x1a48e60 | WS server setup |
| on_open handler | 0x1a67590 | Connection opened |
| on_close handler | 0x1a67950 | Connection closed |
| on_error handler | 0x1a68970 | Connection error |
| ws check/throw | 0x1a018c0 | Throws if WS not enabled |

## Key Strings

| File Offset | String |
|-------------|--------|
| 0x43b49d | /workspace/monorepo/graph-io/server/remote_client/src/remote_client.cpp |
| 0x389250 | websocket transport not enabled |
| 0x362c92 | RemoteRequestHandler |
| 0x3e6f06 | EnqueueResponse's empty root must be empty! |
| 0x3bdbeb | ^/saleae/?$ |
| 0x3555c5 | ^/analyzer/?$ |

## Confidence Level

**High** -- based on decompilation of 8 functions, string analysis, RTTI type
name demangling, and cross-reference with bundle.js analysis from TASK-0078.
The transport mechanism (WebSocket over TCP, JSON text frames, same protocol as
in-process path) is confirmed from multiple independent evidence sources.

## Implications

1. **Not relevant to USB RE**: This module is purely about IPC transport between
   the Electron renderer and the native graph server. It does not touch USB,
   device communication, or calibration.

2. **Confirms graph-server.md findings**: The WebSocket transport carries the
   same 47 messageType JSON protocol documented in graph-server.md.

3. **macOS architecture insight**: The out-of-process mode explains why macOS
   builds may behave differently -- the WebSocket hop adds latency and a
   potential failure mode (the "suspend" EOF handling).

4. **Automation integration**: The `/analyzer/?$` endpoint suggests there may
   be a separate path for analyzer-specific communication, though this could
   also just be a secondary endpoint for the same protocol.
