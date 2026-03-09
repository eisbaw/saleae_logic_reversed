# Analog Capture Pipeline Analysis

Analysis date: 2026-03-09
Binary: libgraph_server_shared.so (48MB, stripped, ELF 64-bit x86-64)
Source: Saleae Logic 2 v2.4.40


## Summary

The analog capture pipeline has two distinct paths depending on device type:

1. **Logic devices (Logic 4/8/Pro 8/Pro 16)**: ADC samples arrive as
   `VectorData<int16_t>` (12-bit values in 16-bit containers) through the
   same USB EP2 IN bulk transfer stream as digital data. The
   `DisaggregatorNode` separates analog from digital channels. Samples
   flow through `AnalogCalibrationFilter` (FIR filter application), then
   to `AnalogCreatorNode` -> `AnalogStore`.

2. **MSO devices**: Waveform data arrives through a separate path via
   `MsoOscilloscopeInstrument::HandleWaveformData`, producing
   `VectorRawWaveformData` that flows through `WaveformStoreNode`.
   MSO devices are out of scope per the PRD but share some ADC/AFE code.


## ADC Hardware by Device

Three ADC families are used across different hardware revisions:

### HMCAD1100 (Analog Devices / Hittite)

- **Used in**: Logic 4, Logic 8 (older Xilinx Spartan-6 hardware revisions)
- **Type**: 8-bit, quad-channel pipeline ADC
- **Modes**: 1x1 GSPS, 2x500 MSPS, 4x250 MSPS, 8x125 MSPS
- **Interface**: Parallel LVDS to FPGA
- **Key functions**:
  - `Logic2FpgaDevice::InitHmcad1100(Hmcad1100Settings)` -- full init sequence
  - `Logic2FpgaDevice::SetTestPattenHmcad1100(Hmcad1100Settings::AdcTestPattern)`
  - `Hmcad1100Settings::SetDefault(::DeviceType)` -- per-device defaults
- **Key string**: `"invalid ADC data, exceedes 8 bits."` -- confirms 8-bit
  ADC data range validation
- **Evidence**: Only referenced from `Logic2FpgaDevice` (the base class for
  FX3/FPGA devices), not from any Pro-specific code path
- **ADC gain**: `/hittiteAdcGainRegisterValue` -- Hittite is the former name
  of Analog Devices' HMCAD line

### AD9637 (Analog Devices)

- **Used in**: Logic Pro 8, Logic Pro 16 (older Xilinx hardware revisions)
- **Type**: 12-bit, dual-channel, 80 MSPS
- **Interface**: LVDS to FPGA
- **Key functions**:
  - `Logic2FpgaDevice::SetAd9637Enabled(bool)` -- enable/disable
  - `Logic2FpgaDevice::WriteAd9637Register(U16, U16)` -- SPI register write
  - Board revision check: `mBoardRevision.mAdcType == RevisionDetail::AdcType::AD9637`
- **Evidence**: Separate code path from HMCAD; selected via `RevisionDetail::AdcType`
- **String**: `"Unsupported board revision for Ad9637"`

### ADS528x (Texas Instruments ADS5282/ADS5287)

- **Used in**: All devices on newer Lattice ECP5 hardware revisions
- **Type**: 12-bit, 8-channel pipeline ADC
- **Interface**: JESD204B serial to FPGA (via Lattice ECP5 SERDES)
- **Key functions**:
  - `SetAds528xEnabled` -- enable ADC
  - `SetAds528xDrive` -- configure drive strength
- **Bitstream evidence**: All 10 Lattice ECP5 bitstreams have `ADS528x` in
  the design name (`Logic8_Top_ADS528x`, `Logic_Pro_Top_ADS528x`,
  `Logic_Pro16_Top_ADS528x`)
- **JESD link management**:
  - `"FPGA JESD lock=0x{:x} synced=0x{:x} error=0x{:x} power_good={} init_done={} sync_err={}, lane_0-3_pll={}, lane_4-7_pll={}, smpl_pll={}"`
  - `"JESD reset started, waiting for lock..."`
  - `"Locked JESD after {} ms, {} retries"`
  - `"Failed to reset JESD transceivers"`
  - `"bitslip detected after enabling Ads528x. ADC1: {} ADC2: {}. Attempt {} of {}"`
  - `"bitslip error detected after SERDES reset. attempt {}. ADC0: {}, ADC1: {}"`
  - `"Ads528x initialization - bitslip would not lock after multiple attempts."`
