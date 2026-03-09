# Capture Start/Stop Command Sequence Analysis

Analysis date: 2026-03-09
Binary: libgraph_server_shared.so (48MB, stripped, ELF 64-bit x86-64)
Source: Saleae Logic 2 v2.4.40
Hardware: Logic Pro 8 (21a9:1005) on nvdk-msdlab-09


## Summary

Capture is orchestrated through a multi-layer pipeline:

1. **IPC Layer** (Electron -> native): `StartCapture` / `StopCapture` JSON
   messages routed to `LogicDeviceNode` (node ID 4) via `GraphActions::RouteAction`.

2. **Graph Layer**: `LogicDeviceNode::HandleStartCapture` validates state,
   registers callbacks, then delegates to device-specific code.

3. **Device Layer**: `Logic2FpgaDevice::SetCaptureParameters` writes config
   to FPGA via register writes, then `LogicDevice::StartSampling` sends the
   FX3 start command and begins the USB read stream.

4. **USB Layer**: `TransferQueue` manages async bulk IN transfers on EP2
   (0x82). Stop is accomplished by cancelling in-flight transfers and
   sending a stop command via EP1 OUT.


## IPC Message Types

### StartCapture Request

Type: `Saleae::Graph::LogicDevice::StartCapture`
Direction: JS -> native (via `DriveRequest`)
Routed to: `LogicDeviceNode` (node 4) via `GraphActions::RouteAction`

The StartCapture message is handled by:
```
LogicDeviceNode::HandleStartCapture(const LogicDevice::StartCapture &)
```

Guards:
- "Cannot start capture while device is not in Ready state"
- "start capture: device is not currently acquired"
- "start capture - session id already in use"

Response type: `Saleae::Graph::LogicDevice::StartCaptureResponse`

### StopCapture Request

Type: `Saleae::Graph::LogicDevice::StopCapture`
Direction: JS -> native
Response type: `Saleae::Graph::LogicDevice::StopCaptureResponse`

Guards:
- "stop capture: device is not currently acquired"
- "can't release device while recording" (blocks ReleaseDevice during capture)


## Full Capture Start Sequence

### Step 1: HandleStartCapture (LogicDeviceNode)

Entry point from IPC. The handler:

1. Validates device is in `DSReady` state
2. Validates device is currently acquired by the requesting session
3. Validates session ID is not already in use for capture
4. Registers three lambda callbacks:
   - `$_7`: `OnDataBuffer(DeviceId, Buffer)` -- receives USB data from EP2 IN
   - `$_8`: `OnError(DeviceId, const LogicError &)` -- handles device errors
   - `$_9`: completion callback (void)
5. Emits `SamplingConfigurationData` to the graph pipeline
6. Emits `DisaggregatorSettingsData` to configure channel splitting
7. Delegates to device-level functions

### Step 2: SetCaptureParameters (Logic2FpgaDevice)

Writes capture configuration to the FPGA:

```
void Logic2FpgaDevice::SetCaptureParameters(DeviceSettings &)
```

This function:
1. Logs "setting capture settings"
2. Calls `FpgaDeviceFeatures::WriteConfigurationData(const U8 *, U32)` to
   send the configuration data blob to the FPGA via EP1 OUT bulk write
3. Logs "reading back capture settings" and verifies the readback
4. On failure: "Capture parameter failed to download to device"
5. On failure: "Failed to set capture settings"

Configuration data blob size constraint:
```
WriteConfigurationData does not support lengths larger than 62/509
```
This means:
- USB 2.0: max 62 bytes per WriteConfigurationData call
- USB 3.0: max 509 bytes per WriteConfigurationData call

These sizes match: 64 - 2 = 62 (USB 2.0 max packet minus header),
512 - 3 = 509 (USB 3.0 max packet minus header). The header likely
contains the FPGA register address or command type.

After writing, the response is read back:
"WriteConfigurationData response"

### Step 3: SetBankPower (device-specific)

Enables IO bank power for the enabled channels:

```
SetBankPower
```

Results:
- "bank enabled, PLL is locked." -- success, channel bank ready
- "bank enabled, PLL IS NOT LOCKED" -- PLL did not lock (error)

