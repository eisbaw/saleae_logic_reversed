# Analog Downsampler Node: Decimation Algorithm

Reverse engineered from `libgraph_server_shared.so` (Saleae Logic 2 v2.4.40).

Source path leaked via assertions:
- `/workspace/monorepo/graph-io/nodes/analog_downsampler_node/src/analog_downsampler_node.cpp`
- Related: `/workspace/monorepo/graph-io/nodes/waveform_summary_node/src/waveform_summary_node/waveform_summary_node.cpp`

## Summary

The AnalogDownsamplerNode performs **simple integer-stride decimation** (every-Nth-sample
selection). It does NOT implement anti-aliasing filtering, averaging, or min/max
summarization. Those operations occur in separate nodes (calibration filter upstream,
WaveformSummaryNode downstream for display).

Confidence: HIGH (decompiled code clearly shows stride-based sample selection)

## Class Hierarchy

```
Saleae::Graph::Kernel
  -> Saleae::Graph::ConvenienceKernel
    -> Saleae::Graph::AnalogDownsamplerNode  (@ Ghidra 01bc8380)
```

Registered via `KernelFactory::AddKernel<AnalogDownsamplerNode>`.

## DownsampleMode Enum

```cpp
namespace Saleae::DownsampleMode_impl {
    enum DownsampleMode : uint32_t {
        Average = 1,   // string at 0x3bd8c1
        MinMax  = 2,   // string at 0x402b8b
    };
}
```

**Important**: Despite the enum having Average and MinMax modes, the
AnalogDownsamplerNode itself only performs stride decimation. The mode
is used for configuration validation and is enforced elsewhere:
- The MSO oscilloscope instrument asserts `mDownsampleMode != DownsampleMode::MinMax`
  (mode 2 is forbidden for MSO analog capture).
- The mode value is written to FPGA register 0x40 as bits [6:5] in the
  SetAnalogChannelConfiguration function (FUN_02e64640, the MSO FPGA config writer).

The enum likely controls FPGA-side behavior for hardware-assisted downsampling,
while the software AnalogDownsamplerNode always does simple stride decimation.

## DownsampleRatioFloor

The `Detail::DownsampleRatioFloor(mode, ratio)` function quantizes the ratio
to valid hardware-compatible values. Reconstructed from the assertion check
in FUN_02e4a280 (MsoOscilloscopeInstrument::ProcessPendingConfiguration):

```
Inputs: mode (uint32_t), ratio (uint32_t)
Output: floor-aligned ratio

if ratio < 2:
    return 1          (no downsampling)
if ratio < 4:
    return 2          (2x)
if ratio < 6:
    return 4          (4x)
if ratio < 8:
    return 6          (6x)
if ratio >= 8:
    aligned = min(ratio >> 3, 0x27AC4) << 3
    return aligned    (multiple of 8, max 162528x = 0x27AC4 * 8)
```

This means valid downsample ratios are: 1, 2, 4, 6, 8, 16, 24, ..., up to
162,528. The pattern is power-of-2 quantization for small values, then
alignment to multiples of 8 for larger values.

The max ratio of 162,528 (0x4F620) at e.g. 500 MSPS input would give a minimum
output rate of approximately 3.07 kSPS.

## SamplingConfiguration Structure (partial)

Serialized via JSON keys `/downsampleMode`, `/downsampleRatio`,
`/preTriggerIndex`, `/postTriggerIndex` (from FUN_02e83360).

```
struct SamplingConfiguration {
    /* +0x00 */ uint32_t  mDownsampleMode;      // DownsampleMode enum
    /* +0x04 */ uint32_t  mDownsampleRatio;      // integer decimation factor
    /* +0x08 */ int64_t   mPreTriggerIndex;       // negative sample count before trigger
    /* +0x10 */ int64_t   mPostTriggerIndex;      // positive sample count after trigger
    // ... other fields follow
};
```

