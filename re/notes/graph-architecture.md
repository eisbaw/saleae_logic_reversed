# Processing Graph Architecture: Graph, Node, SubscriptionManager

Reverse engineered from `libgraph_server_shared.so` (Saleae Logic 2 v2.4.40).

Source paths leaked via assertions:
- `/workspace/monorepo/graph-io/core/graph/src/graph.cpp`
- `/workspace/monorepo/graph-io/core/graph/src/subscription_manager.cpp`
- `/workspace/monorepo/graph-io/core/node/src/node.cpp`

## Overview

The Saleae Logic 2 native library implements a **directed acyclic graph (DAG)**
processing architecture. All data processing -- from USB device input through
disaggregation, calibration, storage, analysis, and rendering -- flows through
this graph. The graph is constructed dynamically based on the capture
configuration.

Key components:
1. **Graph** -- the container; owns nodes, pipes, ports, and subscriptions
2. **Node** -- a processing unit with a kernel, pipe dock, and execution state
3. **Kernel** -- the actual processing logic (ConvenienceKernel or custom)
4. **Pipe** -- a typed data connection between nodes
5. **Port** -- named input/output endpoints on the graph boundary
6. **SubscriptionManager** -- polls for state changes and notifies observers

## Graph Class

**Namespace**: `Saleae::Graph::Graph`

### Graph Object Layout (partial)

Reconstructed from decompiled AddNode, RemoveNode, AddPipe, SyncNodeState:

| Offset | Size | Type | Field | Description |
|--------|------|------|-------|-------------|
| 0x10 | 8 | ptr | mKernelRegistry | Registry of kernel types |
| 0x18 | 24 | map<NodeId, Node*> | mSubscribers | Subscriber linked list sentinel |
| 0x20 | 8 | ptr | mSubscribers.begin | Tree root |
| 0x28 | 8 | ptr | mSubscribers.sentinel | End sentinel |
| 0x28 | 24 | map<NodeId, Node*> | mNodes | Red-black tree of nodes by ID |
| 0x30 | 8 | size_t | mNodes.size | Number of nodes |
| 0x38 | 24 | map<NodeId, ?> | mNodeStates | Red-black tree of node states |
| 0x40 | 8 | ptr | mNodeStates.root | Tree root |
| 0x48 | 8 | size_t | mNodeStates.size | Number of state entries |
| 0x50 | 8 | ptr | mTaskQueue | Task execution queue |
| 0x60 | 40 | mutex | mExecutionMutex | Guards execution state |
| 0x88 | 4 | int | mPendingTaskCount | Incremented for each queued task |
| 0x8c | 1 | bool | mShutdown | If true, skip task dispatch |
| 0x90 | 8 | ptr | mDispatcher | Redux dispatcher for task execution |
| 0xc0 | ? | ? | mNodeNameMap | Map from node name to ID (FUN_01bbb5f0) |
| 0x1d0 | 8 | ptr | mPipes | Linked list of pipe connections |
| 0x1e8 | 24 | hash_map<PipeId, Pipe> | mApiConnections | Hash map of API pipe connections |
| 0x218 | 8 | ptr | mInputPorts | Tree of graph input ports |
| 0x230 | 8 | ptr | mOutputPorts | Tree of graph output ports |
| 0x240 | 40 | mutex | mSubscriptionMutex | Guards subscription map |
| 0x268 | ? | map | mSubscriptions | Subscriptions map |
| 0x290 | 4 | atomic<int> | mStateVersion | Incremented on structural change |
| 0x298 | 8 | ptr | mSubscriptionNotifier | Notified on state version change |
| 0x2a0 | 8 | uint64_t | mNextId | Auto-incrementing ID counter |

Node ID 0 is reserved for the Graph itself. The mNextId counter starts at 1
and is incremented for each AddNode call (unless a specific ID is provided).

### Key Functions