The PLL lock check ensures the FPGA's clock system is stable for the
requested sample rate before capture begins.

### Step 4: ComputeTransferSize (LogicDevice, virtual)

Computes the optimal USB bulk transfer buffer size:

```
virtual U32 LogicDevice::ComputeTransferSize(U64)
```

This is a virtual method with overrides per device type:
- LogicDevice (base)
- LogicGraduateDevice (Logic 8)
- LogicStudentDevice (Logic 4)
- LogicProfessionalDevice (Logic Pro 8/16)

Factors:
- USB speed (2.0 vs 3.0)
- Sample rate
- Number of enabled channels
- Available USB bandwidth

Logs:
- "ideal transfer size {}" -- computed optimal size
- "Transfer size is: {}" -- final size after constraints
- ", capping transfer size." -- when capped
- "SALEAE_USB_TRANSFER_LIMIT used to limit transfer size to {}" -- env override

The `AdjustTransferSize` function further tunes the size.

### Step 5: StartSampling (device-specific, virtual)

Each device type has its own implementation:

```
virtual void LogicDevice::StartSampling(DeviceSettings &)
virtual void LogicGraduateDevice::StartSampling(DeviceSettings &)
virtual void LogicProfessionalDevice::StartSampling(DeviceSettings &)
virtual void LogicStudentDevice::StartSampling(DeviceSettings &)
virtual void Logic16Device::StartSampling(DeviceSettings &)
```

This function:
1. Calls `SetBankPower` to enable channel IO banks
2. Calls `SetCaptureParameters` to program the FPGA
3. Performs a connection test: `TestFirmwareResponsivnessAndReseed`
4. Calls `SendStartCommand` to tell the FX3 to begin data flow

Error strings:
- "StartSampling failed in either bank power, capture parameters, or
   start command. Error: {}"
- "failed to write capture parameters or send the FX3 start command.
   Note that the read thread has not started."
- "StartSampling failed on first connection test. error: {}"

### Step 6: SendStartCommand

Sends the start command to the FX3 firmware:

```
SendStartCommand
```

This triggers data flow from the FPGA through the FX3 to EP2 IN.

Logs:
- "Sending start command"
- "startCommand" (field/label)

Errors:
- "Start command error. Error: {}; CY Error: {}"
  (CY Error = Cypress FX3 error code)
- "Failed to send start command to device."
- "Start command failed to execute on device"

The start command is sent via EP1 OUT (bulk write) or possibly a
vendor control transfer (exact mechanism needs Ghidra decompilation).

### Step 7: BulkReadStream Start

After the start command succeeds:

```
"Starting bulk read thread, transfer_size={}, timeout={}"
```

The `ReadStream_QueueBuffersThread` begins:
1. Allocates DMA buffers via `libusb_dev_mem_alloc` (zero-copy)
2. Allocates USB 3.0 streams via `libusb_alloc_streams`
3. Submits initial async bulk transfers via `libusb_submit_transfer`
4. Enters event loop processing completions via `on_libusb_transfer_complete`

Queue management:
- "BulkReadThread: Waiting for memory to be available and for the queue
   to have availability. queue_size={}, queue_capacity={}"
- "BulkReadThread: Waiting for read requests to be queued"

### Step 8: Data Flow Begins

Data arrives on EP2 IN and is processed through the graph pipeline:

```
EP2 IN -> on_libusb_transfer_complete -> OnDataBuffer($_7 lambda)
  -> emit SamplingConfigurationData (once, at start)
  -> emit DisaggregatorSettingsData (once, at start)
  -> emit VectorData<uint8_t> (continuous, raw USB data)
```

The first thing emitted is a start packet:
```
Got start packet: ch={} timestamp={}, time={}, config_id={}
```

Then data chunks flow continuously:
```
id={}, start={}, end={}, sample_count={}, config_id={}, ts={},
  software={}, cur_overflow={}, prev_overflow={}, samples={}
```


## Full Capture Stop Sequence

### Step 1: HandleStopCapture (LogicDeviceNode)

Entry point from IPC. The handler validates capture is active and the
device is acquired.

### Step 2: SendStopCommand

```
SendStopCommand
```

