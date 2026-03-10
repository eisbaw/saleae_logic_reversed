# Software Trigger Engine

RE notes from TASK-0067 analysis of libgraph_server_shared.so v2.4.40.

All Saleae Logic devices are streaming samplers with no on-device trigger
hardware (except MSO, which has partial FPGA-based analog trigger support).
All trigger detection runs host-side in the graph server.

## Trigger Architecture

Three trigger node classes exist in the graph processing pipeline:

| Class | Source File | Purpose |
|-------|-----------|---------|
| `Saleae::Graph::DigitalTriggerNode` | `digital_trigger_node.cpp` | Edge/pulse detection on digital channels |
| `Saleae::Graph::AnalogTriggerNode` | (oscilloscope_instrument.cpp) | Threshold/hysteresis on analog ADC samples |
| `Saleae::Graph::AnalyzerTriggerNode` | (unknown .cpp) | Trigger on protocol decoder results |

Each node is a graph kernel registered via `KernelFactory::AddKernel<T>()`.
Nodes receive data through pipe handlers and emit trigger results.


## Trigger Type Enum

Serialized as strings in the settings data.

| Value | String | Description |
|-------|--------|-------------|
| 0 | `EdgeTrigger` | Triggers on digital edge transitions |
| 1 | `PulseTrigger` | Triggers on pulse width (duration) |

The dispatch in `CheckTrigger` (FUN_01be1720, Ghidra 0x01be1720):
- Reads trigger type from `*(int*)(settings + 0x58)`
- Type 0 -> calls EdgeTrigger_Detect (FUN_01be1960)
- Type 1 -> calls PulseTrigger_Detect (FUN_01be23c0)
- Any other value -> abort (`FUN_01be5210`, likely `__builtin_unreachable`)


## DigitalTriggerNode

### Pipe Handlers

Registered in the constructor (FUN_01be0530, Ghidra 0x01be0530):

| Handler | Port | Function |
|---------|------|----------|
| HandleDigitalData | "in" | FUN_01be0ef0 - processes incoming digital sample data |
| HandleSamplingConfiguration | "in" | FUN_01be0dd0 - receives sample rate, resets trigger state |
| HandleDigitalTriggerSettingsData | "settings" | FUN_01be0d60 - receives trigger config from UI |

Output port: "out" (DigitalTriggerResultData)

### HandleDigitalData Flow (FUN_01be0ef0)

1. Assert port_name == "in"
2. Look up channel state via `GetChannelState` (FUN_01be50b0)
3. Update accumulated sample count: `state.total_samples += DigitalData.length`
4. If trigger settings exist AND trigger is enabled AND not yet triggered:
   - Store shared_ptr to DigitalData in the channel state
   - Call `CheckTrigger` (FUN_01be1720)
5. Call `EmitOutput` (FUN_01be10f0) to propagate any trigger results

### HandleSamplingConfiguration Flow (FUN_01be0dd0)

1. Assert port_name == "in"
2. Copy SamplingConfigurationData fields into channel state:
   - Offsets 0x18-0x28 of SamplingConfigurationData -> state offsets 0x50-0x60
3. Call `ConfigReset` (FUN_01be12f0) to reinitialize trigger detection
4. Reset sample counter to 0

### CheckTrigger (FUN_01be1720)

1. Call `GetTriggerSampleRange` (FUN_01be1890) to compute the range of samples
   to scan across all channels being monitored
2. Update `state.max_scanned_sample` (offset 0x168)
3. If there are unscanned samples remaining:
   - Read trigger type from settings (offset 0x58)
   - Dispatch to EdgeTrigger_Detect or PulseTrigger_Detect
4. For each trigger found, iterate through the results tree and update
   the trigger output data

### ConfigReset (FUN_01be12f0)

Reinitializes trigger detection state when settings or sampling config change.

1. Reads settings pointer (offset 0x120)
2. Based on trigger type:
   - EdgeTrigger (type 0): Sets trigger pattern pointer to `settings + 0x30`,
     copies edge channel from `settings + 0x28`
   - PulseTrigger (type 1): Sets trigger pattern pointer to `settings + 0x40`,
     copies pulse channel from `settings + 0x28`, then computes pulse width
     min/max from the pulse duration range and sample rate:
     - `min_samples = floor(duration_min * sample_rate)`
     - `max_samples = ceil(duration_max * sample_rate)`