| ELF VA | Ghidra VA | Function | Description |
|--------|-----------|----------|-------------|
| 0x1ab2830 | 0x1bb2830 | Graph::AddNode | Create node with kernel, insert into mNodes tree |
| 0x1ab12f0 | 0x1bb12f0 | Graph::RemoveNode | Remove node, clean up pipes and state |
| 0x1ab1fe0 | 0x1bb1fe0 | Graph::AddPipe | Connect a pipe between two nodes |
| 0x1ab4dc0 | 0x1bb4dc0 | Graph::SyncNodeState | Sync node state by ID, asserts node exists |
| 0x1aaffa0 | 0x1baffa0 | Graph::ConnectPort | Connect graph input/output ports |
| 0x1abcb40 | 0x1bbcb40 | Graph::CreateKernel | Look up kernel type by name, instantiate |
| 0x1abb870 | 0x1bbb870 | SubscriptionManager::Poll | Poll for state changes, notify subscribers |
| 0x1ab3020 | 0x1bb3020 | Graph::SyncNodeStateInner | Inner implementation of state sync |
| 0x1a3ab90 | 0x1b3ab90 | Graph::LogAction | Log "Graph Actions: " with action details |

## Graph::AddNode (ELF 0x1ab2830)

Signature (reconstructed):
```cpp
NodeId Graph::AddNode(
    const std::string& node_type,    // kernel type name
    const std::string& node_name,    // display name
    std::optional<NodeId> id         // explicit ID or auto-assign
);
```

Flow:
1. If no explicit ID provided, auto-assign from `mNextId++` (offset 0x2a0)
2. Assert ID != 0 ("A node cannot be given Id 0; this Id is reserved for the Graph")
3. Search mNodes tree (offset 0x28) for duplicate ID
4. If duplicate found, throw "Node with specified id already exists: {id}. trying to add: {name}"
5. Look up kernel type via CreateKernel (registry at offset 0x10)
6. Allocate Node object (0x218 bytes via `operator new`)
7. Initialize Node with DependencyInjector and kernel
8. Insert into mNodes tree (node at offset 0x28 has key = NodeId at tree_node+0x20)
9. Lock mSubscriptionMutex (offset 0x240)
10. Insert into mSubscriptions map (offset 0x268) with empty subscription set
11. Unlock mutex
12. Sync the node state
13. Set node name
14. Register for subscription notifications
15. Insert into mNodeStates tree (offset 0x38, 0x60 byte entries for state, 0x30 byte for subscriber)
16. Return NodeId

The Node object is 0x218 bytes (536 bytes).

## Graph::RemoveNode (ELF 0x1ab12f0)

Signature (reconstructed):
```cpp
void Graph::RemoveNode(NodeId id);
```

Flow:
1. Call FUN_01bbb700 on mNodeNameMap (offset 0xc0) -- possibly clear name mapping
2. Search mNodes tree for the node by ID
3. If not found, throw "failed to locate node in graph to issue stop execution"
4. Stop the node's execution
5. Disconnect all node ports
6. Verify no pipe connections remain: walk mPipes linked list (offset 0x1d0),
   checking `pipe[3] == nodeId` or `pipe[8] == nodeId`. If any match,
   throw "Cannot remove node that still has pipe connections"
7. Clear from mNodeStates tree
8. Remove from mNodes tree
9. Lock mSubscriptionMutex, remove from mSubscriptions map, unlock
10. If not shutting down (offset 0x8c):
    - Increment mPendingTaskCount (offset 0x88)
    - Queue "Task (RemoveNode)" on the task queue (offset 0x50)
11. Increment mStateVersion (atomic int at offset 0x290)
12. Notify mSubscriptionNotifier (offset 0x298)

### Pipe Linked List

The mPipes linked list (offset 0x1d0) stores pipe connection records. Each pipe
record has at minimum:
- Offset 0 (field[0]): next pointer
- Offset 0x18 (field[3]): source NodeId
- Offset 0x40 (field[8]): destination NodeId

This is checked in RemoveNode to ensure all pipes are disconnected before a
node can be removed.

## Graph::AddPipe (ELF 0x1ab1fe0)

Signature (reconstructed):
```cpp
PipeId Graph::AddPipe(
    NodeId from_node,              // source node ID
    const std::string& port_name,  // output port name
    NodeId to_node,                // destination node ID
    NodeId to_node_target,         // target port identifier
    std::optional<PipeId> id       // explicit pipe ID or auto-assign
);
```

Flow:
1. If no explicit ID, auto-assign from mNextId++ (offset 0x2a0)
2. Look up mApiConnections hash map (offset 0x1e8) to check for duplicate PipeId
3. If duplicate, throw "Provided id already exists in mApiConnections"
4. Look up source node (from_node) and destination node (to_node) in mNodes tree
5. If either not found, throw "Node does not exist at the provided id"
6. Insert pipe record into mApiConnections hash map
7. Set pipe fields: from_node at offset 0x18, to_node at offset 0x20, port_name at 0x28
8. Connect source node's output port to destination node's input port

