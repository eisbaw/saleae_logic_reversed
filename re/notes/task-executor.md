# TaskExecutor and Mailbox: Threading Model

Reverse engineered from `libgraph_server_shared.so` (Saleae Logic 2 v2.4.40).

Source paths leaked via assertions:
- `/workspace/monorepo/graph-io/core/task_executor/include/mailbox.h`
- `/workspace/monorepo/graph-io/core/task_executor/src/task_executor.cpp`

## Overview

The Saleae Logic 2 native engine uses a custom thread pool called
`TaskExecutor` (namespace `Saleae::Tasks`) to run all graph processing tasks.
On top of this, a `Mailbox<T>` template (namespace `Saleae`) implements an
actor-style message queue pattern that guarantees single-threaded message
processing with thread affinity.

The design is a classic work-stealing/FIFO thread pool with:
- Configurable thread count (env var or hardware concurrency)
- Two task queues: immediate and scheduled (time-delayed)
- Condition variable based wakeup
- Cooperative shutdown via flag
- An "idle wait" mechanism for draining all tasks

## Namespace Hierarchy

```
Saleae::Tasks::TaskExecutor              -- thread pool + task dispatch
Saleae::Tasks::TaskHandle                -- handle to a running/pending task
Saleae::Tasks::Detail::TaskExecutorInternals  -- shared_ptr'd internals
Saleae::Tasks::Detail::TaskHandleInternals    -- shared_ptr'd per-task state
Saleae::Graph::TaskExecutorProvider      -- abstract provider interface
Saleae::Graph::DefaultTaskExecutorProvider    -- default impl
Saleae::Mailbox<T>                       -- actor mailbox template
```

## TaskExecutor

### Construction (ELF 0x2ce0680)

The `TaskExecutor` constructor determines thread count and spawns workers:

```
TaskExecutor::TaskExecutor(string name):
    1. Read env var SALEAE_THREAD_COUNT
    2. If env var is set:
         thread_count = stoi(SALEAE_THREAD_COUNT)
         Log: "SALEAE_THREAD_COUNT used to set thread count to {}"
       Else:
         thread_count = std::thread::hardware_concurrency()
    3. Allocate vector<thread_handle> of size thread_count (8 bytes each)
    4. Initialize internal state (mutexes, queues, counters)
    5. Call StartThreads()
```

The function that reads `SALEAE_THREAD_COUNT` is labeled `GetThreadCount` in the
binary's own log messages. This confirms the thread pool is sized either by an
explicit environment variable or by the number of hardware threads.

### Object Layout (partial, from TaskExecutorInternals)

Reconstructed from the worker thread loop and wait-for-idle function:

| Offset | Size | Type | Field | Description |
|--------|------|------|-------|-------------|
| 0x18 | 1 | bool | mShutdown | Cooperative shutdown flag |
| 0x20 | 40 | mutex | mMutex | Guards task queues |
| 0x48 | 48 | condition_variable | mCV | Worker wakeup |
| 0x78 | 8 | ptr | mImmediateQueue.begin | Immediate task vector start |
| 0x80 | 8 | ptr | mImmediateQueue.end | Immediate task vector end |
| 0x88 | 8 | ptr | mImmediateQueue.capacity | Immediate task vector capacity |
| 0x98 | 8 | ptr | mScheduledQueue.begin | Scheduled (timed) task start |
| 0xa0 | 8 | ptr | mScheduledQueue.end | Scheduled (timed) task end |
| 0xb8 | 8 | ptr | mThreadHandles.begin | Thread handle vector start |
| 0xc0 | 8 | ptr | mThreadHandles.end | Thread handle vector end |
| 0xd8 | 8 | long | mActiveThreadCount | Number of threads executing tasks |
| 0xe0 | 48 | condition_variable | mIdleCV | Wakes idle waiters |
| 0x110 | 8 | long | mIdleWaiterCount | Number of threads waiting for idle |

### Thread Startup (ELF 0x2ce0b10)

```
TaskExecutor::StartThreads():
    for each slot in mThreadHandles:
        assert(slot == 0)  // not already started
        create thread with WorkerThreadFunc, passing TaskExecutorInternals*
        store pthread_t into slot
```