Also serialized under `/analogDownsample` in the export config (FUN_01c15570).
Device limits are published as `/analogLimit_MaxDownsampleRatio` alongside
other MSO constants (FUN_02e77af0).

## AnalogDownsamplerNode Object Layout (partial)

Reconstructed from HandlePipeData (FUN_01bc8640) and HandleRequest (FUN_01bc8f20):

```
offset  type                    field
+0x00   vtable*                 vtable (ConvenienceKernel base)
...     (ConvenienceKernel internals)
+0xD0   uint64_t                mDownsampleConfig (mode << 32 | ratio)
+0xD8   bool                    mConfigReceived
+0xE0   uint64_t                mSampleOffset (position in current input block)
+0xE8   uint64_t                mStride (= downsampling ratio from Create)
+0xF0   bool                    mDownsampler (flag: downsampler is initialized)
```

## Core Algorithm: HandlePipeData (FUN_01bc8640)

The function processes incoming analog data through the graph pipe system.

### Data Flow

1. **Input arrives** as `shared_ptr<const Data>`, dynamic_cast to concrete types
2. **SamplingConfigurationData** arrives first:
   - Asserts `!mDownsampler` (no double-init)
   - Extracts mDownsampleRatio from the config
   - Calls `AnalogDownsampler_Create(input_rate, output_rate)` (FUN_01bc7fd0)
   - Create validates: output_rate <= input_rate, and input_rate % output_rate == 0
   - Stores `stride = input_rate / output_rate` at `this->mStride` (offset +0xE8)
   - Sets `mDownsampler = true` (offset +0xF0)
   - Allocates an empty output `VectorData<int16_t>` and emits it downstream

3. **AnalogData** arrives subsequently:
   - Asserts `mDownsampler` is initialized
   - Gets the input sample buffer: `plVar6[0]` = begin ptr, `plVar6[1]` = end ptr
   - Input data is `int16_t[]` (2 bytes per sample, confirmed by `>> 1` for count, `* 2` for byte offsets)
   - Calculates how many output samples to produce:
     ```
     input_count = (end - begin) / 2  (byte count to sample count)
     remaining_offset = this->mSampleOffset  (from previous chunk)
     output_count = (input_count - remaining_offset + stride - 1) / stride
     new_offset = remaining_offset - input_count  (carry for next chunk)
     ```
   - Allocates output buffer via `FUN_01bc92e0` (vector<int16_t> grow)

### Decimation Kernel (SIMD-optimized)

When `stride == 1` (no decimation, 1:1 copy):
- Uses vectorized 128-bit copy loop (16 bytes = 8 samples per iteration)
- Unrolled 2x (32 bytes per loop body)
- Falls through to scalar for remainder

When `stride > 1` (actual decimation):
- Scalar loop, unrolled 4x:
  ```c
  for (i = 0; i < output_count; i += 4) {
      output[i+0] = input[pos + 0*stride];
      output[i+1] = input[pos + 1*stride];
      output[i+2] = input[pos + 2*stride];
      output[i+3] = input[pos + 3*stride];
      pos += 4 * stride;
  }
  ```
- Processes remaining 1-3 samples in a scalar tail loop

This is simple decimation with NO anti-aliasing filter. The anti-aliasing
is expected to be handled by the calibration FIR filter upstream in the
processing pipeline, or by the FPGA if the mode is set to Average.

### Output Emission

After decimation, the output buffer is wrapped in a new `VectorData<int16_t>`
shared_ptr and emitted downstream via `Kernel::EmitData` (FUN_02d910f0).

## Relationship to Other Nodes

### Pipeline Position

```
[USB Device] -> [Disaggregator] -> [AnalogCalibrationFilter] -> [AnalogDownsamplerNode] -> [AnalogStore]
                                                                                       |-> [AnalogTriggerNode]
```

The downsampler sits AFTER calibration filtering but BEFORE storage. This
means the FIR calibration filter processes full-rate data, and the downsampler
reduces the rate for storage/display.