- **Note**: The string `"ADC 1 bitslip detected"` and `"ADC 2 bitslip detected"`
  suggest two physical ADC chips (or two ADC banks within one chip)

### ADC Selection Logic

The ADC type is determined by hardware revision:
```
RevisionDetail {
    AdcType: {HMCAD1100, AD9637}
    UsesNewCryptoInterface: bool
    // ... other fields
}
```

For Lattice ECP5 devices, the ADS528x is always used (the bitstream design
inherently determines the ADC interface). For Xilinx devices, the ADC type
is selected via `RevisionDetail::AdcType`.


## ADC Data Format

### Raw ADC Data in USB Stream

From the template instantiations in the binary:

- Logic devices emit `VectorData<int16_t>` (template param `s` = `short`)
  for analog channels after disaggregation
- The `AnalogCreatorNode::Initialize` registers a pipe handler for
  `VectorData<int16_t>` (mangled as `VectorDataIsE`)
- `AnalogDataCollection::Reader::Iterator::AdvanceToNextValueLessThan(short)`
  and `AdvanceToNextValueGreaterThan(short)` -- the search functions use
  `short` (int16_t), confirming the internal sample type

### ADC Bit Width

- **HMCAD1100 devices**: 8-bit ADC, but data is stored as `int16_t`
  - String: `"invalid ADC data, exceedes 8 bits."` -- runtime validation
  - Logic 4: 8-bit unsigned, zero-extended to int16_t, range [0, 255]
  - Logic 8: 8-bit values normalized to 12-bit signed range [-2048, 2047]
    (see re/notes/adc-encoding.md for details)
- **AD9637 devices**: 12-bit ADC, right-justified two's complement in int16_t
  - Range: [-2048, 2047], bits [11:0] with sign extension in [15:12]
- **ADS528x devices**: 12-bit ADC, right-justified two's complement in int16_t
  - Same encoding as AD9637

### AcquisitionInfo

The Python API exposes ADC range:
```python
AcquisitionInfo.__init__(self, min_adc_code: Optional[int], max_adc_code: Optional[int])
```

Fields:
- `/minAdcCode` -- minimum ADC value in acquisition
- `/maxAdcCode` -- maximum ADC value in acquisition

This tells us the data retains raw ADC codes (not immediately converted
to voltages) through the storage layer.

### ADC-to-Voltage Transform

The transform from ADC codes to voltages uses a linear transform:
```
voltage = adc_code * scale + offset    (confirmed)
```

Confirmed by JS bundle `apply1DTransform = (A, e) => A * e.m + e.b`.

Evidence:
- `cal.mAdcToVoltageTransform.mScale != 0.0` -- assertion, scale must be nonzero
- `isfinite( hw_channel.mAdcToVoltageTransform.mScale )` -- validated
- `cal.mAdcToDacVoltageScale != 0.0` -- separate DAC voltage scale
- `/adcToVoltageTransform` -- JSON field name
- `/adcTransform` -- another transform field
- `MissingVoltageTransform` -- error when transform data is absent
- `VoltageTransform` -- the transform type name

See re/notes/adc-encoding.md for full transform details and per-device defaults.

### Two Python APIs for Analog Data

Two distinct Cython modules exist:
1. `saleae.data.analog_span_adc` -- for ADC-based data (Logic devices)
   - `Cython_MakeAnalogSpanFromAnalogReader` -- constructs from AnalogDataCollection
2. `saleae.data.analog_span_wdc` -- for waveform data collection (MSO devices)
   - `Cython_MakeAnalogSpanFromWaveformReader` -- constructs from WaveformDataCollection

Both implement the same `AnalogSpan` API with identical method signatures
(`_adc_to_voltage`, `find_ge`, `find_gt`, `find_le`, `find_lt`, `histogram`,
`raw_chunks`, etc.), but wrap different underlying data stores.


## Analog Data Pipeline Architecture

### Logic Device Pipeline (Primary)

```
FPGA -> FX3 EP2 IN -> TransferQueue -> OnDataBuffer
  -> DisaggregatorNode
       |
       +-> VectorData<int16_t> per analog channel
             |
             v
       AnalogCalibrationFilter  <-- SetCalibration, SetChannel actions
             |  receives: VectorData<int16_t>, SamplingConfigurationData
             |  applies: FIR filter coefficients from calibration data
             v
       AnalogCreatorNode
             |  receives: VectorData<int16_t> (after calibration)
             |  creates: AnalogData objects
             v
       AnalogDownsamplerNode  <-- SetAnalogDownsampleRate action
             |  receives: AnalogData
             |  decimates based on downsampleRatio and downsampleMode
             v
       AnalogStore  <-- SetDiscardData, StartSerialization,
             |         StartDeserialization actions
             |  stores: SampleData<AnalogData>
             |  receives: SamplingConfigurationData, PipeDone
             v
       AnalogTriggerNode  <-- AnalogTriggerSettingsData
             |  receives: AnalogData, SamplingConfigurationData
             |  detects: trigger conditions on analog data
             v
       AnalogRenderer  <-- RenderRequestData
             |  produces: AnalogRenderData
             v
       RenderingManagerNode
```