Each thread is created with `pthread_create`, passing the `WorkerThreadFunc`
(ELF 0x2ce0c40) as the start routine. No thread attributes are set (default
stack size, detach state, etc.).

### Worker Thread Loop (ELF 0x2ce0c40)

This is the core loop each worker thread runs:

```
WorkerThreadFunc(TaskExecutorInternals* self):
    bVar4 = false  // "just_finished_task" flag
    loop:
        lock(self->mMutex)

        if bVar4:
            self->mActiveThreadCount--
            if self->mIdleWaiterCount != 0:
                notify_all(self->mIdleCV)
            bVar4 = false

            // Check scheduled queue first
            if scheduledQueue is not empty:
                goto process_scheduled

        // Wait loop (no tasks available)
        while not self->mShutdown:
            if immediateQueue or scheduledQueue non-empty:
                goto process_tasks
            wait(self->mCV, lock)

        // Shutdown requested
        unlock(lock)
        return

    process_scheduled:
        // Peek at first scheduled task's deadline
        deadline = scheduledQueue.front().time
        loop:
            now = steady_clock::now()
            if self->mShutdown: break
            if immediateQueue non-empty OR (scheduledQueue non-empty AND now >= deadline):
                break
            // Timed wait until deadline
            condition_variable::wait_until(self->mCV, lock, deadline)

    process_tasks:
        // Transfer mature scheduled tasks to immediate queue
        while scheduledQueue non-empty:
            now = steady_clock::now()
            if now < scheduledQueue.front().time:
                break  // not yet ready
            // Move task from scheduledQueue to immediateQueue
            move task to immediateQueue
            pop from scheduledQueue

        if immediateQueue is empty:
            continue  // back to wait loop

        // Dequeue one task from immediateQueue (front)
        task = immediateQueue.front()
        pop_front(immediateQueue)
        self->mActiveThreadCount++
        if self->mIdleWaiterCount != 0:
            notify_all(self->mIdleCV)
        unlock(lock)

        // Execute the task (unlocked!)
        task->Run()
        destroy task
        bVar4 = true  // set "just finished" flag
```

Key observations:
- **Two queues**: an immediate queue (offset 0x78) and a scheduled/timed queue
  (offset 0x98). Scheduled tasks have a deadline and are only moved to the
  immediate queue when their time arrives.
- **FIFO dispatch**: tasks are dequeued from the front.
- **Priority**: scheduled tasks are checked first (they have deadlines).
  However, there is no explicit priority field -- just time-based ordering.
- **Work is done unlocked**: the mutex is released before calling `task->Run()`,
  so tasks can themselves enqueue more tasks without deadlock.
- **Active thread counting**: `mActiveThreadCount` tracks how many threads are
  currently executing tasks. This is used by the idle-wait mechanism.

### Task Enqueue (ELF 0x2cac040)

```
TaskExecutor::EnqueueTask(TaskExecutorInternals* self, action):
    if action is nullptr:
        throw runtime_error("Action cannot be nullptr")

    // Create a future/promise pair for the caller to wait on
    promise = new std::promise<void>

    if self->mShutdown:
        promise.set_value()  // resolve immediately
        return future

    self->mPendingTaskCount++

    // Lock the task queue mutex (0x168) -- separate from the worker mutex!
    lock_guard<mutex>(self->mTaskQueueMutex)  // "locking mTaskQueueMutex"

    // Grow deque if at capacity (uses 0x55 = 85 elements per chunk)
    if queue is full:
        grow_deque()

    // Enqueue the task (deque push_back)
    // Task entry is 0x30 bytes: action ptr + promise + metadata
    enqueue(action, promise, task_name)

    // Notify workers (condition_variable::notify_one via FUN_02ddee90)
    notify_one_worker()

    unlock()
    return future  // caller can wait on this
```

The task queue is a **deque** (double-ended queue) with 0x55 (85) elements per
chunk. This is visible from the modular arithmetic in the dequeue code:
`(index % 0x55) * 0x30` and `index / 0x55 * 8` for chunk lookup.