Sends a stop command to the FX3:
- "COMMAND_STOP_STREAM" -- the command identifier
- "COMMAND_STOP_STREAM reply" -- response from FX3
- "Failed to send stop command to device."
- "Failed to send STOP_STREAM command to device"
- "Failed to send the stop command to the device. the device might not
   be responding."
- "Stop command error. Error: {}; CY Error: {}"

The stop command is sent via EP1 OUT, telling the FX3 to stop
forwarding FPGA data to EP2 IN.

### Step 3: Cancel In-Flight Transfers

```
Cancelling transfers
Cancelling {} id={} transfer_count={}
```

All pending async transfers on EP2 IN are cancelled:
- `libusb_cancel_transfer` called on each in-flight transfer
- "libusb_cancel_transfer failed" if cancellation fails
- "A cancellation for an in-flight transfer hasn't completed but closing
   the device handle" -- edge case handling
- "A cancellation hasn't even been scheduled on the transfer for which
   the device is closing" -- edge case

### Step 4: StopBulkReadStream

```
StopBulkReadStream
Stopping bulk read stream {}, block_until_stopped={}
stopping stream
```

The read thread is signaled to stop and optionally blocked until done.

### Step 5: Stream Completion

```
Done with usb stream, {} successful transfers
```

The stream reports how many transfers completed successfully.

### Step 6: CaptureCompleteData

```
CaptureCompleteData
```

A `CaptureCompleteData` message is emitted through the graph pipeline,
signaling to all downstream nodes (DisaggregatorNode, DigitalStore,
AnalogStore, trigger nodes) that capture has ended.


## USB Command Channel (EP1 OUT / EP1 IN)

### WriteConfigurationData

Configuration data is written to the FPGA via bulk transfers on EP1 OUT:

```
void FpgaDeviceFeatures::WriteConfigurationData(const U8 *, U32)
```

The data is sent in chunks:
- USB 2.0: max 62 bytes per chunk
- USB 3.0: max 509 bytes per chunk

After each write, a response is read from EP1 IN:
"WriteConfigurationData response"

### ReadRegisters / WriteRegisters

FPGA register I/O for individual register access:

```
virtual std::vector<U8> FpgaDeviceFeatures::ReadRegisters(
    std::vector<U8> &addrs, const S8 *label)
virtual void FpgaDeviceFeatures::WriteRegisters(
    std::vector<std::pair<U8, U8>> &addr_val_pairs, const S8 *label)
void FpgaDeviceFeatures::WriteRegisterBit(
    U8 reg, U8 bit, U8 val, const S8 *label)
```

Log format:
- "FPGA ReadRegister 0x{:04X} = 0x{:08X}" -- 16-bit addr, 32-bit value
- "FPGA WriteRegister 0x{:04x} = 0x{:08x} ({})" -- with label

These are used for individual register access during configuration.
The `WriteConfigurationData` path is used for bulk configuration writes.

### Reseed

Device responsiveness check performed during capture setup:

```
TestFirmwareResponsivnessAndReseed
ReseedDevice(UsbEndpoint &)
```

Error: "libusb_bulk_transfer failure (reseed)"


## Capture Configuration Parameters

### SamplingConfigurationData

The sampling configuration carries these fields (from log format string):

```
Setting sampling config, downsample={}, pre={}, post={}, stats={}, chunk_size={}
```

- `downsample` -- downsample ratio (1 = no downsampling)
- `pre` -- pre-trigger buffer length (samples)
- `post` -- post-trigger buffer length (samples)
- `stats` -- whether per-packet statistics are included
- `chunk_size` -- samples per packet/chunk (`mSamplesPerChunk`)

### mSamplesPerChunk Constraints

```
config.mSamplesPerChunk >= FpgaConstants::MinSamplesPerPacket_Altera
config.mSamplesPerChunk >= FpgaConstants::MinSamplesPerPacket_Xilinx
config.mSamplesPerChunk <= FpgaConstants::MaxSamplesPerPacket_Altera
config.mSamplesPerChunk <= FpgaConstants::MaxSamplesPerPacket_Xilinx
config.mSamplesPerChunk % FpgaConstants::SamplesPerPacketMultiple_WithStats == 0
```

The chunk size must be:
1. Within FPGA-specific min/max bounds (Altera and Xilinx have different limits)
2. Aligned to `SamplesPerPacketMultiple_WithStats`

