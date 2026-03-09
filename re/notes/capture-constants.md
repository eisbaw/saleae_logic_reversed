# Capture Constants Extraction Notes

Working notes from TASK-0023 analysis of libgraph_server_shared.so v2.4.40.


## Method

Constants were extracted by:
1. Finding assertion strings in .rodata containing constant names
2. Locating code references (via RIP-relative LEA scan) to those strings
3. Disassembling the surrounding functions to find CMP instructions with
   immediate values that correspond to the assertion conditions
4. Cross-referencing with JS bundle code in extracted app.asar


## Key Function: ValidateConfig in mso_fpga.cpp

Address: 0x02d64640 (function prologue)
Source file: `/workspace/monorepo/graph-io/devices/mso_device/src/mso_device/component/mso_fpga.cpp`

This function validates capture configuration before programming the FPGA.
It contains assertions for all FpgaConstants and MsoConstants limits.

The function has two code paths selected by device/FPGA type:
- Path 1 (device type >= 2): Xilinx FPGA limits
- Path 2 (device type <= 1): Altera FPGA limits

Selection at 0x2d64750: `test $0xfe, %al; je` (al from device type function)


### Comparison instruction mapping

| Address | Instruction | Assertion | Extracted Value |
|---------|-------------|-----------|-----------------|
| 0x2d64718 | `cmp $0x7, %rbx; jle` | MinTriggerBufferSamples | 8 |
| 0x2d64754 | `cmp $0x3fffc001, %rbx; jge` | MaxTriggerBufferSamples_Xilinx | 0x3FFFC000 |
| 0x2d6477e | `cmp $0xff, %ecx; jbe` | MinSamplesPerPacket_Xilinx | 256 |
| 0x2d6478a | `cmp $0x1e01, %ecx; jb (pass)` | MaxSamplesPerPacket_Xilinx | 7680 |
| 0x2d647cd | `cmp $0x2fffc001, %rbx; jge` | MaxTriggerBufferSamples_Altera | 0x2FFFC000 |
| 0x2d647f7 | `cmp $0x3f, %ecx; jbe` | MinSamplesPerPacket_Altera | 64 |
| 0x2d64800 | `cmp $0x1781, %ecx; jae` | MaxSamplesPerPacket_Altera | 6016 |
| 0x2d64813 | `test %cl, %cl; jne` | SamplesPerPacketMultiple_WithStats | 256 |

The SamplesPerPacketMultiple_WithStats = 256 conclusion: `test %cl, %cl`
checks `(mSamplesPerChunk & 0xFF) == 0`, which is equivalent to `val % 256 == 0`.
This modulo check is conditional on the "withStats" flag (byte at config+0x00).


### MaxScopeChannels

Address: 0x2d58f49 in oscilloscope_simulator.cpp
Source: `/workspace/monorepo/graph-io/devices/mso_device/src/mso_device/oscilloscope_simulator.cpp`

```
cmp $0x4, %ebp
jge -> assertion "channel_number < MsoConstants::MaxScopeChannels"
```

MaxScopeChannels = 4 (channels 0-3)


### TotalAfeConfigurations

Address: 0x2d5b614 in mso_device_interface.cpp / oscilloscope_data.cpp
Source: `/workspace/monorepo/graph-io/devices/mso_device/src/mso_device/oscilloscope_data.cpp`

```
cmp $0x43, %esi
jae -> assertion "number >= 0 && number <= MsoConstants::TotalAfeConfigurations"
```

TotalAfeConfigurations = 66 (0x42). Values 0 through 66 are valid AFE config numbers.
Value 66 (0x42) has special handling (separate code path via `je`).


### Glitch Filter

Address: 0x2d664ee in mso_fpga.cpp (InternalSetDigitalConfiguration)
Source: `void Saleae::MsoFpga::InternalSetDigitalConfiguration(uint16_t, ...)`

Two assertions at lines 0x32b (811) and 0x32c (812):
1. "config.mPulseWidth >= MsoConstants::MinGlitchFilterLength && config.mPulseWidth <= MsoConstants::MaxGlitchFilterLength"
2. "config.mPulseWidth == 5 || ( ( config.mPulseWidth % 10 ) == 0 )"

