# Digital Capture Data Format and Streaming Protocol

Analysis date: 2026-03-09
Binary: libgraph_server_shared.so (48MB, stripped, ELF 64-bit x86-64)
Source: Saleae Logic 2 v2.4.40


## Summary

The digital capture data pipeline follows this path:

```
FPGA -> FX3 -> EP2 IN bulk transfer -> TransferQueue
  -> OnDataBuffer -> DisaggregatorNode -> DigitalCreatorNode
  -> DigitalStore (RLE-encoded) -> DigitalTriggerNode / DigitalRenderer
```

Key findings:
1. Digital data is **interleaved** in the USB bulk transfer stream -- all
   enabled channels (digital and analog) share the same EP2 IN endpoint.
2. The `DisaggregatorNode` separates the interleaved stream into per-channel
   data.
3. Digital data is stored using **Run-Length Encoding (RLE)** via
   `Saleae::Digital::Detail::AppendRle`.
4. Packet sizes are constrained by FPGA-specific constants
   (`FpgaConstants::Min/MaxSamplesPerPacket_Altera/Xilinx`).
5. Each packet includes stats metadata
   (`SamplesPerPacketMultiple_WithStats`).
6. USB transfer size is computed dynamically by
   `LogicDevice::ComputeTransferSize`.
7. The data format includes start packets, overflow detection, and
   configuration IDs for stream synchronization.


## Data Flow Architecture

### USB to Graph Pipeline

```
    USB Hardware
        |
        | EP2 IN (0x82), bMaxBurst=15
        | Async bulk transfers via TransferQueue
        v
    LinuxUsbDevice::ReadStream_QueueBuffersThread
        |
        | on_libusb_transfer_complete callback
        v
    LogicDeviceNode::OnDataBuffer(DeviceId, Buffer)
        |
        | Emits VectorData<uint8_t> (raw bytes)
        | Also emits SamplingConfigurationData
        | Also emits DisaggregatorSettingsData
        v
    DisaggregatorNode
        |
        | Splits interleaved data into per-channel streams
        | Emits OwnedSpan<uint8_t> per channel
        | Emits DigitalData per digital channel
        | Emits VectorData<int16_t> per analog channel
        | Also forwards CaptureCompleteData
        v
    +---+---+---+
    |   |   |   |
    v   v   v   v
  DigitalCreatorNode  AnalogCreatorNode  DigitalTriggerNode
    |                   |                  |
    v                   v                  v
  DigitalStore        AnalogStore      Trigger detection
    |                   |
    v                   v
  DigitalRenderer     AnalogRenderer
```

### Key Data Types in Pipeline

| Type | Template Param | Purpose |
|------|---------------|---------|
| `VectorData<uint8_t>` | `h` (unsigned char) | Raw USB bytes from device |
| `VectorData<int16_t>` | `s` (short) | Analog ADC samples (12-bit in 16-bit container) |
| `OwnedSpan<uint8_t>` | - | Disaggregated per-channel data |
| `DigitalData` | - | Processed digital channel data |
| `SamplingConfigurationData` | - | Sample rate, channel config |
| `DisaggregatorSettingsData` | - | Channel layout for disaggregation |
| `CaptureCompleteData` | - | End-of-capture signal |


## Capture Start Sequence

From string analysis of `HandleStartCapture` and `SetCaptureParameters`:

```
1. HandleStartCapture (LogicDeviceNode)
   - Guard: "Cannot start capture while device is not in Ready state"
   - Registers OnDataBuffer callback (lambda $_7)
   - Registers error callback (lambda $_8)
   - Returns StartCaptureResponse

2. SetCaptureParameters (Logic2FpgaDevice)
   - Writes capture config to FPGA registers
   - Sets sample rate, channel enables, trigger config
   - "Capture parameter failed to download to device"
   - "setting capture settings"
   - "reading back capture settings"

3. SetBankPower
   - Enables IO bank power for enabled channels
   - "bank enabled, PLL is locked."
   - "bank enabled, PLL IS NOT LOCKED"

4. ComputeTransferSize (LogicDevice)
   - Computes optimal USB bulk transfer buffer size
   - "ideal transfer size {}"
   - "Transfer size is: {}"
   - ", capping transfer size."
   - SALEAE_USB_TRANSFER_LIMIT env var for override
   - virtual function -- each device type can override

5. StartSampling (LogicDevice/LogicGraduateDevice/LogicProfessionalDevice/
                   LogicStudentDevice)
   - Device-specific implementations
   - "StartSampling failed in either bank power, capture parameters,
      or start command. Error: {}"
   - "failed to write capture parameters or send the FX3 start command.
      Note that the read thread has not started."

6. BulkReadStream starts
   - "Starting bulk read thread, transfer_size={}, timeout={}"
   - ReadStream_QueueBuffersThread begins queuing async transfers
```


## Sampling Configuration

The `SamplingConfigurationData` type carries:

```
Setting sampling config, downsample={}, pre={}, post={}, stats={}, chunk_size={}
```

Fields:
- `downsample` -- downsampling ratio
- `pre` -- pre-trigger buffer length
- `post` -- post-trigger buffer length
- `stats` -- whether stats metadata is included in packets
- `chunk_size` -- samples per chunk/packet (`mSamplesPerChunk`)

### Constraints (from assertion strings)

```
config.mSamplesPerChunk >= FpgaConstants::MinSamplesPerPacket_Altera
config.mSamplesPerChunk >= FpgaConstants::MinSamplesPerPacket_Xilinx
config.mSamplesPerChunk <= FpgaConstants::MaxSamplesPerPacket_Altera
config.mSamplesPerChunk <= FpgaConstants::MaxSamplesPerPacket_Xilinx
config.mSamplesPerChunk % FpgaConstants::SamplesPerPacketMultiple_WithStats == 0
```

The `mSamplesPerChunk` must be:
1. Within FPGA-specific min/max bounds (different for Altera vs Xilinx)
2. A multiple of `SamplesPerPacketMultiple_WithStats`

This means each chunk/packet contains a fixed number of samples, aligned
to include per-packet stats metadata. The WithStats suffix indicates that
some bytes at the end of each packet contain metadata (overflow counters,
timestamps, etc.) rather than sample data.

### Trigger Buffer Constraints

```
( config.mPreTriggerLength + config.mPostTriggerLength )
    % FpgaConstants::SamplesPerPacketMultiple_WithStats == 0
-config.mPreTriggerLength >= MsoConstants::MinTriggerBufferIndex_Altera
-config.mPreTriggerLength >= MsoConstants::MinTriggerBufferIndex_Xilinx
sample_length <= MsoConstants::MaxTriggerBufferSamples_Altera
sample_length <= MsoConstants::MaxTriggerBufferSamples_Xilinx
sample_length >= MsoConstants::MinTriggerBufferSamples
config.mPostTriggerLength <= MsoConstants::MaxTriggerBufferIndex
```


## USB Bulk Transfer Format

### Transfer Size Computation

`LogicDevice::ComputeTransferSize(U64)` is a virtual method -- each device
type computes its own optimal transfer size based on:
- USB speed (USB 2.0 vs 3.0)
- Sample rate
- Number of enabled channels
- USB bandwidth constraints

The `SALEAE_USB_TRANSFER_LIMIT` environment variable can cap the transfer
size. Log messages:
```
ideal transfer size {}
Transfer size is: {}
, capping transfer size.
SALEAE_USB_TRANSFER_LIMIT used to limit transfer size to {}
```

### Packet Structure

Each bulk transfer contains one or more "chunks" of sample data. Each
chunk contains exactly `mSamplesPerChunk` samples plus optional stats
metadata.

#### Start Packet

Before sample data begins flowing, a start packet is sent:
```
Got start packet: ch={} timestamp={}, time={}, config_id={}
Expected start packet
Received digital data without ever receiving a start packet
```

Fields:
- `ch` -- channel number
- `timestamp` -- device timestamp
- `time` -- host time
- `config_id` -- configuration ID for stream synchronization

#### Data Packets

