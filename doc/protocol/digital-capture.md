# Digital Capture Streaming Protocol

Status: High confidence (static analysis of libgraph_server_shared.so v2.4.40)
Corroboration: Multiple independent evidence sources (string analysis,
RTTI/mangled symbols, assertion messages, source file paths).
Partially validated: EP2 transport layer, FIFO overflow, and sample
packing confirmed via USB pcap and Rust driver implementation.


## Overview

Digital capture data flows from the FPGA through the FX3 USB controller to
the host over EP2 IN (0x82) bulk transfers. The data stream is interleaved
(all enabled channels -- digital and analog -- share one endpoint). Host-side
software disaggregates the stream into per-channel data. Digital samples are
then stored using Run-Length Encoding (RLE).

```
FPGA -> FX3 EP2 IN -> TransferQueue -> OnDataBuffer
  -> DisaggregatorNode -> DigitalCreatorNode -> DigitalStore (RLE)
```


## USB Transport Layer

### Endpoint

- **EP2 IN** (0x82): Bulk, wMaxPacketSize=1024, bMaxBurst=15 (USB 3.0)
- Theoretical peak: 16 x 1024 = 16,384 bytes per service interval
- Async transfers via `TransferQueue` with dynamic queue sizing

### Transfer Size

`LogicDevice::ComputeTransferSize(U64)` computes the optimal buffer size.
It is virtual, allowing device-specific overrides (Logic 4, Logic 8,
Logic Pro 8/16 each have different implementations).

The `SALEAE_USB_TRANSFER_LIMIT` environment variable can cap the size.

### Transfer Queue

- Managed by `ReadStream_QueueBuffersThread`
- Completion callback: `on_libusb_transfer_complete`
- Dynamic queue sizing: `queue_size < queue_capacity`
- Uses `libusb_dev_mem_alloc` for zero-copy DMA buffers when available
- Uses `libusb_alloc_streams` for USB 3.0 bulk streams


## Interleaved Data Format

All enabled channels share EP2 IN. The `DisaggregatorNode` receives
`DisaggregatorSettingsData` (channel layout description) and splits the
interleaved byte stream into per-channel data.

Output types per channel type:
- Digital channels: `VectorData<uint8_t>` then `DigitalData`
- Analog channels: `VectorData<int16_t>` (12-bit ADC in 16-bit container)


## Packet Structure

### Start Packet

Before data flows, a start packet establishes the stream:
```
ch={}, timestamp={}, time={}, config_id={}
```

Fields:
- `ch` -- channel number
- `timestamp` -- device-side timestamp (FPGA clock)
- `time` -- host-side time
- `config_id` -- configuration identifier for stream synchronization

The DigitalCreatorNode rejects data that arrives without a preceding
start packet:
```
"Received digital data without ever receiving a start packet"
```

### Data Chunks

Each chunk contains a fixed number of samples (`mSamplesPerChunk`)
aligned to `FpgaConstants::SamplesPerPacketMultiple_WithStats`.

Per-chunk metadata:
```
id={}, start={}, end={}, sample_count={}, config_id={}, ts={},
  software={}, cur_overflow={}, prev_overflow={}, samples={}
```

Fields:
- `id` -- sequential chunk/packet ID
- `start`, `end` -- sample index range
- `sample_count` -- number of samples (always multiple of 8)
- `config_id` -- must match start packet
- `ts` -- timestamp
- `software` -- software-generated data flag (simulation)
- `cur_overflow` -- current FIFO overflow counter
- `prev_overflow` -- previous overflow counter


## Packet Size Constraints

Samples per chunk must satisfy all of:

```
mSamplesPerChunk >= FpgaConstants::MinSamplesPerPacket_{Altera,Xilinx}
mSamplesPerChunk <= FpgaConstants::MaxSamplesPerPacket_{Altera,Xilinx}
mSamplesPerChunk % FpgaConstants::SamplesPerPacketMultiple_WithStats == 0
```

The different min/max values for Altera vs Xilinx confirm that the FPGA
type affects the packet format. The WithStats multiple alignment reserves
space at the end of each packet for overflow counters and other metadata.

Pre/post trigger lengths must also be aligned:
```
(mPreTriggerLength + mPostTriggerLength)
    % SamplesPerPacketMultiple_WithStats == 0
```


## Digital Sample Encoding

### Raw USB Format

Digital samples are packed 8 per byte (1 bit per sample). The constraint
`sample_count % 8 == 0` confirms byte-aligned packing.

Channel count is constrained to 2 or 4: `channel_count == 2 || channel_count == 4`

### Host-Side RLE Storage

After disaggregation, digital data is converted to RLE format:

```
void Saleae::Digital::Detail::AppendRle(
    Saleae::Digital::Byte *, int64_t &, Saleae::Graph::SampleCount)
```