### Trigger Buffer Constraints

Pre/post trigger lengths:
```
( config.mPreTriggerLength + config.mPostTriggerLength )
    % FpgaConstants::SamplesPerPacketMultiple_WithStats == 0
-config.mPreTriggerLength < config.mPostTriggerLength
-config.mPreTriggerLength >= MsoConstants::MinTriggerBufferIndex_Altera
-config.mPreTriggerLength >= MsoConstants::MinTriggerBufferIndex_Xilinx
config.mPostTriggerLength <= MsoConstants::MaxTriggerBufferIndex
```

Sample length constraints:
```
sample_length >= MsoConstants::MinTriggerBufferSamples
sample_length <= MsoConstants::MaxTriggerBufferSamples_Altera
sample_length <= MsoConstants::MaxTriggerBufferSamples_Xilinx
```

Holdoff and timeout constraints:
```
config.mTriggerAuto_HoldoffSamples <= MsoConstants::MaxHoldoffSamples
config.mTriggerAuto_TimeoutSamples <= MsoConstants::MaxTimeoutSamples
config.mTriggerNormal_HoldoffSamples <= MsoConstants::MaxHoldoffSamples
config.mTriggerNormal_TimeoutSamples <= MsoConstants::MaxTimeoutSamples
config.mTriggerInitialHoldoffSamples <= MsoConstants::MaxInitialHoldoffSamples
```

Analog trigger constraints:
```
config.mTriggerHysteresisRising <= config.mTriggerThreshold
config.mTriggerHysteresisFalling >= config.mTriggerThreshold
isfinite( hardware_config.mTriggerThreshold )
```

### DeviceSettings

The `DeviceSettings` struct bundles all configuration:

```
virtual DeviceSettings LogicGraduateDevice::GetDeviceSettings()
virtual DeviceSettings LogicProfessionalDevice::GetDeviceSettings()
SavedDeviceSettings DeviceSettings::Save()
```

Key methods:
- `GetDigitalOnlyPerforamnceOptions` -- digital-only sample rates (note typo)
- `GetAnalogBasicPerformanceOptions` -- analog+digital basic rates
- `GetAnalogAdvancedPerformanceOptions` -- advanced multi-rate configs
- `GetInterfaceMaximumBitRate` -- max USB bandwidth
- `GetPerformanceInterfaceType` -- performance mode enum
- `ChannelConfigByIndex(U32)` -- per-channel config

### Sample Rate Configuration

IPC: `Saleae::Graph::LogicDevice::SetSampleRate`
Handler: `HandleSetSampleRate`

```
set sample rate: {}, {}   (two values: digital rate, analog rate)
set sample rate: device is not currently acquired
At least one sample rate must be set
digital sample rate is not valid
analog sample rate is not valid
attempted to set invalid sample rate
sample rate not found. request: {}, {}. Available rates: {}
SampleRate should never be 0
```

Sample rate is set BEFORE capture starts, as a separate IPC message.

### Channel Enable Configuration

IPC: `Saleae::Graph::LogicDevice::EnableChannels`
Handler: `HandleEnableChannels`

```
enable channels: device is not currently acquired
```

Channel count constraint in the data format:
```
channel_count == 2 || channel_count == 4
```

### Digital Voltage Threshold

IPC: `Saleae::Graph::LogicDevice::SetDigitalVoltageThreshold`
Handler: `HandleSetDigitalVoltageThreshold`

```
SetDigitalDerivedVoltageThresholds
/digitalThreshold
/digitalThresholdOptions
```

### ADC Full Scale Gain

IPC: `Saleae::Graph::LogicDevice::SetAdcFullScaleGain`
Handler: `HandleSetAdcFullScaleGain`


## Data Format in USB Stream

### ExpandWriteData / ContractReadData

Data packing/unpacking for USB transfer:

```
void LogicDevice::ExpandWriteData(U8 *)
void LogicDevice::ContractReadData(U8 *)
```

These suggest that data sent to the device via EP1 OUT (commands,
config) is "expanded" (padded/formatted) and data received from
EP1 IN (responses) is "contracted" (parsed/compacted).

### OnWrite Callback

