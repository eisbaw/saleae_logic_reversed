---
id: TASK-0086
title: RE task_executor and mailbox -- threading model
status: Done
assignee: []
created_date: '2026-03-10 12:32'
updated_date: '2026-03-10 20:04'
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
- `/workspace/monorepo/graph-io/core/task_executor/include/mailbox.h`
- `/workspace/monorepo/graph-io/core/task_executor/src/task_executor.cpp`

The threading infrastructure used by the processing graph. Reveals:
- Thread pool design and sizing
- Mailbox (message-passing) pattern between nodes
- Task scheduling and priority
- How streaming data is handed off between threads without blocking

Investigation steps:
1. Search for strings like "task_executor", "mailbox", "thread"
2. Decompile the executor and mailbox classes
3. Document the concurrency model
4. Check if this explains any timing-sensitive behavior in captures

Low priority -- internal concurrency plumbing. Only matters if we see threading issues in our Rust driver.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
TaskExecutor (ELF 0x2ce0680): Thread pool sized by SALEAE_THREAD_COUNT env var, fallback to hardware_concurrency(). Two-queue design: immediate FIFO + scheduled (deadline-based) with time-based promotion. Task deque uses 85-element chunks, 48 bytes/entry. Two-lock queue pattern. 6-state lifecycle: Disabled(0), Waiting(1), WaitingDisabled(2), Running(3), RunningDisabled(4), Cancelled(5).\n\nMailbox<T>: Actor pattern with thread affinity enforcement via pthread_self() comparison. Batches all queued messages before yielding. One visible instantiation: MsoDevice with 8 variant message types.\n\nRust driver does NOT need to replicate TaskExecutor -- it's internal graph plumbing. USB I/O, serialization, analysis, and subscription polling each have dedicated threads.\n\nAnalysis: re/notes/task-executor.md (350+ lines)"
<!-- SECTION:FINAL_SUMMARY:END -->