Each data packet/chunk contains:
```
id={}, start={}, end={}, sample_count={}, config_id={}, ts={},
  software={}, cur_overflow={}, prev_overflow={}, samples={}
```

Fields:
- `id` -- packet ID
- `start` -- start sample index
- `end` -- end sample index
- `sample_count` -- number of samples in this packet
- `config_id` -- matches the start packet's config_id
- `ts` -- timestamp
- `software` -- software-generated flag?
- `cur_overflow` -- current FIFO overflow count
- `prev_overflow` -- previous overflow count
- `samples` -- the actual sample data

The `cur_overflow` and `prev_overflow` fields enable overflow detection.
When `cur_overflow != prev_overflow`, samples were lost.

#### Channel Count Constraint

```
channel_count == 2 || channel_count == 4
```

This assertion in the binary suggests that digital channels are always
captured in groups of 2 or 4 (likely corresponding to the FPGA's data
bus width or the ADC channel grouping).

#### Sample Count Alignment

```
sample_count % 8 == 0
```

Samples are always aligned to 8-sample boundaries, consistent with
byte-aligned packing of 1-bit digital samples (8 bits = 8 samples).


## Digital Data Encoding

### Interleaved USB Format

The raw USB data from EP2 IN is interleaved -- all enabled channels
(both digital and analog) share the same bulk transfer stream. The
`DisaggregatorNode` splits this into per-channel streams based on the
`DisaggregatorSettingsData` which describes the channel layout.

### Raw Digital Sample Format

For digital channels, each sample is a single bit (high or low). In the
USB stream, digital samples are packed as bytes, with 8 digital samples
per byte. The `sample_count % 8 == 0` constraint confirms byte-aligned
packing.

### RLE Storage Format

After disaggregation, digital data is stored in Run-Length Encoded format
using `Saleae::Digital::Detail::AppendRle(Byte *, int64_t &, SampleCount)`.

The RLE format stores transitions rather than individual samples:
- Each entry records the sample count at which a transition occurs
- `SampleCount` is the type used for run lengths
- `Digital::Byte` is the unit type for the RLE stream

The `DigitalInternals::BuildDataAndLookupTable` function builds an index
over the RLE data for efficient random access.

The `Digital` format string:
```
Digital(first={}, ts={}, ch={}, edge={}, sample_count={}, pulses={} {} {} {} {})
```

Fields:
- `first` -- initial bit state (high or low)
- `ts` -- timestamp
- `ch` -- channel number
- `edge` -- edge direction of first transition
- `sample_count` -- total samples
- `pulses` -- transition timing data (the RLE encoded transitions)

### ModifyFakeData Signature (reveals data format)

```
void LogicDevice::ModifyFakeData(U8*, U32, U64, U32, U32, BitState,
                                  ChunkedArray<U64>*)
void LogicStudentDevice::ModifyFakeData(U32, U8*, U32, U64, U32, U32,
                                         BitState, ChunkedArray<U64>*)
```

Parameters (inferred):
1. `U8*` -- pointer to raw sample buffer
2. `U32` -- buffer size / number of bytes
3. `U64` -- starting sample number
4. `U32` -- samples per transfer / chunk
5. `U32` -- number of channels
6. `BitState` -- initial bit state for the channel
7. `ChunkedArray<U64>*` -- transition timestamps

The `ChunkedArray<U64>` for transition timestamps and `BitState` for
initial state confirms the RLE-style encoding: digital data is
represented as an initial state plus a list of transition timestamps.

### FakeReadThread (reveals data flow)

```
void Logic2FpgaDevice::FakeReadThread(U32, U32)
```

The two U32 parameters are likely (number_of_channels, samples_per_chunk)
or (channel_mask, transfer_size). This function generates synthetic data
in the same format as real hardware, confirming the data format.


## FPGA Register Configuration for Digital Channels

### Digital Configuration Addresses

Two address calculation functions exist for FPGA register mapping:

```
uint16_t Saleae::FpgaRegisters::CalculateDigitalConfigurationAddress_DigitalDerived(int)
uint16_t Saleae::FpgaRegisters::CalculateDigitalConfigurationAddress_SmartCable(int, int)
```