3. Builds the list of channels to monitor from the trigger pattern


## DigitalTriggerSettingsData

Serialization keys found in the binary (used for IPC between Electron and
the graph server):

| Field | JSON Key | Type | Struct Offset |
|-------|----------|------|---------------|
| enabled | `/triggerEnabled` | bool | +0x20 |
| type | `/triggerType` | string ("EdgeTrigger"/"PulseTrigger") | +0x58 (as int: 0/1) |
| edge channel | `/edgeChannel` | int | +0x28 |
| edge type | `/edgeType` | string | +0x28+4 (edge_direction: 0=rising, 1=falling) |
| pulse channel | `/pulseChannel` | int | (same slot as edge channel) |
| pulse type | `/pulseType` | string | |
| duration range | `/durationRange` | object | (min/max seconds) |
| edge pattern | | vector<int> | +0x30 (pointer to pattern array) |
| pulse pattern | | vector<int> | +0x40 (pointer to pattern array) |

### Edge Direction Encoding

In the edge detection function (FUN_01be1960):
- `*(uint*)(param_2 + 4)` holds the edge direction
- Value 0: Rising edge -- matches when digital value transitions to 1
- Value 1: Falling edge -- matches when digital value transitions to 0


## EdgeTrigger Detection Algorithm (FUN_01be1960)

The edge trigger scans digital sample data for transitions matching
the configured edge direction (rising or falling) on specified channels.

1. Look up the current state for each monitored channel in a BST (keyed by
   channel index)
2. For each channel, read the current digital value using `DigitalData` iterators
3. Build a vector of `(channel_state, edge_direction_is_falling)` pairs
4. Create a DigitalData iterator spanning the configured sample range
5. Advance through the digital data, checking each transition:
   - If edge_direction == 0 (rising): match when `GetValue() == 1`
   - If edge_direction == 1 (falling): match when `GetValue() == 0`
6. When a matching transition is found and passes the channel filter
   (`FUN_01be3a10` checks all channels in the pattern):
   - Allocate a trigger result object (0x48 bytes)
   - Record the trigger timestamp relative to the current state
   - Emit via `SendData("out", DigitalTriggerResultData)`
   - Set `triggered = true` (offset 0x180 -> 1)

Key observations:
- Multi-channel pattern matching: all channels in the pattern must match
  before the trigger fires
- The trigger is one-shot: once `triggered` flag is set, no further
  detection occurs until reset
- Trigger position is tracked as a sample offset from the start of the
  capture, not an absolute timestamp


## PulseTrigger Detection Algorithm (FUN_01be23c0)

The pulse trigger detects pulses with duration within a configured range.
This is a more complex state machine (4800 bytes of code vs 2176 for edge).

1. Similar channel lookup and iterator setup as EdgeTrigger
2. Maintains additional state for pulse tracking:
   - Pulse start sample position
   - Pulse polarity (positive/negative)
   - Min/max duration thresholds (computed in ConfigReset from duration range
     and sample rate)
3. Scans for transitions (edges), measuring the duration between:
   - Edge start -> opposite edge = pulse width
4. If pulse width falls within [min_samples, max_samples]:
   - Trigger fires at the end of the pulse
5. Supports positive and negative pulse detection via the pulse type setting


## AnalogTriggerNode (MSO Devices)

For MSO (oscilloscope) devices, analog triggering has FPGA hardware support.
For standard Logic devices, this node handles software-based analog triggering
on ADC samples.

### AnalogTriggerConfiguration Struct

Reconstructed from `SetAnalogTriggerConfiguration` (FUN_02e654a0,
Ghidra 0x02e654a0) assertions:

| Offset | Field | Type | Validation |
|--------|-------|------|-----------|
| +0x00 | mChannelSource | uint8 | < mDeviceConstants.mAnalogChannelCount |
| +0x02 | mTriggerThreshold | int16 | (ADC code) |
| +0x04 | mTriggerHysteresisFalling | int16 | >= mTriggerThreshold |
| +0x06 | mTriggerHysteresisRising | int16 | <= mTriggerThreshold |
| +0x08 | mTriggerMode | uint8 (2 bits) | Sets field at bits [2:3] in reg 0x70 |
| +0x09 | mTriggerType | uint8 (4 bits) | Sets field at bits [4:7] in reg 0x70 |
| +0x0a | mGlitchFilterEnabled | uint8 (2 bits) | Sets field at bits [0:1] in reg 0x70 |
| +0x10 | mInitialHoldoffSamples | uint64 | <= MaxInitialHoldoffSamples, aligned to 8 |
| +0x18 | mTriggerNormal_HoldoffSamples | uint64 | <= MaxHoldoffSamples, aligned to 8 |
| +0x20 | mTriggerNormal_TimeoutSamples | uint64 | <= MaxTimeoutSamples, aligned to 8 |
| +0x28 | mTriggerAuto_HoldoffSamples | uint64 | <= MaxHoldoffSamples, aligned to 8 |
| +0x30 | mTriggerAuto_TimeoutSamples | uint64 | <= MaxTimeoutSamples, aligned to 8 |

### Hysteresis

The trigger threshold is a signed ADC code value. Two hysteresis bands define
the arming/disarming levels:

```
                  HysteresisRising (<= threshold)
                        |
   ------threshold------+------
                        |
                  HysteresisFalling (>= threshold)
```

- For rising edge trigger: signal must first drop below HysteresisRising to
  arm, then cross above threshold to fire
- For falling edge trigger: signal must first rise above HysteresisFalling to
  arm, then cross below threshold to fire

### Serialization Fields

| Field | JSON Key | Type |
|-------|----------|------|
| trigger voltage | `/triggerVoltage` | double |
| trigger hysteresis | `/triggerHysteresis` | double |
| trigger out of range | `/triggerOutOfRange` | bool |
| channel source | `/channelSource` | int |
| trigger type | `/triggerType` | string |
| trigger enabled | `/triggerEnabled` | bool |
| trigger relative | `/triggerRelative` | bool |


## AnalogChannelConfiguration Struct

From `SetAnalogChannelConfiguration` (FUN_02e64640, Ghidra 0x02e64640):

| Offset | Field | Type | Validation |
|--------|-------|------|-----------|
| +0x00 | mWithStats | bool | Determines alignment constraint |
| +0x04 | mSampleRate | uint32 | Written to FPGA reg 0x54 |
| +0x08 | mDownsampleMode | uint8 (2 bits) | Written to reg 0x40 bits [5:6] |
| +0x10 | mPreTriggerLength | int64 | Negated before FPGA write; bounds vary by FPGA |
| +0x18 | mPostTriggerLength | int64 | Bounds: <= MaxTriggerBufferIndex |
| +0x20 | mSamplesPerChunk | uint32 | Bounds vary by FPGA type |


## Pre/Post Trigger Buffer Management

### Buffer Constraints

| Constant | Value | Scope |
|----------|-------|-------|
| MinTriggerBufferSamples | 8 | All devices |
| MaxTriggerBufferIndex | 0xBFFF (49,151) | Post-trigger max |
| MaxTriggerBufferSamples_Xilinx | 0x3FFFC000 (1,073,725,440) | Xilinx total |
| MaxTriggerBufferSamples_Altera | 0x2FFFC000 (805,289,920) | Altera total |
| MinTriggerBufferIndex_Xilinx | -0x3FFFC000 | Pre-trigger min (negated) |
| MinTriggerBufferIndex_Altera | -0x2FFFC000 | Pre-trigger min (negated) |
| SamplesPerPacketMultiple_WithStats | 256 | Alignment when stats enabled |

### Validation Rules (from assertions)

1. `pre_trigger + post_trigger >= 8` (MinTriggerBufferSamples)
2. `-pre_trigger < post_trigger` (pre is stored negative)
3. `post_trigger <= 0xBFFF` (MaxTriggerBufferIndex)
4. FPGA-specific total buffer limit
5. `(pre + post) % SamplesPerPacketMultiple_WithStats == 0` when stats enabled
6. Samples per chunk must be within FPGA-specific min/max packet size

### FPGA Register Writes for Buffer Config

| Register | Content | Notes |
|----------|---------|-------|
| 0x58 | `-mPreTriggerLength` (uint32) | Pre-trigger count, negated |
| 0x5c | `mPostTriggerLength` (uint32 low) | Post-trigger count |
| 0x64 | Combined hi bits | `[15:0] = pre_hi, [31:16] = post_hi` |
| 0x68 | `mSamplesPerChunk` | Chunk size for DMA |
| 0x54 | `mSampleRate` | Rate divider |
| 0x60 | `round(clock_freq / mSampleRate)` | Clock divider |