The mApiConnections hash map at offset 0x1e8 uses a typical open-addressing
hash table with power-of-2 bucket count and popcount-based probing optimization.

## Graph::ConnectPort (ELF 0x1aaffa0)

Signature (reconstructed):
```cpp
void Graph::ConnectPort(
    const PortDescriptor& port,  // port name, type, index
    int direction                // 0 = input, 1 = output
);
```

Flow:
1. If direction == 0 (input):
   - Check if the port is already registered in mInputPorts (offset 0x218)
   - If already occupied: throw "Graph input port is already occupied: {name} {index}"
   - Otherwise connect to the node's input port
2. If direction == 1 (output):
   - Check mOutputPorts (offset 0x230) for existing registration
   - If already occupied: throw "Graph output port is already occupied: {name} {index}"
   - Otherwise connect to the node's output port
3. If port references a specific NodeId:
   - Look up the node in mNodes tree (offset 0x28)
   - If not found: "Node with the specified mNodeId does not exist"
   - Attempt to register port on the node
   - If invalid: "Invalid port input" or "Invalid port output"

The port tree is a red-black tree keyed by (port_name, port_index), allowing
the graph to have multiple named input/output ports at its boundary.

## Node Class

**Namespace**: `Saleae::Graph::Node`

### Node Object Layout (partial, 0x218 bytes)

| Offset | Size | Type | Field | Description |
|--------|------|------|-------|-------------|
| 0x00 | 8 | ptr | vtable | Virtual method table |
| 0x08 | ? | string | mName | Node display name (at +0x8, SSO string) |
| 0x30 | 8 | long | mExecutionState | NodeExecutionState enum value |
| 0x58 | 1 | bool | mPipeDockTraceEnabled | Enable pipe data tracing to cout |
| 0xb8 | ? | ? | mNodeInfo | Node metadata (name accessible at +0xb8) |
| 0x1a0 | 24 | ? | mExternalPipeDock | Pipe dock for external connections |
| 0x1b0 | 8 | ptr | mConnectedOutputs.begin | Start of connected outputs vector |
| 0x1b8 | 8 | ptr | mConnectedOutputs.end | End of connected outputs vector |

Each connected output entry is 0x20 bytes (32 bytes) and contains:
- SSO string (port name, 0x18 bytes inline or heap-allocated)
- Connection metadata

### Node Constructor (from assertion)

The Node constructor signature (from PRETTY_FUNCTION in assertion):
```cpp
Saleae::Graph::Node::Node(
    Saleae::DependencyInjector,
    std::unique_ptr<Kernel>
)
```

The Node takes ownership of a Kernel via unique_ptr. The kernel contains the
actual processing logic.

### NodeExecutionState

The NodeExecutionState enum has at least 3 values, stored at Node offset 0x30:

| Value | Name | Description |
|-------|------|-------------|
| 0 | Invalid | Node is invalid/uninitialized |
| 1 | Stopped | Node is stopped, ready to start |
| 2 | Executing | Node is actively processing data |

Valid transitions (from FUN_02203da0 at ELF 0x2103da0):
- Stopped (1) -> Invalid (0): valid, used during node destruction
- Stopped (1) -> Executing (2): valid, used when starting capture
- Stopped (1) -> Stopped (1): INVALID, throws "Stopped->Stopped is not a valid..."
- Invalid (0) -> anything except Stopped: throws "Only Stopped->Invalid is a valid..."
- Executing (2) -> anything except Stopped: throws "Only Stopped->Executing is a valid..."
- Unknown value: throws "Unknown mExecutionState"

The state transition function performs a `dynamic_cast` to verify the action
type before applying the transition. The node's name string (at offset +0x8)
is included in error messages for debugging.

### Pipe Dock and Data Flow

The pipe dock (at Node offset 0x1a0) manages the data connections between nodes.
When data arrives on a pipe, the dock dispatches it to the registered pipe
handler based on the port name.

From FUN_021fce60 (ELF 0x20fce60), the pipe data dispatch works as follows:

1. Receive data with: pipe_name (string), data_id (uint64_t), data (shared_ptr<Data>)
2. Look up the pipe name in the mExternalPipeDock hash map (offset 0x1a0)
3. If trace is enabled (offset 0x58), log: `"{node_name} -> [{pipe_name}, *] {dest_name}"`
   or `"{node_name} -> [{pipe_name}, {data_id}] {dest_name}"`