- `DigitalDerived` -- regular digital channel configuration, takes
  channel index
- `SmartCable` -- smart cable digital port configuration, takes
  (port_number, sub_channel) or similar

The register addresses are 16-bit, consistent with the general FPGA
register interface:
```
FPGA ReadRegister 0x{:04X} = 0x{:08X}
FPGA WriteRegister 0x{:04x} = 0x{:08x} ({})
```

### Digital Voltage Thresholds

```
SetDigitalDerivedVoltageThresholds
HandleSetDigitalVoltageThreshold
set digital voltage threshold
/digitalThreshold
/digitalThresholdOptions
```

Digital channels use configurable voltage thresholds (Logic Pro feature).
The threshold determines what voltage level separates logic high from
logic low.


## FIFO Overflow Detection

Three distinct overflow types exist:

```
Digital Fifo overflow
Analog Fifo overflow
Master Fifo overflow
```

These are FPGA-side FIFO overflows that occur when the host cannot read
data fast enough. The overflow state is reported in the per-packet
metadata (`cur_overflow`, `prev_overflow` fields).

### Digital Stream Reset

```
Digital stream reset after receiving {} chunks of digital data
  and {} samples. First 64 bytes of packet:
```

When an error or overflow occurs, the digital stream is reset. The error
handler logs the first 64 bytes of the offending packet for debugging.

### Digital FSM Error

```
digital fsm error
```

This suggests the FPGA implements a state machine for digital capture
that can enter an error state. The FSM likely manages the sample
packing, FIFO management, and start/stop sequencing.


## Sample Rate Configuration

### Digital-Only Mode

```
void DeviceSettings::GetDigitalOnlyPerforamnceOptions(std::vector<SampleRateOption> &) const
void DeviceSettings::SetDigitalOnlyPerformanceOption(const SampleRateOption &)
U32 SampleRateOption::DigitalSampleRateHz() const
```

In digital-only mode (no analog channels enabled), the device can achieve
higher sample rates since the full USB bandwidth is available for digital
data.

### Sample Rate vs Channel Count

```
U64 DeviceSettings::GetInterfaceMaximumBitRate() const
digital sample rate is not valid
attempted to set invalid sample rate
invalid sample rate option
sample rate not found. request: {}, {}. Available rates: {}
```

The maximum sample rate depends on:
1. Number of enabled channels (more channels = lower max rate)
2. USB speed (2.0 vs 3.0)
3. Whether analog channels are also enabled

### Performance Options

Three performance option types:
1. `GetDigitalOnlyPerforamnceOptions` -- digital only (highest rates)
2. `GetAnalogBasicPerformanceOptions` -- basic analog+digital
3. `GetAnalogAdvancedPerformanceOptions` -- advanced analog configurations

Each returns a vector of `SampleRateOption` which combines digital and
analog sample rates.


## Smart Cable Support

Smart cables are external probe accessories that connect via the main
cable and add additional digital channels.

```
SmartCable(port={} vid=0x{:x} pid=0x{:x} bcd={} hwVersion={}
           fwVersion={} plug_count={} orientation={})
SmartCable(port={} disconnected)
EnableSmartCablePorts
GetSmartCableDetails
/smartCablePortCount
port_number < MsoConstants::MaxSmartCablePortCount
```

Smart cable channels have their own FPGA register space:
```
CalculateDigitalConfigurationAddress_SmartCable(int port, int channel)
SetEnabledSmartCableDigitalChannels(int port, std::array<bool, 4>)
```

Each smart cable port provides 4 digital channels.


## Confidence Assessment