### MSO Waveform Pipeline (Out of Scope)

```
MsoDeviceInterface -> HandleWaveformData
  -> MsoOscilloscopeInstrument::HandleWaveformData
       |  parses: MsoDataHeader + SharedSpanList
       v
  VectorRawWaveformData
       |
       v
  WaveformStoreNode -> WaveformDataCollection
       |
       v
  WaveformRendererNode -> WaveformSummaryNode
```


## DisaggregatorNode Details

The `DisaggregatorNode` is the critical demuxing stage:

- **Input**: `OwnedSpan<uint8_t>` (raw USB bytes), `DisaggregatorSettingsData`
  (channel layout), `CaptureCompleteData` (end signal)
- **Output per channel type**:
  - Digital channels: `OwnedSpan<uint8_t>` then -> `DigitalData`
  - Analog channels: `VectorData<int16_t>`
- **Configuration**: `DisaggregatorSettingsData` describes how the
  interleaved byte stream maps to channels
- **Glitch filters**: `ConfigureGlitchFilters` action can configure
  per-channel digital glitch filtering within the disaggregator
- **Error**: `"DisaggregatorNode requires DisaggregatorSettingsData before any other data"`

The interleaving format is not yet known in detail. It could be:
- Sample-interleaved: `[ch0_s0, ch1_s0, ch2_s0, ..., ch0_s1, ...]`
- Block-interleaved: `[block_ch0, block_ch1, block_ch2, ...]`
- Or a more complex format with headers per block

The `DisaggregatorSettingsData` from `LogicDeviceNode::GetDisaggregatorSettings`
likely describes the layout.


## AnalogCalibrationFilter

The calibration filter sits between raw ADC data and the creator node:

- **Input pipes**:
  - `VectorData<int16_t>` -- raw ADC samples
  - `SamplingConfigurationData` -- sample rate info for filter selection
- **Actions**:
  - `SetChannel` -- configure which channel this filter processes
  - `SetCalibration` -- load calibration coefficients
- **Source file**: (no explicit path found, but part of the graph-io nodes)

The calibration filter applies FIR filter coefficients that compensate for
analog front-end (AFE) distortion. The coefficients are per-channel and
per-sample-rate, loaded from calibration data via `SetCalibration`.

This is the boundary between TASK-0012 (raw ADC) and TASK-0013 (calibration).
The raw int16_t ADC data enters the filter; calibrated AnalogData exits.


## Analog Front-End (AFE) Configuration

### Per-Channel AFE Config

Four analog channels are configured (for Pro devices):
```
    0: enabled={}, coupling={}, probe_attenuation={}, afe_config={}
    1: enabled={}, coupling={}, probe_attenuation={}, afe_config={}
    2: enabled={}, coupling={}, probe_attenuation={}, afe_config={}
    3: enabled={}, coupling={}, probe_attenuation={}, afe_config={}
```

### AfeConfig_RawHardware Fields

```
AfeConfig_RawHardware(attn1={}, attn2={}, dac={:.6f}, preamp={},
                       ladder={}, filter={}, pwoer={})
```

Fields:
- `attn1` -- attenuator 1 setting
- `attn2` -- attenuator 2 setting
- `dac` -- DAC voltage (6 decimal places, so a precise analog value)
- `preamp` -- preamplifier setting (likely on/off or gain level)
- `ladder` -- resistor ladder attenuator setting
- `filter` -- analog filter setting (bandwidth limiting?)
- `pwoer` -- typo for "power" -- power enable

Note: The typo `pwoer` is in the actual binary. This could be a useful
fingerprint for identifying the struct in Ghidra.

### AfeConfig_VoltageScale

```
AfeConfig_VoltageScale(min={}, max={})
```

This maps AFE settings to a voltage range. The min/max describe what
voltage range the current AFE configuration can measure.

### AFE Config Numbering

