# Analog Capture Streaming Protocol

Status: High confidence (static analysis of libgraph_server_shared.so v2.4.40)
Corroboration: Multiple independent evidence sources (string analysis,
RTTI/mangled symbols, assertion messages, source file paths, Lattice
bitstream metadata)
Awaiting: USB capture validation and Ghidra decompilation for exact
constants and interleaving format


## Overview

Analog capture data shares the same USB EP2 IN (0x82) bulk transfer
stream as digital data. The interleaved byte stream is split by the
`DisaggregatorNode` into per-channel data: digital channels produce
`VectorData<uint8_t>`, analog channels produce `VectorData<int16_t>`.

The analog data pipeline applies calibration (FIR filtering), optional
downsampling, and stores samples as `AnalogData` objects containing
raw ADC codes (int16_t). Conversion to voltages happens at display/export
time using a linear `AdcToVoltageTransform`.

```
FPGA -> FX3 EP2 IN -> TransferQueue -> OnDataBuffer
  -> DisaggregatorNode
       |
       +-> VectorData<int16_t> per analog channel
             |
             v
       AnalogCalibrationFilter (FIR filter, per-channel, per-sample-rate)
             |
             v
       AnalogCreatorNode -> AnalogData
             |
             v
       AnalogDownsamplerNode (decimation: Average or MinMax mode)
             |
             v
       AnalogStore (SampleData<AnalogData>)
             |
             +-> AnalogTriggerNode (software trigger detection)
             +-> AnalogRenderer (display)
             +-> AnalogMeasurer (measurements, V1 and V2 APIs)
```


## ADC Hardware

Three ADC families are used, selected by hardware revision:

| ADC | Bit Depth | Devices | FPGA | Interface |
|-----|-----------|---------|------|-----------|
| HMCAD1100 | 8-bit | Logic 4, Logic 8 (old rev) | Xilinx Spartan-6 | Parallel LVDS |
| AD9637 | 12-bit | Logic Pro 8/16 (old rev) | Xilinx Spartan-6/7-Series | Parallel LVDS |
| ADS5282/5287 | 12-bit | All devices (new rev) | Lattice ECP5 | JESD204B serial |

**Confidence: High** -- Three independent Ghidra sources agree (not yet hardware-validated):
1. Mangled function names: `InitHmcad1100`, `SetAd9637Enabled`, `SetAds528xEnabled`
2. Board revision enum: `RevisionDetail::AdcType::{HMCAD1100, AD9637}`
3. Lattice bitstream design names: all contain `ADS528x`

### ADC Initialization

**HMCAD1100** (older Logic 4/8):
- `Logic2FpgaDevice::InitHmcad1100(Hmcad1100Settings)` -- full init
- `Hmcad1100Settings::SetDefault(::DeviceType)` -- per-device defaults
- `SetTestPattenHmcad1100(Hmcad1100Settings::AdcTestPattern)` -- test mode
- ADC register access: `"ADC Read 0x{:x} = 0x{:x}"`, `"ADC Write 0x{:x} = 0x{:x}"`

**AD9637** (older Logic Pro 8/16):
- `Logic2FpgaDevice::SetAd9637Enabled(bool)` -- enable/disable
- `Logic2FpgaDevice::WriteAd9637Register(U16, U16)` -- SPI register write

**ADS528x** (newer Lattice ECP5 devices):
- `SetAds528xEnabled` -- enable ADC
- `SetAds528xDrive` -- drive strength config
- JESD204B link management with retries and bitslip detection

**Confidence: High** -- Function signatures extracted from mangled symbols (Ghidra only)


## Raw ADC Data Format

### Sample Type

Analog samples are stored as `int16_t` (signed 16-bit integer).

**Evidence (Confirmed)**:
- Template instantiation: `VectorData<int16_t>` (mangled as `VectorDataIsE`,
  where `s` = `short`)
- `AnalogCreatorNode` pipe handler receives `VectorData<int16_t>`
- `AdvanceToNextValueLessThan(short)` / `AdvanceToNextValueGreaterThan(short)`
  search on int16_t values
- Python API: `"failed to set base object on Numpy array wrapping an AnalogData chunk."`
  and `"value too large to convert to int16_t"` -- confirms int16_t storage

### Bit Width per ADC

- HMCAD1100: 8-bit ADC data, stored in int16_t
  - Runtime validation: `"invalid ADC data, exceedes 8 bits."` (sic)
- AD9637: 12-bit ADC data, stored in int16_t
- ADS528x: 12-bit ADC data, stored in int16_t

**Confidence: High** -- Assertion strings confirm the validation logic

### ADC Code Range

The `AcquisitionInfo` struct tracks the actual ADC range per acquisition:
- `min_adc_code` -- minimum ADC code value observed
- `max_adc_code` -- maximum ADC code value observed

This means raw ADC codes are preserved through the storage layer.

**Confidence: Confirmed** -- Python API signature with type annotations


## ADC-to-Voltage Transform

Voltage conversion uses a linear transform:

```
voltage = adc_code * scale + offset
```

Evidence:
- `cal.mAdcToVoltageTransform.mScale != 0.0` -- assertion (Ghidra)
- `isfinite(hw_channel.mAdcToVoltageTransform.mScale)` -- validation (Ghidra)
- `cal.mAdcToDacVoltageScale != 0.0` -- DAC voltage scaling (Ghidra)
- JSON fields: `/adcToVoltageTransform`, `/adcTransform`,
  `/voltageTransform`