Each task entry is **0x30 bytes** (48 bytes) and contains:
- The action (std::function<void(TaskHandle&)>)
- A promise for signaling completion
- A task name string (for logging)

### Wake-Up Mechanism (ELF 0x2cdee90)

```
TaskHandle::WakeUp():
    internals = self->mInternals
    if internals is null: return
    lock(internals->mStateMutex)  // at offset 0x50 of TaskHandleInternals

    switch(internals->mState):  // at offset 0x78
        case 4 (RunningDisabled): set to 3 (Running)
        case 2 (WaitingDisabled): set to 1 (Waiting)
        case 0 (Disabled):       set to 1 (Waiting); schedule on executor
    unlock()
```

### Disable/Cancel (ELF 0x2cdef70)

```
TaskHandle::Disable():
    internals = self->mInternals
    if internals is null: return
    lock(internals->mStateMutex)
    internals->mState = 5 (Cancelled)
    unlock()
    // Cancel the registration handle
    // Wait on the future if state was Running or RunningDisabled
    // Destroy the action closure
```

### Wait-for-Idle (ELF 0x2cdebb0)

```
TaskExecutor::WaitForIdle():
    lock(self->mMutex)
    self->mIdleWaiterCount++

    while true:
        log("Waiting for idle (tasks: {}, pending tasks: {}, active threads: {})",
            immediateQueue.size(), scheduledQueue.size(), self->mActiveThreadCount)

        if immediateQueue.empty()
           AND scheduledQueue.empty()
           AND self->mActiveThreadCount == 0:
            break

        condition_variable::wait(self->mIdleCV, lock)

    self->mIdleWaiterCount--
    unlock()
```

This provides a clean "drain and idle" mechanism. The log message at
`task_executor.cpp` line 0x6a (106) is the source of the
"Waiting for idle (tasks: {}, pending tasks: {}, active threads: {})" string.

### Task State Machine

From the `~TaskHandleInternals` destructor (ELF 0x2ce2700) which builds an
unordered_map of state names, the task states are:

| Value | Name | Description |
|-------|------|-------------|
| 0 | Disabled | Task is not scheduled |
| 1 | Waiting | Task is in queue, waiting to run |
| 2 | WaitingDisabled | Task is waiting but has been disabled |
| 3 | Running | Task is currently executing |
| 4 | RunningDisabled | Task is executing but was disabled during execution |
| 5 | Cancelled | Task was cancelled |

The destructor at ELF 0x2ce2700 checks that a task handle is in state 0
(Disabled) or 5 (Cancelled) when destroyed. If not, it logs:
```
"task handle destroyed that was not canceled or disabled: {state_name}"
```
This is a diagnostic warning, sourced from `task_executor.cpp` line 0x13a (314),
function `~TaskHandleInternals`.

State transitions for WakeUp:
```
Disabled (0)        -> Waiting (1)        [+ schedule]
WaitingDisabled (2) -> Waiting (1)        [already queued]
RunningDisabled (4) -> Running (3)        [still executing]
```

State transitions for Disable:
```
any state -> Cancelled (5)
```

### Cancel Callbacks (from `TaskHandle::RegisterCancelCallback`)

From the demangled string:
```
Graph::RegistrationHandle Saleae::Tasks::TaskHandle::RegisterCancelCallback(
    std::function<void()>
)
```

Tasks can register callbacks that are invoked when the task is cancelled. The
returned `RegistrationHandle` can be used to unregister the callback. This is
the mechanism by which the Mailbox knows when to stop processing messages.

## Mailbox<T>

### Design

The `Mailbox<T>` is a template class that implements an actor-style message
queue. It is parameterized by the message type `T`, which is typically a
`std::variant` of several message types.

From the demangled constructor signature:
```cpp
Saleae::Mailbox<T>::Mailbox(
    Tasks::TaskExecutor& executor,
    Mailbox<T>::HandleMailFn handler  // std::function<void(T)>
)
```

The Mailbox:
1. Takes a reference to a `TaskExecutor` for scheduling
2. Takes a handler function that processes each message
3. Stores the thread ID of the handler thread for affinity checking
4. Enqueues tasks on the executor with the name `"Task (Mailbox)"`

