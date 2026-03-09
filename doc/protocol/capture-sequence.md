# Capture Start/Stop Command Sequence

Status: High confidence (static analysis of libgraph_server_shared.so v2.4.40)
Corroboration: Multiple independent evidence sources (RTTI/mangled symbols,
assertion messages, format strings, function signatures, IPC protocol analysis)
Awaiting: USB capture validation, Ghidra decompilation for exact command bytes
and register values


## Overview

Capture in Saleae Logic 2 follows a strict sequence: configure the device
(sample rate, channels, thresholds), program the FPGA with capture parameters,
send a start command to the FX3 microcontroller, then begin reading data from
EP2 IN via async bulk transfers. Stopping reverses this: send a stop command,
cancel in-flight USB transfers, and signal completion to the processing graph.

```
IPC: StartCapture
  |
  v
LogicDeviceNode::HandleStartCapture
  |
  +-- Validate: DSReady state, device acquired, no active session
  +-- Emit SamplingConfigurationData to graph
  +-- Emit DisaggregatorSettingsData to graph
  |
  v
LogicDevice::StartSampling(DeviceSettings &)   [virtual, per device type]
  |
  +-- SetBankPower()                 --> enable IO banks, wait for PLL lock
  +-- SetCaptureParameters()         --> write config blob to FPGA via EP1 OUT
  +-- TestFirmwareResponsivnessAndReseed()  --> verify FX3 is alive
  +-- ComputeTransferSize()          --> calculate optimal USB buffer size
  +-- SendStartCommand()             --> tell FX3 to begin data flow
  |
  v
ReadStream_QueueBuffersThread        --> submit async bulk reads on EP2 IN
  |
  v
Data flows: EP2 IN -> TransferQueue -> OnDataBuffer -> Graph pipeline
```


## Prerequisites (Configuration Before Capture)

Before `StartCapture` can be called, the device must be configured via
separate IPC messages. These are independent operations, not part of the
capture start sequence itself:

| Operation | IPC Type | Handler |
|-----------|----------|---------|
| Acquire device | `AcquireLogicDeviceRequest` | `HandleAcquireLogicDeviceRequest` |
| Set sample rate | `SetSampleRate` | `HandleSetSampleRate` |
| Enable channels | `EnableChannels` | `HandleEnableChannels` |
| Set voltage threshold | `SetDigitalVoltageThreshold` | `HandleSetDigitalVoltageThreshold` |
| Set ADC gain | `SetAdcFullScaleGain` | `HandleSetAdcFullScaleGain` |
| Set LED | `SetLed` | `HandleSetLed` |

The device must be in `DSReady` state and acquired by the requesting
session. Sample rate must be set and at least one channel must be enabled.


## Start Sequence Detail

### 1. HandleStartCapture (Graph Layer)

**Function**: `LogicDeviceNode::HandleStartCapture(const StartCapture &)`

**Guards** (abort with error if any fail):
- Device not in Ready state: "Cannot start capture while device is not in Ready state"
- Device not acquired: "start capture: device is not currently acquired"
- Session conflict: "start capture - session id already in use"

**Actions**:
1. Registers data callback (lambda `$_7`): `OnDataBuffer(DeviceId, Buffer)`
2. Registers error callback (lambda `$_8`): `OnError(DeviceId, const LogicError &)`
3. Registers completion callback (lambda `$_9`): `void()`
4. Emits `SamplingConfigurationData` to downstream graph nodes
5. Emits `DisaggregatorSettingsData` describing channel layout
6. Calls into device-level `StartSampling`

### 2. SetBankPower (Device Layer)

Enables the FPGA IO bank power for the channels that will be used.
The PLL must lock before capture can proceed.

**Success**: "bank enabled, PLL is locked."
**Failure**: "bank enabled, PLL IS NOT LOCKED" (capture aborts)

### 3. SetCaptureParameters (Device Layer)

**Function**: `Logic2FpgaDevice::SetCaptureParameters(DeviceSettings &)`

Programs the FPGA with the capture configuration:
1. Serializes configuration into a binary blob
2. Writes blob to FPGA via `WriteConfigurationData(const U8 *, U32)`
3. WriteConfigurationData sends data in chunks via EP1 OUT bulk writes
4. Reads back configuration to verify
5. Logs: "setting capture settings" / "reading back capture settings"

**Chunk size limits** (per USB speed):
- USB 2.0: max 62 bytes per WriteConfigurationData call
- USB 3.0: max 509 bytes per WriteConfigurationData call

**Failures**:
- "Capture parameter failed to download to device"
- "Failed to set capture settings"
- "Failed to write configuration data"

### 4. TestFirmwareResponsivnessAndReseed (Device Layer)

Verifies the FX3 firmware is responsive before starting capture:
- Performs a `ReseedDevice` operation on the bulk endpoint
- Uses `libusb_bulk_transfer` for a synchronous read/write test

**Failure**: "StartSampling failed on first connection test. error: {}"

### 5. ComputeTransferSize (Device Layer)

**Function**: `LogicDevice::ComputeTransferSize(U64)` (virtual)