- Error: `MissingVoltageTransform` when data is absent
- Real .cal file: `mFullScaleMinVoltage`/`mFullScaleMaxVoltage` per channel (validated)
- `doc/calibration/calibration-format.md`: fully documents the transform
  formula `voltage = adc_code * m + b` with real calibration data

**Confidence: Confirmed** -- Ghidra assertions + real .cal file data + Rust implementation


## Analog Front-End (AFE)

### Signal Chain (Logic Pro devices)

```
Input -> Attenuator Stage 1 (attn1) -> Attenuator Stage 2 (attn2)
  -> LMH6518 VGA (preamp + ladder + gain) -> ADC
       ^-- DC offset via DAC
```

### Per-Channel Configuration

Four channels per device (Pro 8 uses channels 0-7 of Pro 16):

```
enabled, coupling (AC/DC), probe_attenuation, afe_config
attn1, attn2, vga_gain, vga_preamp, vga_ladder, dac (V), min (V), max (V)
```

### AFE Config Numbering

- Fixed set of predefined configurations, accessed by config number
- `AfeConfig_RawHardware::ForConfigNumber(int)` -- maps number to settings
- `AfeConfig_RawHardware::GetConfigNumber()` -- reverse mapping
- Config count stored in `MsoAfeMetadata::AFE_CONFIG_COUNT`

### Calibration Integration

- `AfeConfig_VoltageScale(min={}, max={})` -- voltage range per config
- Transform fields: `/attn100Transform`, `/attn10Transform`
- AFE settling time measured: `"AFE Settling Time: {} ns, {} samples"`

**Confidence: Confirmed** -- Format strings with field names from binary


## Decimation / Downsampling

### Downsample Modes

| Mode | Value | Description |
|------|-------|-------------|
| Average | 1 | Standard decimation with sample averaging |
| MinMax | 2 | Preserves peaks by keeping min/max per window |

### Constraints

- `analogLimit_MaxDownsampleRatio` -- maximum decimation ratio
- Output rate must divide input rate evenly
- Rate cannot be set after capture has started
- Ratio quantized by `DownsampleRatioFloor(mode, ratio)`

**Confidence: Confirmed** -- Enum values and assertion strings from binary


## Packet Size Constraints

Analog-specific packet size fields:
- `/maxAnalogSamplesPerPacket` -- maximum analog samples per chunk
- `/minAnalogSamplesPerPacket` -- minimum analog samples per chunk

These are separate from the digital `FpgaConstants::Min/MaxSamplesPerPacket`
values, suggesting analog data may use different packet sizes.

The same `SamplesPerPacketMultiple_WithStats` alignment applies. Per-packet
statistics metadata (`mAnalogStats`) is included when `stats` is enabled
in the sampling configuration.

**Confidence: High** -- JSON field names and assertion strings


## Overflow Detection

```
Analog Fifo overflow
analog fsm error
```

Analog data has its own FPGA FIFO and state machine, separate from
digital. Overflow detection uses the same `cur_overflow`/`prev_overflow`
mechanism as digital packets.

**Confidence: Confirmed** -- Strings present in binary alongside digital
counterparts


## JESD204B Interface (Lattice ECP5 Devices)

Newer hardware revisions use JESD204B serial interface between the
ADS528x ADC and the Lattice ECP5 FPGA:

```
JesdStatus(link_up={}, sync_status={}, realigned={}, aligned={},
           spll_locked={}, cpll_locked={})
```

Status fields:
- `link_up` -- JESD link active
- `sync_status` -- synchronization status
- `realigned` / `aligned` -- lane alignment status
- `spll_locked` / `cpll_locked` -- PLL lock status

Initialization includes:
- PLL and SERDES reset
- JESD link lock with timeout and retry
- Bitslip detection and correction (up to multiple attempts)
- Separate tracking for ADC1 and ADC2 bitslip status

**Confidence: Confirmed** -- Detailed format strings and Lattice
bitstream metadata


## Device-Specific Behavior

| Device | Analog Channels | ADC (old rev) | ADC (new rev) |
|--------|----------------|---------------|---------------|
| Logic 4 | 0 (digital only?) | N/A | N/A |
| Logic 8 | ? | HMCAD1100 (8-bit) | ADS528x (12-bit) |
| Logic Pro 8 | 8 | AD9637 (12-bit) | ADS528x (12-bit) |
| Logic Pro 16 | 16 | AD9637 (12-bit) | ADS528x (12-bit) |

Note: Logic 4 (LogicStudentDevice) may not support analog channels at all.
Logic 8 may have limited analog support compared to Pro devices.

**Confidence: Hypothesis for Logic 4/8 channel counts** -- Needs
USB capture validation


## Environment Variables

| Variable | Purpose |
|----------|---------|
| `SALEAE_ANALOG_TEST_PATTERN` | Enable ADC test pattern mode |
| `SALEAE_MSO_ADC_1V` | Force ADC 1V mode (MSO) |
| `SALEAE_MSO_CALIBRATION_PATH` | Override calibration data path |


## Open Questions

1. Exact interleaving format in the USB bulk transfer (sample-level vs
   block-level interleaving)
2. ADC data justification: RESOLVED -- right-justified, bits [11:0],
   sign-extended in [15:12]. See re/notes/adc-encoding.md.
3. ADC data encoding: RESOLVED -- two's complement, range [-2048, 2047].
   See re/notes/adc-encoding.md.
4. Exact values of minAnalogSamplesPerPacket and maxAnalogSamplesPerPacket
5. AnalogStats format (what fields are in per-packet stats)
6. HMCAD1100 multi-channel mode selection logic
7. Logic 4 and Logic 8 analog channel counts