### Thread Affinity (from assertion)

The assertion `mMailbox.IsOnMailboxThread()` is checked at the start of every
`HandleMailbox` call. This confirms that the Mailbox enforces single-threaded
execution of message handlers. The check uses:

```cpp
bool IsOnMailboxThread() {
    pthread_t self = pthread_self();
    if (self == 0) return mThreadId == 0;
    return self == mThreadId;
}
```

This is visible in the decompiled `HandleMailbox` functions (e.g., ELF 0x2d0b660)
where `pthread_self()` is compared against a stored thread ID at offset 0x1b8
of the owning device object.

### Message Processing Pattern

From the `ProcessMailbox` string and the `HandleMailbox` function:

```
Mailbox<T>::Send(T message):
    lock(mailbox_mutex)
    push message to queue
    if not already scheduled:
        schedule ProcessMailbox task on TaskExecutor
    unlock()

ProcessMailbox (runs on TaskExecutor thread):
    store current thread ID for affinity check
    while queue not empty:
        message = dequeue()
        handler(message)   // calls HandleMailbox
    clear thread ID
```

The mailbox batches: once scheduled, it processes ALL queued messages before
yielding the thread. This is efficient for bursty message patterns.

### Known Mailbox Instantiations

Only one concrete `Mailbox<T>` instantiation is visible in the binary:

```cpp
Saleae::Mailbox<std::variant<
    Saleae::MsoDevice::FirmwareInterruptMessage,
    Saleae::MsoDevice::InitializeMessage,
    Saleae::MsoDevice::PollForInstrumentsMessage,
    Saleae::MsoDevice::HeartbeatMessage,
    Saleae::MsoDevice::InstrumentErrorEventMessage,
    Saleae::MsoDevice::AddFakeInstrumentMessage,
    Saleae::MsoDevice::RemoveFakeInstrumentMessage,
    Saleae::MsoDevice::StateMessage
>>
```

This is the `MsoDevice` mailbox. The handler `MsoDevice::HandleMailbox`
dispatches based on the variant index (firmware interrupt, smart cable
plug/unplug, thermal events, power events, fault codes, etc.).

The FX3-based Logic devices (Logic 4/8/Pro 8/Pro 16) likely have their own
mailbox instantiations for similar purposes, but the template is inlined and
does not appear in RTTI strings for those devices.

### MsoDevice Mailbox Messages

From the decompiled `HandleMailbox` (ELF 0x2d0b660), the message types and
their handling:

| Variant Index | Message Type | Handler Behavior |
|---------------|-------------|-----------------|
| 0 | FirmwareInterruptMessage | Dispatches on sub-type: smart cable plug/unplug, thermal events, power events, MSO faults, interrupt overflow |
| 1 | InitializeMessage | Triggers device initialization sequence |
| 2 | PollForInstrumentsMessage | Scans for connected instruments |
| 3 | HeartbeatMessage | Periodic health check |
| 4 | InstrumentErrorEventMessage | Handles instrument errors |
| 5 | AddFakeInstrumentMessage | Adds a simulated instrument |
| 6 | RemoveFakeInstrumentMessage | Removes a simulated instrument |
| 7 | StateMessage | State machine transition |

The FirmwareInterruptMessage has sub-types dispatched via nested switch:
- Sub-type 1: wall power events (unplugged, plugged pre-PD, PD contract
  established, insufficient PD power, voltage detected)
- Sub-type 2/3: smart cable events (plugged, unplugged on specific port)
- Sub-type 5: interrupt overflow (firmware event queue overflowed)
- Sub-type 6: thermal events (hot, critical, unknown)
- Sub-type 0xe: MSO firmware fault (no fault, unknown, thermal, fan malfunction,
  wall/PC power lost)

## Thread Pool Sizing

The default thread count is `std::thread::hardware_concurrency()`, which on
most Linux systems returns the number of logical CPUs. This can be overridden
by setting the `SALEAE_THREAD_COUNT` environment variable.