Evidence: From the graph-io string analysis:
- SamplingConfigurationData is consumed by: AnalogCalibrationFilter,
  AnalogTriggerNode, DigitalTriggerNode, AnalogStore, DigitalStore,
  AnalogDownsamplerNode (FUN_01bc8640)
- AnalogData flows: calibration filter -> downsampler -> store

### WaveformSummaryNode (separate from downsampler)

**Source**: `/workspace/monorepo/graph-io/nodes/waveform_summary_node/src/waveform_summary_node/waveform_summary_node.cpp`

The WaveformSummaryNode handles LOD (level-of-detail) for the display renderer.
It is NOT the same as the AnalogDownsamplerNode.

- Receives `WaveformRenderRequest` from `RenderingManagerNode`
- Produces `WaveformSummaryResult`
- Connected via pipes: `waveform-summary-requests`, `waveform-summary-responses`
- Uses `WaveformSummaryResult` which contains min/max pairs for rendering

The WaveformSummaryNode likely generates the display LOD levels (min/max
envelope pairs for efficient waveform rendering at different zoom levels),
while the AnalogDownsamplerNode handles actual sample rate reduction.

### FPGA-side DownsampleMode

The DownsampleMode enum value is written to the FPGA via register 0x40
(bits [6:5]) in the MSO FPGA configuration. This suggests the FPGA can
perform hardware-assisted downsampling in Average mode, producing
averaged samples directly, while the software downsampler handles the
simpler stride case.

When the FPGA does averaging, the software downsampler stride would be 1
(pass-through), since the FPGA already reduced the rate.

## Constants

From the MSO FPGA config function (FUN_02e64640):

| Constant | Xilinx | Altera |
|----------|--------|--------|
| MinSamplesPerPacket | 0x100 (256) | 0x40 (64) |
| MaxSamplesPerPacket | 0x1E00 (7680) | 0x1780 (6016) |
| MaxTriggerBufferSamples | 0x3FFFC000 | 0x2FFFC000 |
| MinTriggerBufferSamples | 8 | 8 |
| MaxTriggerBufferIndex | 0xBFFF (49151) | 0xBFFF (49151) |
| SamplesPerPacketMultiple_WithStats | varies by mode | varies by mode |

## Key Functions (Ghidra Addresses)

| Address | Name | Description |
|---------|------|-------------|
| 01bc8380 | AnalogDownsamplerNode::ctor | Constructor, calls ConvenienceKernel base |
| 01bc7fd0 | AnalogDownsampler_Create | Validates rates, computes stride = input/output |
| 01bc8640 | HandlePipeData | Main processing: receives config then data |
| 01bc8f20 | HandleRequest | Handles SetAnalogDownsampleRate action |
| 01bc92e0 | Vector_GrowAndZero | Grows int16_t output vector, zeros new elements |
| 02e83360 | SamplingConfig_Deserialize | Reads mode, ratio, pre/post trigger from JSON |
| 02e64640 | MsoFpga_SetAnalogChannelConfig | Writes downsampler config to FPGA registers |
| 02e77af0 | DeviceLimits_Deserialize | Reads MaxDownsampleRatio and other limits |

## Open Questions

1. **FPGA Average mode**: When DownsampleMode::Average is active, does the FPGA
   actually average samples, or does it just do strided selection? The FPGA
   register bits suggest hardware support, but we haven't confirmed the FPGA
   behavior.

2. **Min/Max in WaveformSummaryNode**: The WaveformSummaryNode likely produces
   min/max envelopes for rendering, but we haven't decompiled its core processing
   loop. This would reveal the LOD algorithm.

3. **AnalogData format**: Confirmed as `VectorData<int16_t>` (2-byte signed
   samples), but the exact ADC-to-voltage conversion (gain/offset from
   calibration) is applied upstream in the calibration filter.

4. **Chunk boundary handling**: The `mSampleOffset` field tracks the position
   across chunk boundaries, ensuring correct alignment when the decimation
   stride doesn't evenly divide the chunk size. The exact carry-forward
   arithmetic should be verified.