```
void LogicDevice::OnWrite(U8 *, U32, const LogicError &)
mOnWriteData
mOnWriteData is required.
```

This callback is invoked when a write to the device completes. The
`mOnWriteData` member holds the callback context.


## Error Handling

### During StartSampling

Three categories of failure:
1. Bank power failure (PLL did not lock)
2. Capture parameter write failure
3. Start command failure

```
StartSampling failed in either bank power, capture parameters, or
  start command. Error: {}
```

### During Capture

Read stream errors:
```
ReadStream Error: {}
USB ReadStream Error: {}
USB Transfer Error, expected transfer size={}, actual transfer size={}
BulkReadThread: Timeout waiting for transfer to complete.
```

Transfer status codes:
```
LIBUSB_SUCCESS / LIBUSB_TRANSFER_COMPLETED
LIBUSB_TRANSFER_CANCELLED
LIBUSB_TRANSFER_ERROR
LIBUSB_TRANSFER_NO_DEVICE
LIBUSB_TRANSFER_OVERFLOW
LIBUSB_TRANSFER_STALL
LIBUSB_TRANSFER_TIMED_OUT
```

Device errors:
```
Device error: {}
Device is not responsive, please reconnect the device. error:
Device unresponsive:
Device unresponsive. Error:
```

On error, the device transitions from DSReady to DSError (terminal).

### FPGA Overflow

```
Digital Fifo overflow
Analog Fifo overflow
Master Fifo overflow
digital fsm error
analog fsm error
```

Overflow is detected per-packet via `cur_overflow != prev_overflow`.


## Device-Specific Behavior

### Logic Pro 8/16 (LogicProfessionalDevice)

- Uses AD9637 (old rev) or ADS528x (new rev) ADCs
- AFE configuration with attenuators, VGA (LMH6518), DAC
- Voltage threshold configurable per channel
- `SetVoltageLevel(U32)` for analog input range
- Public key crypto: `GetPublicKey()`
- EEPROM test: `TestEeprom()`

### Logic 8 (LogicGraduateDevice)

- Uses HMCAD1100 (old rev) or ADS528x (new rev) ADC
- Simpler AFE than Pro devices
- Custom StartSampling implementation

### Logic 4 (LogicStudentDevice)

- Uses HMCAD1100 (old rev) or ADS528x (new rev) ADC
- Custom StartSampling implementation
- Different `ModifyFakeData` signature (extra U32 param)

### Logic 16 (Logic16Device, legacy)

- FX2LP-based (not FX3)
- Separate LED control: `SetLedData(U8 *, U32)`, `SetupLed(bool, bool, double)`
- Legacy `FakeReadThread()` (no params, unlike FX3 devices)
- Custom StartSampling


## Environment Variables

| Variable | Purpose |
|----------|---------|
| `SALEAE_USB_TRANSFER_LIMIT` | Cap USB bulk transfer size |
| `SALEAE_REVISION_OVERRIDE` | Override hardware revision detection |
| `SALEAE_ANALOG_TEST_PATTERN` | Enable ADC test pattern mode |
| `SALEAE_CHUNKED_FPGA_PROGRAMMING` | Use chunked FPGA bitstream upload |


## USB Speed Impact

USB speed detection: `libusb_get_device_speed()`
- "USB Full Speed" -- 12 Mbps (unlikely)
- "USB High Speed" -- 480 Mbps (USB 2.0)
- "USB Super Speed" -- 5 Gbps (USB 3.0)

Impact on capture:
- WriteConfigurationData chunk size: 62 bytes (USB 2.0) vs 509 bytes (USB 3.0)
- Max sample rate (bandwidth limited)
- Max channels at high rates
- Transfer size computation
- Power budget (USB 2.0 = 500mA, USB 3.0 = 900mA)


## Automation API (gRPC) Capture Commands

The gRPC automation API provides a parallel capture path:

```protobuf
StartCaptureRequest -> StartCaptureReply
StopCaptureRequest -> StopCaptureReply
WaitCaptureRequest -> WaitCaptureReply

CaptureConfiguration {
    ManualCaptureMode { trimDataSeconds }
    TimedCaptureMode { durationSeconds, trimDataSeconds }
    DigitalTriggerCaptureMode { triggerChannelIndex, triggerType, ... }
    bufferSizeMegabytes
}

LogicDeviceConfiguration {
    digitalSampleRate
    analogSampleRate
    digitalThresholdVolts
    LogicChannels { digitalChannels[], analogChannels[] }
    GlitchFilterEntry[] { channelIndex, pulseWidthSeconds }
}
```