## Holdoff and Timeout

All holdoff/timeout values are in samples, must be multiples of 8. Converted
to FPGA ticks by dividing by 8 (right shift 3).

### Holdoff Types

| Type | Purpose | Max Constant |
|------|---------|-------------|
| InitialHoldoff | Ignore triggers after start of capture | MaxInitialHoldoffSamples |
| Normal Holdoff | Re-arm delay after trigger in normal mode | MaxHoldoffSamples |
| Normal Timeout | Auto-trigger timeout in normal mode | MaxTimeoutSamples |
| Auto Holdoff | Re-arm delay after trigger in auto mode | MaxHoldoffSamples |
| Auto Timeout | Auto-trigger timeout in auto mode | MaxTimeoutSamples |

### FPGA Registers for Holdoff/Timeout

| Register | Content |
|----------|---------|
| 0x80 | Initial holdoff ticks low 32 bits |
| 0x90 | Initial holdoff ticks high 13 bits |
| 0x74 | Auto holdoff ticks |
| 0x78 | Auto timeout ticks |
| 0x84 | Normal holdoff ticks |
| 0x88 | Normal timeout ticks |
| 0x8c | Combined hi bits: `[11:0] = initial_holdoff_hi, [27:16] = normal_timeout_hi` |


## Software Trigger Injection

The `SetSoftwareTrigger` function (FUN_02e66d60, Ghidra 0x02e66d60) provides
a mechanism to force a trigger at a specific timestamp from the host:

```
Source: mso_fpga.cpp, function SetSoftwareTrigger

1. Log: "Setting software trigger at timestamp {}"
2. Write FPGA reg 0x98 = timestamp_low32
3. Write FPGA reg 0x9c = timestamp_high28 (28 bits, bits [27:0])
```

The software trigger is received and processed by `HandleWaveformData`
(FUN_02e44b80) in the oscilloscope instrument:

1. Checks if a software trigger was pending (`param_1 + 0x3290`)
2. Validates the trigger timestamp matches the expected value
3. Handles empty triggers (triggered at boundary with no data)
4. Tracks per-channel completion (4 channels, `param_1 + 0x3288..0x328b`)
5. Logs: "Software waveform {} complete" when all channels received

### Software Trigger Path Strings

| String | Context |
|--------|---------|
| "Setting software trigger at timestamp {}" | FPGA register write |
| "Received a software trigger, but there was no pending request" | Unexpected trigger |
| "Received a software trigger that does not match the pending request" | Timestamp mismatch |
| "Received an empty software trigger for {}" | Empty trigger data |
| "Received software triggered waveform for channel that has already completed" | Duplicate |
| "Software waveform {} complete" | All channels done |


## AnalyzerTriggerNode

Triggers on decoded protocol analyzer results. Registered handlers:

| Handler | Request Type |
|---------|-------------|
| HandleWorkRequest | `Saleae::Graph::WorkRequest` |
| HandleNewAnalyzerRunStarted | `Saleae::Graph::NewAnalyzerRunStarted` |
| HandleAnalyzerTriggerSettingsData | (pipe handler) |

This node waits for a specific protocol decode result to appear, then triggers.
The detailed matching logic is in the `SetApi` lambda (which hooks into the
analyzer pipeline). Out of scope for this analysis as protocol analyzers are
excluded from the RE effort.


## Glitch Filter (Digital Channels, MSO Only)

Part of `InternalSetDigitalConfiguration` (FUN_02e663f0, Ghidra 0x02e663f0).

The glitch filter rejects pulses shorter than a configured width. Written
to a per-channel FPGA register specified by the `param_2` register address.

### Constraints

- MinGlitchFilterLength: 5 (runtime, from `/digitalLimit_MinGlitchFilterLength`)
- MaxGlitchFilterLength: ~250,001 (runtime, from `/digitalLimit_MaxGlitchFilterLength`)
- Valid values: exactly 5, or any multiple of 10
- Encoded in FPGA register as: `pulse_width | ((pulse_width & 1) << 20)`

### Register Encoding

