---
id: TASK-0083
title: RE graph.cpp and node.cpp -- processing graph architecture
status: Done
assignee: []
created_date: '2026-03-10 12:32'
updated_date: '2026-03-10 19:17'
labels:
  - re
  - ghidra
  - architecture
dependencies: []
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Source paths leaked:
- `/workspace/monorepo/graph-io/core/graph/src/graph.cpp`
- `/workspace/monorepo/graph-io/core/graph/src/subscription_manager.cpp`
- `/workspace/monorepo/graph-io/core/node/src/node.cpp`

The processing graph is the backbone connecting all nodes (device → disaggregator → trigger → store → render). Understanding it reveals:
- How nodes are connected and data flows between them
- Subscription/observer pattern for data propagation
- Graph construction for different capture configurations
- Threading model (which nodes run on which threads)

Investigation steps:
1. Search for strings like "graph", "node", "subscription", "connect"
2. Decompile the base Node class (virtual methods)
3. Map the subscription_manager publish/subscribe pattern
4. Understand graph construction during capture start

Lower priority -- architectural understanding, not needed for basic driver operation.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Processing graph architecture RE complete.\n\nGraph object (~0x2a0 bytes): mNodes as std::map<NodeId, Node*> (red-black tree), mPipes, mApiConnections (hash map), ports, mutexes, atomic state version counter.\n\nNode object (0x218 bytes): kernel pointer, pipe dock, execution state, node ID, parent graph back-pointer.\n\nNodeExecutionState enum: Invalid=0, Stopped=1, Executing=2 with validated transitions.\n\nPipe dock mechanism for typed data dispatch between nodes via mConnectedOutputs -> mExternalPipeDock mapping.\n\nSubscriptionManager polling loop with configurable rate and version-based change detection.\n\nGraph actions: AddNode, RemoveNode, AddPipe, ConnectPort, CreateKernel -- all decompiled.\n\nConvenienceKernel pattern with RegisterRequestHandler/RegisterPipeHandler.\n\nAnalysis: re/notes/graph-architecture.md (563 lines)"
<!-- SECTION:FINAL_SUMMARY:END -->