4. For each connected output matching the pipe name (iterating mConnectedOutputs
   at offsets 0x1b0-0x1b8, 0x20 bytes per entry):
   - Look up the connection in the pipe dock
   - If connection not found: assertion "A pipe in mConnectedOutputs should always
     exist in mExternalPipeDock", source: node.cpp line 0x88 (136)
   - If pipe dock tracing is disabled (offset 0x58):
     - Call FUN_02daa640 to deliver data to the connected node
   - If pipe dock tracing is enabled:
     - Check for PipeDone data type via dynamic_cast
     - If PipeDone: call FUN_02daa6f0 (special handling for completion)
     - Otherwise: deliver data normally

The assertion at node.cpp:136 confirms that mConnectedOutputs and
mExternalPipeDock must remain in sync -- every connected output must have a
corresponding entry in the pipe dock.

### Port Name Convention

From the assertion `port_name == "in"` (at 0x5186b8), referenced by
DigitalTriggerNode::HandleDigitalData, the standard input port name is `"in"`.

Other known port names from string analysis:
- `"in"` -- standard data input
- `"results-out"` -- output for results
- `"events-in"` -- input for events
- `"digital-render-requests"` -- render request input
- `"measurement-render-requests"` -- measurement render input

## Kernel Types

The kernel is the processing logic inside a Node. The Graph creates kernels by
type name via a registry (offset 0x10 of Graph).

From FUN_01bbcb40 (ELF 0x1abcb40), the CreateKernel function:
1. Look up the type name in the kernel registry (map at Graph+0x10, key=string)
2. If not found: throw "Unknown kernel type: {name}"
3. If found: call the factory function via vtable (virtual method at offset 0x30)
4. Return the created kernel

Known kernel type names (from prior analysis in graph-server.md):
- `"Graph"` and `"graph"` (reserved, value 0 and 1 in enum)
- All node types listed in graph-server.md (LogicDeviceNode, DisaggregatorNode, etc.)

## SubscriptionManager

**Namespace**: `Saleae::Graph::SubscriptionManager`

### SubscriptionManager::Poll (ELF 0x1abb870)

The SubscriptionManager runs a polling loop that:

1. Creates a tracing span with name "SubscriptionManager::Poll"
2. Computes poll interval: `1000 / poll_rate` milliseconds (from FUN_01bbb210)
3. Enters poll loop (while shutdown flag at offset +0x8 is not set):
   a. Lock mutex (offset 0x40)
   b. Walk the subscriber tree (offset 0x10, sentinel at offset 0x18)
   c. For each subscriber node:
      - Get the node's current state version (via FUN_021fccc0)
      - Look up the last known version in the tracking tree (offset 0x30)
      - If version has changed (current > tracked):
        - Compute the state delta (FUN_021fcc50)
        - Collect updates into a batch (red-black tree, 0x48 byte entries)
        - Each batch entry stores: NodeId (at +0x20 as int), state data (at +0x28),
          shared_ptr (at +0x30, +0x38, +0x40)
        - Update tracked version
   d. Unlock mutex
   e. If any updates were collected:
      - Build update list (convert batch tree to linked list)
      - Dispatch updates via the dispatcher (offset 0x90, virtual call at vtable+0x30)
   f. Sleep until next poll interval (FUN_01bbbef0)

This is the mechanism by which UI state updates are propagated. When a node's
state changes (e.g., capture progress, trigger hit, error), the
SubscriptionManager detects the version mismatch and dispatches notifications.

The poll rate is configured at startup and determines how frequently the UI
gets state updates. The interval is computed as `1000 / rate` ms, suggesting
the rate is in Hz (e.g., 30 Hz = 33ms poll interval).

## Threading Model

From the decompiled code, the threading model is:

1. **Main/Dispatch Thread**: Processes graph actions (AddNode, RemoveNode, AddPipe,
   etc.) via the Redux-like dispatcher. Graph structural modifications are
   serialized through mutexes.

2. **Node Task Threads**: Individual node operations (like RemoveNode cleanup)
   are dispatched as named tasks ("Task (RemoveNode)", "Task (PipeDock)")
   via the task queue (Graph offset 0x50). These run on a thread pool.

3. **Subscription Polling Thread**: SubscriptionManager::Poll runs in its own
   thread, polling at a configured rate. It acquires the subscription mutex
   (Graph offset 0x240) briefly while checking for state changes.