The pulse width value is written directly to the FPGA register. The low bit
of the pulse width is additionally OR'd into bit 20, which likely signals to
the FPGA that the special width-5 mode is active (since 5 is not a multiple
of 10, bit 0 is set only for width=5).


## Logic Pro vs MSO Differences

| Feature | Logic Pro 8/16 | MSO |
|---------|---------------|-----|
| Digital trigger | Software only (DigitalTriggerNode) | Software (DigitalTriggerNode) |
| Analog trigger | Software only (AnalogTriggerNode) | FPGA hardware + software |
| Software trigger injection | Not applicable | Via FPGA regs 0x98/0x9c |
| Glitch filter | Not available | FPGA-based, per-channel |
| Trigger buffer validation | Generic limits | FPGA-type-specific (Altera/Xilinx) |
| Holdoff/timeout registers | N/A (all software) | FPGA registers 0x74-0x90 |

For Logic Pro devices, all trigger detection happens in the DigitalTriggerNode
and AnalogTriggerNode graph kernels, purely in software. The FPGA has no
trigger awareness -- it simply streams all samples to the host.

For MSO devices, the FPGA provides hardware-assisted analog triggering with
configurable threshold, hysteresis, holdoff/timeout, and glitch filtering.
The software trigger injection path (`SetSoftwareTrigger`) allows the host
to force-trigger the FPGA-based trigger at a specific timestamp.


## Trigger Result Data

### DigitalTriggerResultData

Emitted by DigitalTriggerNode on the "out" port. Contains:
- Trigger timestamp (sample index relative to capture start)
- Pre/post trigger sample range
- Boolean: has_trigger / has_next_trigger / has_previous_trigger

### DigitalTriggerState2

Serialized state with fields:
- `/currentTriggerType` - the active trigger type
- `/currentTriggerTime` - timestamp of current trigger
- `/currentTriggerWaveformId` - waveform identifier
- `/triggerCount` - number of triggers detected so far
- `/preTriggerIndex` - pre-trigger buffer position
- `/postTriggerIndex` - post-trigger buffer position
- `/autoTriggerHoldoffSamples` - current auto holdoff
- `/autoTriggerTimeoutSamples` - current auto timeout
- `/realTriggerHoldoffSamples` - actual holdoff applied
- `/realTriggerTimeoutSamples` - actual timeout applied

### AnalogTriggerNodeState2

Similar to DigitalTriggerState2 but for analog triggers.

### AnalogTriggerResultData / AnalyzerTriggerResultData

Result data types for analog and analyzer triggers respectively.


## Key Addresses (ELF VAs)

| Address | Ghidra VA | Function |
|---------|-----------|----------|
| 0x01be0530 | 0x01be0530 | DigitalTriggerNode constructor |
| 0x01be0d60 | 0x01be0d60 | HandleDigitalTriggerSettingsData |
| 0x01be0dd0 | 0x01be0dd0 | HandleSamplingConfiguration |
| 0x01be0ef0 | 0x01be0ef0 | HandleDigitalData |
| 0x01be10f0 | 0x01be10f0 | EmitOutput |
| 0x01be12f0 | 0x01be12f0 | ConfigReset |
| 0x01be1720 | 0x01be1720 | CheckTrigger (dispatcher) |
| 0x01be1890 | 0x01be1890 | GetTriggerSampleRange |
| 0x01be1960 | 0x01be1960 | EdgeTrigger_Detect |
| 0x01be23c0 | 0x01be23c0 | PulseTrigger_Detect |
| 0x02d64640 | 0x02e64640 | SetAnalogChannelConfiguration |
| 0x02d654a0 | 0x02e654a0 | SetAnalogTriggerConfiguration |
| 0x02d663f0 | 0x02e663f0 | InternalSetDigitalConfiguration (glitch filter) |
| 0x02d66d60 | 0x02e66d60 | SetSoftwareTrigger |
| 0x02d44b80 | 0x02e44b80 | HandleWaveformData (software trigger receiver) |
| 0x02d47b00 | 0x02e47b00 | PreprocessConfiguration (analog trigger channel) |
| 0x01cc0bc0 | 0x01cc0bc0 | ConvertToAnalogState |

Note: ELF VA = file offset + 0x1000 for .text section.
Ghidra VA = ELF VA + 0x100000.