These gRPC messages ultimately invoke the same internal StartCapture/
StopCapture handlers on the LogicDeviceNode.


## Confidence Assessment

| Finding | Confidence | Sources |
|---------|-----------|---------|
| StartCapture/StopCapture IPC types | **Confirmed** | Mangled symbols + RTTI strings |
| HandleStartCapture registers 3 lambdas ($7,$8,$9) | **Confirmed** | Mangled symbol names with lambda indices |
| SetCaptureParameters writes to FPGA via EP1 OUT | **High** | Function signature + WriteConfigurationData strings |
| WriteConfigurationData 62/509 byte limit | **Confirmed** | Assertion string in binary |
| SendStartCommand triggers data flow | **High** | String sequence + CY Error messages |
| SendStopCommand / COMMAND_STOP_STREAM | **Confirmed** | String literals in binary |
| BulkReadStream with TransferQueue on EP2 IN | **Confirmed** | Thread function + lsusb + string analysis |
| SamplingConfigurationData fields | **Confirmed** | Format string in binary |
| mSamplesPerChunk constraints per FPGA | **Confirmed** | Assertion strings for Altera/Xilinx |
| Trigger buffer constraints | **Confirmed** | Assertion strings in binary |
| Device-specific StartSampling (5 variants) | **Confirmed** | Virtual function signature strings |
| CaptureCompleteData signals end-of-capture | **High** | Type name + pipe handler registration |
| USB 2.0 vs 3.0 affects config chunk size | **Confirmed** | 62/509 string + speed detection code |


## Open Questions

1. **Exact start command format**: What bytes are sent by SendStartCommand?
   Is it a control transfer or a bulk write on EP1 OUT? What is the
   command byte value? Needs Ghidra decompilation of SendStartCommand.

2. **Configuration data blob layout**: What is the binary format of the
   data sent via WriteConfigurationData? The 62/509 byte chunks -- what
   fields do they contain? Sample rate encoding, channel mask, trigger
   config? Needs Ghidra analysis of SetCaptureParameters.

3. **FPGA register map for capture**: Which registers control sample rate
   divider, channel enable, trigger configuration? The log format shows
   16-bit addresses and 32-bit values, but the specific register addresses
   for capture configuration are not yet extracted.

4. **Exact FpgaConstants values**: What are the numeric values of
   MinSamplesPerPacket_Altera, MaxSamplesPerPacket_Xilinx, and
   SamplesPerPacketMultiple_WithStats? These are compile-time constants
   that need extraction from the binary's .rodata or code sections.

5. **Stop command format**: Is COMMAND_STOP_STREAM a single byte, a
   multi-byte command, or a control transfer? What does the FX3 firmware
   do in response?

6. **EP1 OUT command protocol**: Is there a command byte header for
   EP1 OUT messages that distinguishes start/stop/config/bitstream data?
   Or are these differentiated by USB control transfer vs bulk transfer?

7. **DisaggregatorSettingsData format**: How does this struct describe
   the channel layout in the interleaved USB stream?


## New Leads (backlog candidates)

- `ReflectedChannelMap` in DisaggregatorNode context suggests channels
  can be reordered/reflected in the disaggregation mapping.

- `LogicDevice::ExpandWriteData` and `ContractReadData` suggest a
  specific wire format for EP1 OUT/IN command data that differs from
  the raw payload.

- The `$_10` lambda in `LogicDeviceNode::OnDataBuffer` is separate from
  the `$_7` capture data callback, suggesting OnDataBuffer has additional
  processing (possibly for config readback or start packet handling).

- `CommonDeviceInit` is called during initialization but the same ADC/PLL
  setup may be partially repeated during capture start (SetBankPower
  re-locks the PLL).

- The `SALEAE_REVISION_OVERRIDE` env var suggests hardware revision
  detection can be overridden, which means the capture configuration
  path depends on detected revision.