4. **Node Execution Threads**: When a node transitions to Executing state,
   its kernel runs on a dedicated thread or the node's assigned thread.
   The StoppedKernelAction is a special action type for stopped-state operations.

Key synchronization:
- `mExecutionMutex` (offset 0x60): guards task dispatch and shutdown flag
- `mSubscriptionMutex` (offset 0x240): guards subscription map access
- `mStateVersion` (offset 0x290): atomic counter, incremented on structural changes
- Per-pipe mutex in FUN_02daa640: guards individual pipe data delivery

## Graph Actions (JSON -> Native)

Graph actions are dispatched from the JS side and processed by the native Graph.
From the strings and demangled symbols, these are the graph-level actions:

| Action | Namespace | Purpose |
|--------|-----------|---------|
| AddNode | Saleae::Graph::GraphActions::AddNode | Add a processing node |
| RemoveNode | Saleae::Graph::GraphActions::RemoveNode | Remove a node |
| AddPipe | Saleae::Graph::GraphActions::AddPipe | Connect nodes with a pipe |
| RemovePipe | Saleae::Graph::GraphActions::RemovePipe | Disconnect a pipe |
| ConnectApi | Saleae::Graph::GraphActions::ConnectApi | Connect an external API |
| DisconnectApi | Saleae::Graph::GraphActions::DisconnectApi | Disconnect API |
| RouteAction | Saleae::Graph::GraphActions::RouteAction | Route action to specific node |

Each has a SerializeConverter specialization for JSON deserialization:
```
Saleae::SerializeConverter<Saleae::Graph::GraphActions::AddNode>
Saleae::SerializeConverter<Saleae::Graph::GraphActions::RemoveNode>
Saleae::SerializeConverter<Saleae::Graph::GraphActions::AddPipe>
Saleae::SerializeConverter<Saleae::Graph::GraphActions::RemovePipe>
Saleae::SerializeConverter<Saleae::Graph::GraphActions::ConnectApi>
Saleae::SerializeConverter<Saleae::Graph::GraphActions::DisconnectApi>
```

RouteAction is special: it routes a nested action to a specific node by nodeId.
From the string "RunInParallel can only be used with RouteAction", we know
that parallel action execution is only supported for RouteAction.

## Node-Level Actions

In addition to graph-level actions, each node handles its own actions:

| Action | Purpose |
|--------|---------|
| NodeActions::SetName | Set the node's display name |
| NodeActions::Connect | Connect node to a device/resource |
| NodeActions::Disconnect | Disconnect from device/resource |
| NodeActions::StartSerialization | Begin saving data to disk |
| NodeActions::StartDeserialization | Begin loading data from disk |
| NodeActions::StoppedKernelAction | Action valid only in Stopped state |

## Data Flow During Capture

Based on the pipe handler registrations (from graph-server.md) and the pipe
dock mechanism (from this analysis):

```
                    JS (Electron)
                         |
                    [submitActions / RouteAction]
                         |
                         v
                  +------Graph------+
                  |                 |
                  v                 v
          DeviceListNode     LogicDeviceNode
                              (USB I/O)
                                 |
                        OwnedSpan<uint8_t>
                                 |
                                 v
                        DisaggregatorNode
                     (split interleaved data)
                         /            \
                        /              \
              DigitalData          VectorData<int16_t>
                   |                      |
                   v                      v
             DigitalStore      AnalogCalibrationFilter
                   |              (apply cal)
                   |                      |
                   v                      v
           DigitalTriggerNode        AnalogStore
                   |                      |
                   v                      v
           DigitalRenderer       AnalogTriggerNode
                   |                      |
                   v                      v
           RenderingManagerNode  AnalogRenderer
                                          |
                                          v
                                  RenderingManagerNode
```

Each arrow represents a pipe connection with typed data. The pipe dock on each
node matches incoming data by port name and dispatches to the appropriate
handler.

## DAG Topology for a Typical Digital+Analog Capture

The graph is constructed by the JS side (bundle.js) when a capture starts.
The approximate sequence of AddNode/AddPipe calls:

1. AddNode("DeviceListNode", "device-list")
2. AddNode("LogicDeviceNode", "device-0", nodeId=N)
3. AddNode("DisaggregatorNode", "disaggregator-0")
4. AddPipe(from=N, port="raw-data", to=disaggregator)
5. For each digital channel:
   - AddNode("DigitalStore", "digital-store-ch{i}")
   - AddPipe(disaggregator -> digital-store, port name by channel)
   - AddNode("DigitalRenderer", "digital-renderer-ch{i}")
   - AddPipe(digital-store -> digital-renderer)