```
int Saleae::AfeConfig_RawHardware::GetConfigNumber() const
static Saleae::AfeConfig_RawHardware Saleae::AfeConfig_RawHardware::ForConfigNumber(int)
```

AFE configurations are numbered, with a fixed set of predefined configs.
This is likely the set of voltage ranges the user can select (e.g.,
+/-10V, +/-5V, +/-2.5V, +/-1V, etc.)

### AFE Metadata Constants

```
i % MsoAfeMetadata::AFE_CONFIG_COUNT == 0
i == MsoAfeMetadata::MAX_AFE_CHANNEL_COUNT * MsoAfeMetadata::AFE_CONFIG_COUNT
```

These assertions reveal:
- `AFE_CONFIG_COUNT` -- number of AFE configurations per channel
- `MAX_AFE_CHANNEL_COUNT` -- maximum analog channels
- The metadata array is `MAX_AFE_CHANNEL_COUNT * AFE_CONFIG_COUNT` entries

### VGA (Variable Gain Amplifier)

The Pro devices use an LMH6518 VGA:
```
Saleae::Vga_LMH6518 Saleae::MsoDeviceInterface::BuildVga(uint8_t)
uint16_t Saleae::EncodeVgaConfig(const Saleae::VgaConfig &)
Updating VGA ch {} gain={} attn={}
```

Source: `/workspace/monorepo/graph-io/devices/mso_device/src/mso_device/component/vga_lmh6518.cpp`

The LMH6518 is a DC-to-900MHz fully differential VGA. This is part of
the analog signal conditioning chain:
```
Input -> Attenuators (attn1, attn2) -> LMH6518 VGA -> ADC
```

### AFE Per-Channel Config

```
    0: attn1={}, attn2={}, vga_gain={}, vga_preamp={}, vga_ladder={},
       dac={} V, min={} V, max={} V
```

This expanded format shows the full per-channel AFE state:
- `attn1`, `attn2` -- two-stage attenuator
- `vga_gain` -- VGA gain setting
- `vga_preamp` -- VGA preamp mode
- `vga_ladder` -- VGA ladder attenuator
- `dac` -- offset DAC voltage
- `min`, `max` -- resulting voltage range

### Coupling

```
/coupling
AcCoupling
DcCoupling
```

Pro devices support AC and DC coupling modes. The coupling type is part
of the per-channel configuration.

### AFE Settling Time

```
AFE Settling Time: {} ns, {} samples
```

After changing AFE settings, there is a settling time before ADC data
is valid. This is measured in both nanoseconds and sample counts.

### Attenuator Transform Fields

```
/attn100Transform
/attn10Transform
```

These suggest attenuation factors of 100x and 10x with associated
calibration transforms.


## Decimation / Downsampling

### DownsampleMode Enum

```
Average = 1
MinMax = 2
```

Two downsampling modes:
1. **Average** (1): Standard decimation with averaging
2. **MinMax** (2): Preserves signal peaks by keeping min and max values
   in each decimation window

The assertion `new_sampling_config.mDownsampleMode != DownsampleMode::MinMax`
in certain code paths suggests MinMax mode has restrictions.

### Downsample Constraints

```
analog_decimation_upper == 0
analog_decimation_upper <= 1
/analogLimit_MaxDownsampleRatio
new_sampling_config.mDownsampleRatio == Detail::DownsampleRatioFloor(
    new_sampling_config.mDownsampleMode, new_sampling_config.mDownsampleRatio)
```

- `analog_decimation_upper` must be 0 or 1 in certain contexts
- There is a maximum downsample ratio (`analogLimit_MaxDownsampleRatio`)
- The downsample ratio is quantized by `DownsampleRatioFloor` based on mode
- `"Output sample rate cannot be _greater_ than input sample rate"`
- `"Output sample rate must be a factor of input sample rate"`
- `"Cannot set downsampler rate after execution has begun"`

### AnalogDownsamplerNode

- Source: `/workspace/monorepo/graph-io/nodes/analog_downsampler_node/src/analog_downsampler_node.cpp`
- Receives: `AnalogData`
- Action: `SetAnalogDownsampleRate`
- Error: `"Received analog data without a downsampler"` and
  `"Received sampling config with a downsampler already initialized"`


## Sample Rate Configuration

### Performance Option Types

Three tiers of sample rate configuration:

1. **Digital-only**: `GetDigitalOnlyPerforamnceOptions` (note typo in binary)
   - Highest sample rates, no analog channels enabled
2. **Analog Basic**: `GetAnalogBasicPerformanceOptions`
   - Standard analog+digital configuration
   - Returns `vector<SampleRateOption>`