Each device type computes its optimal USB bulk transfer size based on:
- USB speed (High Speed vs SuperSpeed)
- Sample rate and number of enabled channels
- USB bandwidth constraints

**Logs**:
- "ideal transfer size {}"
- "Transfer size is: {}"
- ", capping transfer size." (when bounded)

**Override**: `SALEAE_USB_TRANSFER_LIMIT` environment variable

### 6. SendStartCommand (USB Layer)

Sends the start command to the FX3 firmware via EP1 OUT. This tells
the FX3 to begin forwarding FPGA sample data to EP2 IN.

**Logs**:
- "Sending start command"

**Errors**:
- "Start command error. Error: {}; CY Error: {}"
- "Failed to send start command to device."
- "Start command failed to execute on device"

The "CY Error" field reports the Cypress FX3 firmware's error code,
indicating the failure happened on the device side.

### 7. BulkReadStream Start (USB Layer)

After the start command succeeds, the async read stream begins:

**Function**: `LinuxUsbDevice::ReadStream_QueueBuffersThread`

1. Allocates DMA buffers (`libusb_dev_mem_alloc`) for zero-copy transfers
2. Allocates USB 3.0 bulk streams (`libusb_alloc_streams`)
3. Submits initial batch of async bulk transfers (`libusb_submit_transfer`)
4. Enters event processing loop

**Log**: "Starting bulk read thread, transfer_size={}, timeout={}"

The `TransferQueue` manages a pool of in-flight transfers with dynamic
queue sizing:
- "BulkReadThread: Waiting for memory to be available and for the queue
   to have availability. queue_size={}, queue_capacity={}"


## SamplingConfigurationData

Emitted once at capture start, carries these fields:

```
Setting sampling config, downsample={}, pre={}, post={}, stats={}, chunk_size={}
```

| Field | Type | Description |
|-------|------|-------------|
| `downsample` | U32 | Downsample ratio (1 = no downsampling) |
| `pre` | S64 | Pre-trigger buffer length (samples, negative = before trigger) |
| `post` | S64 | Post-trigger buffer length (samples) |
| `stats` | bool | Whether per-packet statistics are included |
| `chunk_size` | U32 | Samples per packet/chunk (`mSamplesPerChunk`) |


## Configuration Constraints

### Packet Size (mSamplesPerChunk)

```
config.mSamplesPerChunk >= FpgaConstants::MinSamplesPerPacket_Altera
config.mSamplesPerChunk >= FpgaConstants::MinSamplesPerPacket_Xilinx
config.mSamplesPerChunk <= FpgaConstants::MaxSamplesPerPacket_Altera
config.mSamplesPerChunk <= FpgaConstants::MaxSamplesPerPacket_Xilinx
config.mSamplesPerChunk % FpgaConstants::SamplesPerPacketMultiple_WithStats == 0
```

The chunk size must satisfy all FPGA-specific bounds AND be aligned to
the stats multiple. The WithStats alignment reserves space at the end
of each packet for overflow counters and timing metadata.

### Trigger Buffer

```
( config.mPreTriggerLength + config.mPostTriggerLength )
    % FpgaConstants::SamplesPerPacketMultiple_WithStats == 0
-config.mPreTriggerLength < config.mPostTriggerLength
-config.mPreTriggerLength >= MsoConstants::MinTriggerBufferIndex_{Altera,Xilinx}
config.mPostTriggerLength <= MsoConstants::MaxTriggerBufferIndex
sample_length >= MsoConstants::MinTriggerBufferSamples
sample_length <= MsoConstants::MaxTriggerBufferSamples_{Altera,Xilinx}
```

Pre-trigger length is negative (samples before the trigger point).
The combined pre+post length must be packet-aligned.

### Holdoff and Timeout

```
config.mTriggerAuto_HoldoffSamples <= MsoConstants::MaxHoldoffSamples
config.mTriggerAuto_TimeoutSamples <= MsoConstants::MaxTimeoutSamples
config.mTriggerNormal_HoldoffSamples <= MsoConstants::MaxHoldoffSamples
config.mTriggerNormal_TimeoutSamples <= MsoConstants::MaxTimeoutSamples
config.mTriggerInitialHoldoffSamples <= MsoConstants::MaxInitialHoldoffSamples
```

### Analog Trigger

```
config.mTriggerHysteresisRising <= config.mTriggerThreshold
config.mTriggerHysteresisFalling >= config.mTriggerThreshold
isfinite( hardware_config.mTriggerThreshold )
```

### Channel Count

```
channel_count == 2 || channel_count == 4
sample_count % 8 == 0
```

Digital channels are captured in groups of 2 or 4. Sample counts are
always 8-aligned (byte-aligned 1-bit-per-sample packing).


## Stop Sequence Detail

### 1. HandleStopCapture (Graph Layer)

Validates capture is active and device is acquired.

### 2. SendStopCommand (USB Layer)

Sends `COMMAND_STOP_STREAM` to the FX3 via EP1 OUT.

**Logs**:
- "COMMAND_STOP_STREAM reply" (response from FX3)