The MinGlitchFilterLength and MaxGlitchFilterLength are runtime values from
MsoDeviceConstants (sent as `/digitalLimit_MinGlitchFilterLength` and
`/digitalLimit_MaxGlitchFilterLength` JSON keys). But the pulse width must
additionally be either 5 or a multiple of 10.


## MsoDeviceConstants Struct Layout

Serialization function at 0x02d77b00-0x02d77cc2. This is a deserialization
function (reads JSON keys into struct fields).

| Offset | JSON Key | Type |
|--------|----------|------|
| 0x00 | /maxAdcCode | int16 |
| 0x02 | /ddrMemoryCapacityBytes | ? |
| 0x08 | /analogChannelCount | int32 |
| 0x10 | /smartCablePortCount | ? |
| 0x18 | /clockRateHz | uint64 |
| 0x20 | /analogSampleRate | uint32 |
| 0x28 | /scopeDigitalSampleRate | ? |
| 0x30 | /minAnalogSamplesPerPacket | ? |
| 0x38 | /maxAnalogSamplesPerPacket | ? |
| 0x40 | /minDacVoltage | double? |
| 0x48 | /maxDacVoltage | double? |
| 0x50 | /dacBitDepth | ? |
| 0x58 | /ddrMemoryCapacityBytes (dup?) | ? |
| 0x60 | /fpgaImageType | ? |
| 0x68 | /analogLimit_MaxDownsampleRatio | ? |
| 0x70 | /analogLimit_MinTotalTriggerBufferLength | int64 |
| 0x78 | /analogLimit_MaxTotalTriggerBufferLength | int64 |
| 0x80 | /analogLimit_MinTriggerBufferIndex | int64 |
| 0x88 | /analogLimit_MaxTriggerBufferIndex | int64 |
| 0x90 | /analogLimit_MaxHoldoffSamples | uint64 |
| 0x98 | /analogLimit_MaxTimeoutSamples | uint64 |
| 0xA0 | /digitalLimit_MinGlitchFilterLength | uint32 |
| 0xA4 | /digitalLimit_MaxGlitchFilterLength | uint32 |

Note: the analogLimit values are RUNTIME (per-device) values, not
compile-time constants. They are sent from the native code to the
Electron frontend as part of the device info JSON payload.


## JS-Side Configuration (from extracted app.asar)

### Downsample Ratio Rules

From `roundUpToNearestSampleRate` in bundle.js:

```javascript
minDownsample = { Normal: 1, Average: 2, PeakDetect: 4 }

// Quantization:
if ([3, 5, 7].includes(ratio)) ratio += 1;
if (ratio >= 8) ratio = 8 * Math.ceil(ratio / 8);
// Max: 8 * 162500 = 1,300,000
```

### Analog Downsampled Rates

Fixed list of available downsampled rates:
```javascript
[50, 250, 1250, 6250, 15625, 31250] // Hz, sorted descending in code
```

Devices supporting downsampling: Logic4, Logic8, LogicPro8, LogicPro16.

### SampleRateOption Schema

Each sample rate option has:
- `digital`: number (Hz) -- digital sample rate
- `analog`: number (Hz) -- analog sample rate

Returned by `DeviceSettings::GetDigitalOnlyPerformanceOptions`,
`GetAnalogBasicPerformanceOptions`, `GetAnalogAdvancedPerformanceOptions`.

### Trigger Buffer Computation

From `adjustSampleBufferIndices` in bundle.js:
- Pre-trigger index clamped to [MinTriggerBufferIndex, MaxTriggerBufferIndex - MaxTotalTriggerBufferLength]
- Holdoff limit: MaxHoldoffSamples / downsampleRatio, aligned to 8
- Post-trigger index clamped to [pre + MinTotalTriggerBufferLength, min(MaxTotalTriggerBufferLength + pre, MaxTriggerBufferIndex)]
- Both indices aligned to multiples of 8

From `computeSamplingConfig`:
- Default samples per waveform: min(64, (MaxTotalTriggerBufferLength - 14) / widthPx)

From `computeTriggerSampleSettings`:
- Timeout values clamped to MaxTimeoutSamples (aligned to 8)
- Holdoff values clamped to MaxHoldoffSamples (aligned to 8)
- Default trigger timeout view ratio = 4x view width
- Default min trigger timeout = 1 second
- Default auto trigger timeout view ratio = 2x view width