3. **Analog Advanced**: `GetAnalogAdvancedPerformanceOptions`
   - Multi-rate configurations (different rates per channel?)
   - Returns `vector<vector<SampleRateOption>>` (nested vectors)

### SampleRateOption

```
U32 SampleRateOption::AnalogPhysicalSampleRateHz() const
U32 SampleRateOption::DigitalSampleRateHz() const
```

Each `SampleRateOption` specifies both analog and digital sample rates.
The analog "physical" rate is the actual ADC sampling rate; the
displayed/output rate may be lower due to downsampling.

### Analog-Specific Limits

```
/analogSampleRate
/analogLimit_MaxDownsampleRatio
/analogLimit_MaxHoldoffSamples
/analogLimit_MaxTimeoutSamples
/analogLimit_MaxTotalTriggerBufferLength
/analogLimit_MaxTriggerBufferIndex
/analogLimit_MinTotalTriggerBufferLength
/analogLimit_MinTriggerBufferIndex
/maxAnalogSamplesPerPacket
/minAnalogSamplesPerPacket
```

The `/analogLimit_*` fields are JSON field names in a device constants
or capabilities response. They define the analog-specific constraints.

### Analog Packet Size Constraints

```
/maxAnalogSamplesPerPacket
/minAnalogSamplesPerPacket
```

These are separate from the digital `SamplesPerPacketMultiple_WithStats`
constraints, suggesting analog and digital data may have different
packet sizes within the interleaved stream.


## Analog Statistics

### Per-Packet Stats

```
offset_into_stats_bytes < data.mAnalogStats.size_bytes()
data.mAnalogStats.size() > 0
```

Analog data chunks include statistics metadata (separate from sample
data). The `mAnalogStats` field has both `.size()` (count) and
`.size_bytes()` (byte size), suggesting a vector of fixed-size stat
entries.

The `SamplesPerPacketMultiple_WithStats` alignment requirement applies
to both analog and digital packets.

### Stats in Pipeline

The `stats={}` field in `"Setting sampling config, downsample={}, pre={}, post={}, stats={}, chunk_size={}"` controls whether stats metadata is
included in packets.


## Analog Trigger

### Configuration

```
SetAnalogTriggerConfiguration
DisableAnalogTrigger
trigger_config.mChannelSource < mDeviceConstants.mAnalogChannelCount
```

Analog triggers are software-only (host-side), matching digital triggers.
The trigger source must reference a valid analog channel.

### Trigger Settings

```
AnalogTriggerSettingsData
AnalogTriggerResultData
AnalogTriggerNodeState2
```

The trigger node receives settings and produces results. It processes
`AnalogData` and `SamplingConfigurationData` to detect trigger conditions.


## Channel Configuration

### Channel Count

```
( !ch.mAnalogSettings.mEnabled && !ch.mDigitalSettings.mEnabled )
    || mDeviceConstants.mAnalogChannelCount > i
/analogChannelCount
/analogChannels
/afeChannels
```

Each channel has both analog and digital settings. The `mAnalogChannelCount`
from `MsoDeviceConstants` limits the maximum analog channels.

### AnalogChannelConfigToHardwareConfig

```
static Saleae::ChannelHardwareConfiguration
    Saleae::MsoOscilloscopeInstrument::AnalogChannelConfigToHardwareConfig(
        const Saleae::MsoDeviceConstants &,
        double, double, double,
        Saleae::MsoChannelCalibration)
```

This function translates user-facing configuration (voltage range, offset)
to hardware register values. The three doubles are likely:
- Voltage range (V/div or total range)
- Voltage offset
- Another parameter (bandwidth limit? or scale factor)

The `MsoChannelCalibration` input means calibration data affects the
hardware configuration.


## Environment Variables

- `SALEAE_ANALOG_TEST_PATTERN` -- Enable analog ADC test pattern mode
- `SALEAE_MSO_ADC_1V` -- Force ADC 1V mode (MSO specific)
- `SALEAE_MSO_CALIBRATION_PATH` -- Override calibration data path


## Source Code Organization

Analog pipeline source files:

```
devices/logic_interface/src/
    AnalogSimulationGenerator.cpp    -- Analog test data generation
    Logic2FpgaDevice.cpp             -- ADC init (HMCAD1100, AD9637)
    LogicProfessionalDevice.cpp      -- Pro-specific analog config
    DeviceSettings.cpp               -- Sample rate options

devices/mso_device/src/mso_device/component/
    adc_12qj1600.cpp                 -- MSO ADC component
    vga_lmh6518.cpp                  -- VGA component

core/analog_data/src/
    analog_data_collection.cpp       -- AnalogData storage/retrieval

core/waveform_data/src/waveform_data/
    waveform_data_collection.cpp     -- Waveform (MSO) data storage

core/saleae_python/site-packages/saleae/data/
    analog_span.pyx                  -- Python API for analog data
    analog_span_adc.pyx.cpp          -- Cython: ADC-based analog data
    analog_span_wdc.pyx.cpp          -- Cython: waveform-based analog data

nodes/analog_downsampler_node/src/
    analog_downsampler_node.cpp      -- Decimation pipeline

nodes/analog_measurer/src/
    analog_measurer.cpp              -- Analog measurements

nodes/analog_renderer/src/
    analog_renderer.cpp              -- Analog rendering

nodes/analog_store/src/
    analog_store.cpp                 -- Analog data storage

nodes/waveform_renderer_node/src/waveform_renderer_node/
    waveform_render.cpp              -- Waveform rendering
    waveform_renderer_node.cpp

nodes/waveform_store_node/src/waveform_store_node/
    waveform_store_node.cpp          -- Waveform storage

nodes/waveform_summary_node/src/waveform_summary_node/
    waveform_summary_node.cpp        -- Waveform summary

core/measurements/src/measurements/
    measurer.cpp                     -- Measurement engine
```


## Open Questions

1. **Exact interleaving format**: How are analog and digital samples
   interleaved in the USB bulk transfer? Is it sample-interleaved,
   block-interleaved, or header+payload blocks? The
   `DisaggregatorSettingsData` struct layout would reveal this.

2. **ADC data encoding**: RESOLVED -- 12-bit values are right-justified
   two's complement in the int16_t (bits [11:0], sign-extended in [15:12]).
   Range is [-2048, 2047]. See re/notes/adc-encoding.md.

3. **HMCAD1100 multi-channel modes**: The HMCAD1100 can operate as
   1/2/4/8 channels with different sample rates. How does Saleae select
   the operating mode based on the number of enabled channels?

4. **Exact AFE config numbers**: What voltage ranges do the numbered
   AFE configurations correspond to? `AfeConfig_RawHardware::ForConfigNumber(int)`
   maps config number to hardware settings.

5. **ADS528x register configuration**: What SPI registers are written
   during `GetAdcRegisterConfiguration(MsoDeviceConstants)`? These would
   reveal the ADC's operating mode.

6. **Analog packet size values**: What are the actual numeric values of
   `/minAnalogSamplesPerPacket` and `/maxAnalogSamplesPerPacket`?

7. **AnalogStats format**: What exactly is in the per-packet stats
   for analog data? Min/max/average? Overflow count?

8. **AnalogDeserializer vs DisaggregatorNode**: Is `AnalogDeserializer`
   the same as the disaggregator's analog output path, or is it a
   separate legacy component? The string exists but was not found as
   a class in RTTI.

9. **MeasurerV1 vs MeasurerV2**: What differs between these two
   measurement versions? API version string suggests V2 is for newer
   extension APIs: `"API version {} is required for
   saleae.measurements.analog_measurer.AnalogMeasurer"`.


## New Leads (Backlog Candidates)

- The `FullScaleKernel` and `VerticalScaleKernel` implement auto-scale
  logic. Tracing these would reveal how the AFE is automatically
  configured to match the input signal.

- `MsoAfeBoardPartNumber` (enum with values like `MsoAfe_2ch_9b`,
  `MsoAfe_2ch_12b`, `MsoAfe_4ch_9b`, `MsoAfe_4ch_12b`) reveals that
  MSO devices come in 2-channel/4-channel and 9-bit/12-bit variants.
  The part numbers are: `913'00004`, `913'00006`, `913'00007`, `913'00008`.

- The `CalibrationToMetadataConstants` and `MetadataConstantsToCalibration`
  functions convert between the on-device calibration format and the
  host-side representation. These are key to TASK-0013.

- `JesdStatus` format string reveals all the JESD204B status fields:
  `"JesdStatus(link_up={}, sync_status={}, realigned={}, aligned={}, spll_locked={}, cpll_locked={})"`

- The IO Expander functions (`BuildIoExpander_DcVgaAdcDac`,
  `BuildIoExpander_Attenuation`, `BuildIoExpander_SmartCableA/B`)
  reveal the I2C GPIO expander layout for controlling the AFE hardware.