**Errors**:
- "Failed to send stop command to device."
- "Failed to send STOP_STREAM command to device"
- "Stop command error. Error: {}; CY Error: {}"

### 3. Cancel Transfers

All in-flight async transfers on EP2 IN are cancelled:

```
Cancelling transfers
Cancelling {} id={} transfer_count={}
```

Uses `libusb_cancel_transfer` on each pending transfer.

### 4. Stop Read Stream

```
StopBulkReadStream
Stopping bulk read stream {}, block_until_stopped={}
stopping stream
```

The read thread exits its event loop and reports:
"Done with usb stream, {} successful transfers"

### 5. CaptureCompleteData

A `CaptureCompleteData` message propagates through the graph pipeline,
signaling to all processing nodes that capture has ended. This causes
stores to flush, triggers to finalize, and renderers to complete.


## Device-Specific Implementations

Each device type has its own `StartSampling` implementation:

| Device | Class | Notes |
|--------|-------|-------|
| Logic (original) | `LogicDevice` | FX2-based, base implementation |
| Logic 16 (legacy) | `Logic16Device` | FX2LP, separate impl |
| Logic 4 | `LogicStudentDevice` | FX3, custom start |
| Logic 8 | `LogicGraduateDevice` | FX3, custom start |
| Logic Pro 8/16 | `LogicProfessionalDevice` | FX3, shared Pro impl |

All FX3-based devices (Logic 4/8/Pro 8/Pro 16) share the same base
path through `Logic2FpgaDevice::SetCaptureParameters` but diverge in
`StartSampling` for device-specific initialization.


## USB Endpoint Usage During Capture

| Endpoint | During Capture | Direction |
|----------|---------------|-----------|
| EP0 (Control) | Not used during active capture | Bidi |
| EP1 OUT (0x01) | Config writes, start/stop commands | Host -> Device |
| EP1 IN (0x81) | Config readback, command responses | Device -> Host |
| EP2 IN (0x82) | High-speed sample data stream | Device -> Host |

EP2 IN operates with bMaxBurst=15 (USB 3.0), giving 16 x 1024 = 16,384
bytes per service interval at SuperSpeed.


## Error Recovery

### During StartSampling

Three-part error diagnosis:
```
StartSampling failed in either bank power, capture parameters,
  or start command. Error: {}
```

If the start command specifically fails:
```
failed to write capture parameters or send the FX3 start command.
  Note that the read thread has not started.
```

### During Active Capture

| Error | Cause | Recovery |
|-------|-------|----------|
| `LIBUSB_TRANSFER_ERROR` | USB transport error | Stream error, device may need reconnect |
| `LIBUSB_TRANSFER_STALL` | Endpoint stalled | Clear halt, retry |
| `LIBUSB_TRANSFER_NO_DEVICE` | Device disconnected | Fatal, device lost |
| `LIBUSB_TRANSFER_TIMED_OUT` | Transfer timeout | Retry or abort |
| `LIBUSB_TRANSFER_OVERFLOW` | Buffer overflow | Data lost |
| FIFO overflow | FPGA FIFO full | Samples lost, detected in packet metadata |

On unrecoverable error, the device transitions to `DSError` state, which
is terminal -- the device must be physically reconnected.


## Confidence Assessment

| Finding | Confidence | Sources |
|---------|-----------|---------|
| Start sequence: validate -> config -> start -> stream | **Confirmed** | Multiple correlated string sequences + mangled symbols |
| SetCaptureParameters writes via WriteConfigurationData | **Confirmed** | Function signatures + error strings + 62/509 limit |
| SendStartCommand / COMMAND_STOP_STREAM commands | **Confirmed** | Literal strings in binary |
| EP1 OUT for commands, EP2 IN for data | **Confirmed** | lsusb + binary analysis + burst config |
| 5 device-specific StartSampling variants | **Confirmed** | Virtual function signature strings |
| SamplingConfigurationData format fields | **Confirmed** | Format string in binary |
| CaptureCompleteData end signal | **High** | Type name + pipe handler registration |
| WriteConfigurationData chunk sizes (62/509) | **Confirmed** | Assertion string in binary |
| PLL lock check during SetBankPower | **Confirmed** | Both success/failure strings present |


## Open Questions

1. **Exact start/stop command bytes**: What is the binary format of
   SendStartCommand and COMMAND_STOP_STREAM? Are these specific byte
   values on EP1 OUT, or structured packets with headers?

2. **Configuration blob format**: What is the binary layout of the data
   written by SetCaptureParameters via WriteConfigurationData? This
   determines how sample rate, channel mask, and trigger config are
   encoded for the FPGA.

3. **FPGA register map**: Which specific register addresses control
   sample rate, channel enables, and trigger configuration? The format
   `FPGA WriteRegister 0x{:04x} = 0x{:08x}` shows 16-bit addresses
   and 32-bit values, but the address assignments are unknown.

4. ~~**Exact FpgaConstants values**~~: RESOLVED in TASK-0023.
   See `doc/protocol/capture-constants.md` for all extracted values.

5. **DisaggregatorSettingsData format**: How does this struct describe the
   channel interleaving layout in the USB stream?