The thread pool is shared across ALL graph processing -- node initialization,
data pipe delivery, state synchronization, subscription notification, serialization,
and mailbox message processing all use the same pool.

From the graph architecture notes, additional dedicated threads exist:
- **Bulk read thread**: `"Starting bulk read thread, transfer_size={}, timeout={}"`
- **Serialization threads**: per-store (DigitalStore, AnalogStore, WaveformStoreNode)
- **Analysis worker threads**: `"AnalysisWorkerThread"` / `"InitialWorkerThread"`
- **SubscriptionManager poll thread**: dedicated polling loop
- **DeviceCollection worker thread**: with `WorkerThreadExitException`

So the TaskExecutor pool is for **graph-level** task dispatch, while certain
subsystems (USB I/O, disk I/O, analysis) maintain their own dedicated threads.

## Task Queue Implementation

The immediate task queue is a deque with:
- **Chunk size**: 0x55 (85) task entries per chunk
- **Entry size**: 0x30 (48) bytes per task
- **Chunk array**: a vector of pointers to chunks
- **Front/back indices**: separate start offset and count fields

Addressing: `chunk_ptr[index / 85] + (index % 85) * 48`

When the deque is full, `TaskQueue_Grow` (ELF 0x2cae3c0, 2890 bytes) is called
to allocate a new chunk and update the chunk pointer array. This is a substantial
function, suggesting the deque implementation handles edge cases like wrapping
and chunk reallocation carefully.

When an element is consumed from the front and the front pointer advances past
0xAA (170 = 2 * 85), the first chunk is freed:
```
if front_index >= 0xAA:
    free(chunks[0])
    chunks.erase(chunks.begin())
    front_index -= 0x55
```

## Synchronization Summary

| Primitive | Location | Purpose |
|-----------|----------|---------|
| mMutex (offset 0x20) | TaskExecutorInternals | Guards task queues and shutdown flag |
| mCV (offset 0x48) | TaskExecutorInternals | Wakes worker threads when tasks are enqueued |
| mIdleCV (offset 0xe0) | TaskExecutorInternals | Wakes idle waiters when active count drops |
| mTaskQueueMutex (offset 0x168) | TaskExecutorInternals | Guards the task deque during enqueue/dequeue |
| mStateMutex (offset 0x50) | TaskHandleInternals | Guards per-task state transitions |
| mShutdownMutex (offset 0x1a8) | TaskExecutorInternals | Guards shutdown + pending task count |
| condition_variable (offset 0x1d8) | TaskExecutorInternals | Shutdown completion notification |

There are **two separate mutexes** for the task queues:
1. `mMutex` (offset 0x20): used by worker threads to wait for work and dequeue
2. `mTaskQueueMutex` (offset 0x168): used by the enqueue path

This is a classic two-lock queue pattern that allows enqueueing without waking
sleeping workers unnecessarily, and allows workers to dequeue without blocking
producers.

## Relationship to Graph Architecture

The TaskExecutor is the execution engine underneath the Graph's processing
model:

1. **Graph dispatches named tasks**: When the graph needs to do work (remove a
   node, sync state, process a pipe), it enqueues a task with a descriptive name
   like `"Task (RemoveNode)"`, `"Task (PipeDock)"`, or `"Task (Mailbox)"`.

2. **TaskExecutorProvider**: The `Saleae::Graph::TaskExecutorProvider` is an
   abstract interface that provides TaskExecutor instances. The
   `DefaultTaskExecutorProvider` is the standard implementation.

3. **DependencyInjector**: The `DependencyInjector` passed to Node constructors
   includes access to the TaskExecutor, allowing nodes to schedule work.

4. **PythonTaskManager**: Python-based tasks (for custom analyzers) are also
   scheduled through the TaskExecutor via `SpawnCancelablePythonTask`. These
   tasks are cancellable and receive a `TaskHandle` for lifecycle management.

## Key Function Table