The RLE representation consists of:
- An initial `BitState` (High or Low)
- A sequence of `SampleCount` values indicating transition points
- A lookup table for efficient random access
  (`BuildDataAndLookupTable`)

The display format:
```
Digital(first={}, ts={}, ch={}, edge={}, sample_count={},
        pulses={} {} {} {} {})
```

- `first` -- initial bit state
- `edge` -- first transition direction
- `pulses` -- transition timing data (the RLE content)


## Overflow Detection

Three FIFO overflow types are detected:

| Overflow | Source |
|----------|--------|
| `Digital Fifo overflow` | Digital channel FIFO in FPGA |
| `Analog Fifo overflow` | Analog channel FIFO in FPGA |
| `Master Fifo overflow` | Master FIFO (pre-disaggregation) |

Overflow is detected by comparing `cur_overflow != prev_overflow` in
packet metadata. When overflow occurs, samples are lost.

Recovery:
```
Digital stream reset after receiving {} chunks of digital data
  and {} samples. First 64 bytes of packet:
```

The first 64 bytes of the offending packet are logged for debugging.

An FPGA-side state machine error can also occur:
```
digital fsm error
```


## FPGA Register Interface

### Digital Channel Configuration

Two register address calculation functions:

```
CalculateDigitalConfigurationAddress_DigitalDerived(int channel_index)
    -> uint16_t register_address

CalculateDigitalConfigurationAddress_SmartCable(int port, int sub_channel)
    -> uint16_t register_address
```

### Voltage Thresholds

Digital voltage thresholds (the level that separates logic high from
logic low) are configurable per channel:

```
SetDigitalDerivedVoltageThresholds
/digitalThreshold
/digitalThresholdOptions
```


## Capture Configuration (IPC)

From the Electron IPC layer, capture is configured via:

```json
{
  "digitalSampleRate": <int>,
  "digitalChannels": [<int>, ...],
  "digitalThresholdVolts": <float>
}
```

Sample rate options are returned by `DeviceSettings` methods:
- `GetDigitalOnlyPerforamnceOptions` -- digital-only (highest rates)
- `GetAnalogBasicPerformanceOptions` -- basic analog+digital
- `GetAnalogAdvancedPerformanceOptions` -- advanced multi-rate


## Device-Specific Implementations

| Device | Class | StartSampling | ModifyFakeData |
|--------|-------|--------------|----------------|
| Logic 4 | LogicStudentDevice | Custom | Extra U32 param |
| Logic 8 | LogicGraduateDevice | Custom | Base class |
| Logic Pro 8 | LogicProfessional8Device | Shared Pro | Base class |
| Logic Pro 16 | LogicProfessional16Device | Shared Pro | Base class |
| Logic 16 (legacy) | Logic16Device | Legacy | Separate impl |

The `LogicStudentDevice::ModifyFakeData` has an additional `U32` parameter
compared to `LogicDevice::ModifyFakeData`, suggesting the Logic 4 has a
slightly different data format (possibly different channel packing).


## Confidence Assessment

| Finding | Confidence | Sources |
|---------|-----------|---------|
| EP2 IN for sample data stream | **Confirmed** | lsusb + binary analysis + hardware-tested |
| Interleaved digital+analog on EP2 IN | **High** | Mangled symbols + format strings (Ghidra only) |
| DisaggregatorNode splits stream | **High** | RTTI + pipe handler registration (Ghidra only) |
| 1-bit-per-sample packing, 8-aligned | **Confirmed** | Assertion strings + pcap validation + Rust driver |
| Channel count constrained to 2 or 4 | **High** | Assertion string in binary (Ghidra only) |
| RLE storage via AppendRle | **High** | Mangled function signatures (Ghidra only) |
| FIFO overflow detection mechanism | **Confirmed** | String analysis + pcap validation |
| Packet size constraints (FpgaConstants) | **High** | CMP immediates (Ghidra), see `doc/protocol/capture-constants.md` |
| 5 device-specific StartSampling variants | **High** | Virtual function signature strings (Ghidra only) |


## Smart Cable Digital Channels

Smart cables add external digital channels via probe accessories:

- Each port provides 4 digital channels
- Configuration via `SetEnabledSmartCableDigitalChannels(int port, std::array<bool, 4>)`
- Separate FPGA register space per port
- Smart cable metadata includes VID, PID, hardware/firmware versions


## Cross-References

- `doc/protocol/overflow-handling.md` -- overflow metadata format in EP2 stats stream
- `doc/protocol/capture-constants.md` -- packet size constraints (FpgaConstants)
- `doc/protocol/capture-sequence.md` -- capture start/stop sequence
- `doc/protocol/fpga-registers.md` -- digital channel config registers
- `doc/protocol/ep1-commands.md` -- EP1 command protocol
- `crates/saleae-proto/src/disaggregator.rs` -- EP2 stream disaggregation implementation