| Finding | Confidence | Sources |
|---------|-----------|---------|
| EP2 IN is capture data endpoint | **Confirmed** | lsusb + binary strings |
| Data is interleaved (all channels in one stream) | **High** | DisaggregatorNode in pipeline + VectorData<uint8_t> flow |
| Digital data uses RLE encoding | **Confirmed** | AppendRle function + Digital format string |
| Packets have stats metadata | **High** | SamplesPerPacketMultiple_WithStats assertion |
| Start packet precedes data | **High** | Multiple error handling strings |
| Overflow detection via packet metadata | **High** | cur_overflow/prev_overflow in format string |
| sample_count aligned to 8 | **High** | Assertion string in binary |
| channel_count is 2 or 4 | **High** | Assertion string in binary |
| FPGA-specific packet size limits | **Confirmed** | Altera/Xilinx min/max assertion strings |
| Voltage thresholds configurable | **High** | SetDigitalDerivedVoltageThresholds function |
| Smart cable adds 4 digital channels per port | **High** | SetEnabledSmartCableDigitalChannels signature |
| Digital FSM in FPGA for capture | **Hypothesis** | "digital fsm error" string |
| 8 digital samples per byte | **High** | sample_count % 8 == 0 assertion |


## Open Questions

1. What are the actual numeric values of `FpgaConstants::Min/MaxSamplesPerPacket`
   for Altera and Xilinx? Needs Ghidra analysis to extract constants.

2. What is the exact value of `SamplesPerPacketMultiple_WithStats`?
   This determines packet alignment. Needs constant extraction from binary.

3. How many bytes of stats metadata are appended to each packet?
   The WithStats suffix implies some fixed number of bytes.

4. How is the interleaving structured in the USB bulk transfer?
   Is it sample-interleaved (ch0-s0, ch1-s0, ch0-s1, ...) or
   chunk-interleaved (chunk of ch0 data, chunk of ch1 data)?

5. What is the exact start packet format? The "Got start packet"
   log reveals fields but not the binary layout.

6. What triggers a "digital fsm error" in the FPGA?

7. How does the `DownsampleMode` affect digital data?
   `new_sampling_config.mDownsampleMode != DownsampleMode::MinMax`
   suggests multiple downsampling modes exist.

8. What is the `BitState` enum? Likely just `{Low, High}` or
   `{Zero, One}`.


## New Leads (backlog candidates)

- The `AnalogDeserializer` / `WaveformDeserializer` names suggest analog
  data has a different deserialization path than digital.
- `DigitalCreatorNode` receives both `SamplingConfigurationData` and
  `VectorData<uint8_t>` -- the config must arrive first.
- `LogicStudentDevice::ModifyFakeData` has an extra U32 parameter vs
  `LogicDevice::ModifyFakeData` -- Logic 4 may have a different data
  format.
- The `AnalogCalibrationFilter` node sits in the analog pipeline but
  not the digital pipeline -- digital data needs no calibration.
- `ExportNode` and `ExportActions::ExportData` handle data export,
  which could reveal the exact binary format by examining the export
  format code.


## Source Code References (from embedded paths)

```
/workspace/monorepo/graph-io/core/digital/include/digital.h
/workspace/monorepo/graph-io/core/digital/src/digital.cpp
/workspace/monorepo/graph-io/core/signal_generator/src/signal_generator/digital_generators.cpp
/workspace/monorepo/graph-io/nodes/digital_measurer/src/digital_measurer.cpp
/workspace/monorepo/graph-io/nodes/digital_renderer/src/digital_renderer.cpp
/workspace/monorepo/graph-io/nodes/digital_store/src/digital_store.cpp
/workspace/monorepo/graph-io/nodes/digital_trigger_node/src/digital_trigger_node.cpp
/workspace/monorepo/graph-io/nodes/logic_device_node/src/logic_device_node/logic_device_node.cpp
/workspace/monorepo/graph-io/devices/logic_interface/src/DeviceSettings.cpp
/workspace/monorepo/graph-io/devices/logic_interface/src/Logic2FpgaDevice.cpp
/workspace/monorepo/graph-io/devices/logic_interface/src/LogicGraduateDevice.cpp
/workspace/monorepo/graph-io/devices/logic_interface/src/LogicProfessionalDevice.cpp
/workspace/monorepo/graph-io/devices/logic_interface/src/LogicStudentDevice.cpp
/workspace/monorepo/build/graph-io/linux-x64/release/core/saleae_python/site-packages/saleae/data/digital_data.pyx.cpp
```