| ELF VA | Ghidra VA | Function | Source |
|--------|-----------|----------|--------|
| 0x2ce0680 | 0x2de0680 | TaskExecutor::Constructor (GetThreadCount) | task_executor.cpp |
| 0x2ce0b10 | 0x2de0b10 | TaskExecutor::StartThreads | task_executor.cpp |
| 0x2ce0c40 | 0x2de0c40 | WorkerThreadFunc (main loop) | task_executor.cpp |
| 0x2ce0220 | 0x2de0220 | TaskHandle::Notify (schedule on executor) | task_executor.cpp |
| 0x2ce11b0 | 0x2de11b0 | TaskExecutor::CreateThread (pthread wrapper) | task_executor.cpp |
| 0x2ce2700 | 0x2de2700 | ~TaskHandleInternals (destructor) | task_executor.cpp |
| 0x2cdee90 | 0x2ddee90 | TaskHandle::WakeUp (state transition) | task_executor.cpp |
| 0x2cdef70 | 0x2ddef70 | TaskHandle::Disable (cancel/stop) | task_executor.cpp |
| 0x2cdebb0 | 0x2ddebb0 | TaskExecutor::WaitForIdle | task_executor.cpp |
| 0x2cac040 | 0x2dac040 | TaskExecutor::EnqueueTask | task_executor.cpp |
| 0x2caba40 | 0x2daba40 | TaskExecutor::DequeueAndRun | task_executor.cpp |
| 0x2cac8c0 | 0x2dac8c0 | TaskQueue::Init (zero init) | task_executor.cpp |
| 0x2cae3c0 | 0x2dae3c0 | TaskQueue::Grow (deque expand) | task_executor.cpp |
| 0x2cad630 | 0x2dad630 | TaskQueue::LockGuard wrapper | task_executor.cpp |
| 0x2d0b660 | 0x2e0b660 | MsoDevice::HandleMailbox | mso_device.cpp |
| 0x2d105a0 | 0x2e105a0 | MsoDevice::RemoveInstrumentInternal | mso_device.cpp |
| 0x20f8120 | 0x21f8120 | Node::Initialize (creates Mailbox task) | node.cpp |

## Confidence Levels

- **High**: Thread pool design (SALEAE_THREAD_COUNT env var, hardware_concurrency
  fallback, worker loop structure). Directly visible in decompiled code.
- **High**: Task state enum values (state names are string literals in the
  destructor diagnostic message).
- **High**: Mailbox thread affinity (assertion text `mMailbox.IsOnMailboxThread()`
  and pthread_self comparison pattern visible in multiple functions).
- **High**: Two-queue design (immediate + scheduled) with time-based promotion.
  Clearly visible in the worker loop.
- **Medium**: Object layout offsets. Consistent across multiple functions but
  could have small errors in exact field boundaries.
- **Medium**: Deque chunk size of 85 (0x55). Based on modular arithmetic in
  decompiled code.
- **Low**: Exact locking protocol between mMutex and mTaskQueueMutex.
  The two-lock pattern is evident but the exact interaction needs more analysis.

## Implications for Rust Driver

For the Rust implementation (saleae-driver), this analysis tells us:

1. **No need to replicate the TaskExecutor** in the Rust driver. The TaskExecutor
   is graph infrastructure for the Electron app's processing pipeline. Our driver
   talks directly to USB and does not need a graph processing model.

2. **The mailbox pattern** is useful to understand for debugging timing issues.
   If the Saleae app drops messages, it is because the mailbox was overwhelmed,
   not because of USB-level issues.

3. **Thread count affects performance**. If running the Saleae app alongside the
   Rust driver for comparison, the `SALEAE_THREAD_COUNT` env var can be useful
   for controlling resource usage.

4. **USB I/O is on separate threads**. The `"Starting bulk read thread"` confirms
   that USB bulk transfers run on dedicated threads, not on the TaskExecutor pool.
   This is consistent with our Rust driver design using async USB I/O.

## Files Analyzed

- Binary: `libgraph_server_shared.so` (Ghidra decompilation, string analysis)
- Ghidra scripts: `re/ghidra/scripts/DecompileTaskExecutor.java`,
  `DecompileTaskExecutor2.java`, `DecompileTaskExecutor3.java`
- Prior work: `re/notes/graph-architecture.md` (graph threading model overview)