6. For each analog channel:
   - AddNode("AnalogCalibrationFilter", "cal-filter-ch{i}")
   - AddPipe(disaggregator -> cal-filter)
   - AddNode("AnalogStore", "analog-store-ch{i}")
   - AddPipe(cal-filter -> analog-store)
   - AddNode("AnalogRenderer", "analog-renderer-ch{i}")
   - AddPipe(analog-store -> analog-renderer)
7. AddNode("DigitalTriggerNode", "digital-trigger")
8. AddPipe(relevant digital-store -> digital-trigger)
9. AddNode("RenderingManagerNode", "rendering-manager")
10. AddPipe(all renderers -> rendering-manager)

The exact field names in the AddNode/AddPipe JSON messages are from the
SerializeConverter specializations, which we have not fully decompiled.
The structure is likely:
```json
{"type": "Saleae::Graph::GraphActions::AddNode",
 "nodeType": "LogicDeviceNode",
 "name": "device-0",
 "id": 42}
```

## Relationship to Other Components

### Redux Store

The Graph integrates with the Redux-like state management system:
- Actions are dispatched through `Redux::ReduxStore::Dispatcher`
- State changes increment `mStateVersion`
- SubscriptionManager polls for state changes and dispatches updates
- The "Task (Redux::NotifySubscribers)" string confirms Redux-triggered notifications

### ConvenienceKernel

The ConvenienceKernel is the base class for most node kernels. It provides:
- `RegisterRequestHandler<Request, Response>()` -- register action handlers
- `RegisterPipeHandler<DataType>()` -- register pipe data handlers
- `PipeHandlerOptions` -- configuration for pipe handler behavior

Nodes that use ConvenienceKernel register their handlers during initialization,
creating the typed connections documented in graph-server.md.

### Named Pipes

Named pipes (from `named_pipe_unix.cpp`) are used for streaming data out of
the graph to external consumers (e.g., terminal mode, real-time data export).
The "Task (PipeDock)" string suggests pipe dock operations are also dispatched
as tasks.

## Confidence Levels

- **High**: Graph object layout offsets (directly visible in decompiled code
  with consistent usage across AddNode, RemoveNode, AddPipe, SyncNodeState)
- **High**: Node execution state enum values and valid transitions (from
  explicit error messages)
- **High**: Graph action types (from demangled symbols and SerializeConverter)
- **High**: Pipe dock data dispatch mechanism (from assertion text and code flow)
- **Medium**: Exact Node object size of 0x218 bytes (from operator_new call in AddNode)
- **Medium**: Threading model (inferred from mutex usage and task dispatch patterns)
- **Medium**: Port naming convention (from assertion "port_name == \"in\"" and
  string analysis)
- **Low**: Exact JSON field names for graph actions (would require decompiling
  the SerializeConverter specializations)
- **Low**: Poll rate configuration (inferred from computation pattern, not
  directly observed)

## Key Functions Decompiled

| ELF VA | Purpose | Source File |
|--------|---------|-------------|
| 0x1ab2830 | Graph::AddNode | graph.cpp |
| 0x1ab12f0 | Graph::RemoveNode | graph.cpp |
| 0x1ab1fe0 | Graph::AddPipe | graph.cpp |
| 0x1ab4dc0 | Graph::SyncNodeState | graph.cpp |
| 0x1aaffa0 | Graph::ConnectPort | graph.cpp |
| 0x1abcb40 | Graph::CreateKernel | graph.cpp |
| 0x1abb870 | SubscriptionManager::Poll | subscription_manager.cpp |
| 0x20fce60 | Node::HandlePipeData | node.cpp |
| 0x2103da0 | Node::TransitionExecutionState | node.cpp |
| 0x1ae0dd0 | DigitalTriggerNode::HandleDigitalData | digital_trigger_node.cpp |
| 0x1a3ab90 | GraphServer::LogAction | graph_server.cpp |

## Files Analyzed

- Binary: `libgraph_server_shared.so` (Ghidra decompilation, string analysis)
- Prior work: `re/notes/graph-server.md` (node types, pipe handlers, request handlers)
- Prior work: `re/notes/packed-data-collection.md` (memory allocation backing data flow)
- Prior work: `re/notes/waveform-persistence.md` (WaveformStoreNode serialization)
- Ghidra script: `re/ghidra/scripts/DecompileGraphArch2.java`
